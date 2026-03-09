//! Bootstrap helpers for constructing [`super::AppState`].

use super::AppState;
use crate::auth::{
    agent,
    ipc::{self, VaultStatus},
    vault::VaultPaths,
};
use crate::config;
use crate::inventory::{FolderId, InventoryHost, InventoryTreeModel, TreeFolder, get_default_inventory_path, load_inventory_tree};
use crate::{log_debug, log_error, log_warn};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::fs;
use std::sync::mpsc::{self, Receiver};

pub(super) const DEFAULT_TERMINAL_SIZE: (u16, u16) = (100, 30);

pub(super) struct VaultStatusEventWatcher {
    _watcher: RecommendedWatcher,
    receiver: Receiver<()>,
    paths: VaultPaths,
}

impl VaultStatusEventWatcher {
    fn new() -> Option<Self> {
        let paths = match VaultPaths::resolve_default() {
            Ok(paths) => paths,
            Err(err) => {
                log_debug!("Skipping vault status watcher setup: {}", err);
                return None;
            }
        };

        let run_dir = paths.run_dir();
        if let Err(err) = fs::create_dir_all(&run_dir) {
            log_warn!("Failed to create vault run directory for status watcher: {}", err);
            return None;
        }

        let marker_name = match ipc::vault_status_event_file_path(&paths).file_name().and_then(|segment| segment.to_str()) {
            Some(name) => name.to_string(),
            None => {
                log_warn!("Failed to determine vault status marker name");
                return None;
            }
        };

        let (tx, rx) = mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(
            move |res| {
                if let Ok(event) = res
                    && should_forward_vault_status_event(&event, &marker_name)
                {
                    let _ = tx.send(());
                }
            },
            notify::Config::default(),
        ) {
            Ok(watcher) => watcher,
            Err(err) => {
                log_warn!("Vault status watcher disabled: {}", err);
                return None;
            }
        };

        if let Err(err) = watcher.watch(&run_dir, RecursiveMode::NonRecursive) {
            log_warn!("Failed to watch vault run directory '{}': {}", run_dir.display(), err);
            return None;
        }

        Some(Self {
            _watcher: watcher,
            receiver: rx,
            paths,
        })
    }

    pub(super) fn take_pending_paths(&self) -> Option<VaultPaths> {
        let mut saw_event = false;
        while let Ok(()) = self.receiver.try_recv() {
            saw_event = true;
        }

        saw_event.then(|| self.paths.clone())
    }
}

#[derive(Debug, Clone, Copy)]
struct AppStateConfig {
    host_tree_uncollapsed: bool,
    host_info_visible: bool,
    host_view_size_percent: u16,
    info_view_size_percent: u16,
    quick_connect_default_ssh_logging: bool,
}

impl Default for AppStateConfig {
    fn default() -> Self {
        Self {
            host_tree_uncollapsed: false,
            host_info_visible: true,
            host_view_size_percent: 25,
            info_view_size_percent: 40,
            quick_connect_default_ssh_logging: false,
        }
    }
}

impl AppStateConfig {
    fn load() -> Self {
        config::with_current_config("reading interactive session settings", |cfg| {
            let mut session_config = Self {
                quick_connect_default_ssh_logging: cfg.settings.ssh_logging,
                ..Self::default()
            };

            if let Some(interactive) = cfg.interactive_settings.as_ref() {
                session_config.host_tree_uncollapsed = interactive.host_tree_uncollapsed;
                session_config.host_info_visible = interactive.info_view;
                session_config.host_view_size_percent = interactive.host_view_size;
                session_config.info_view_size_percent = interactive.info_view_size;
            }

            session_config
        })
    }
}

pub(super) struct AppStateInit {
    pub(super) hosts: Vec<InventoryHost>,
    pub(super) host_tree_root: TreeFolder,
    pub(super) inventory_load_error: Option<String>,
    pub(super) collapsed_folders: HashSet<FolderId>,
    pub(super) host_panel_width: u16,
    pub(super) host_panel_default_percent: u16,
    pub(super) host_info_height: u16,
    pub(super) host_info_visible: bool,
    pub(super) quick_connect_default_ssh_logging: bool,
    pub(super) last_terminal_size: (u16, u16),
    pub(super) vault_status: VaultStatus,
    pub(super) vault_status_events: Option<VaultStatusEventWatcher>,
}

fn event_targets_vault_status_marker(event: &Event, marker_name: &str) -> bool {
    event.paths.iter().any(|path| {
        path.file_name()
            .and_then(|segment| segment.to_str())
            .map(|name| name == marker_name)
            .unwrap_or(false)
    })
}

fn should_forward_vault_status_event(event: &Event, marker_name: &str) -> bool {
    (event.kind.is_modify() || event.kind.is_create()) && event_targets_vault_status_marker(event, marker_name)
}

fn fallback_host_tree_root() -> TreeFolder {
    TreeFolder {
        id: 0,
        name: "cossh-inventory.yaml".to_string(),
        path: get_default_inventory_path().unwrap_or_else(|| std::path::PathBuf::from("~/.color-ssh/cossh-inventory.yaml")),
        children: Vec::new(),
        host_indices: Vec::new(),
    }
}

fn load_host_tree_model() -> (InventoryTreeModel, Option<String>) {
    match load_inventory_tree() {
        Ok(tree_model) => (tree_model, None),
        Err(err) => {
            log_error!("Failed to load inventory hosts: {}", err);
            (
                InventoryTreeModel {
                    root: fallback_host_tree_root(),
                    hosts: Vec::new(),
                },
                Some(err.to_string()),
            )
        }
    }
}

fn build_collapsed_folders(host_tree_root: &TreeFolder, host_tree_uncollapsed: bool) -> HashSet<FolderId> {
    let mut collapsed_folders = HashSet::new();
    if !host_tree_uncollapsed {
        AppState::collect_descendant_folder_ids(host_tree_root, &mut collapsed_folders);
    }
    collapsed_folders
}

fn compute_host_info_height(term_height: u16, info_view_size_percent: u16) -> u16 {
    let content_height = term_height.saturating_sub(2);
    let mut host_info_height = ((content_height as u32 * info_view_size_percent as u32) / 100) as u16;
    host_info_height = host_info_height.max(3);
    if content_height > 4 {
        host_info_height = host_info_height.min(content_height.saturating_sub(4));
    }
    host_info_height
}

pub(super) fn load_vault_status() -> VaultStatus {
    agent::AgentClient::new()
        .and_then(|client| client.status())
        .unwrap_or_else(|_| VaultStatus::locked(false))
}

pub(super) fn load_app_state_init() -> AppStateInit {
    let (tree_model, inventory_load_error) = load_host_tree_model();
    let session_config = AppStateConfig::load();
    let host_count = tree_model.hosts.len();
    let host_tree_root = tree_model.root;
    let collapsed_folders = build_collapsed_folders(&host_tree_root, session_config.host_tree_uncollapsed);
    let (term_width, term_height) = crossterm::terminal::size().unwrap_or(DEFAULT_TERMINAL_SIZE);
    let host_panel_width =
        AppState::clamp_host_panel_width_for_terminal(((term_width as u32 * session_config.host_view_size_percent as u32) / 100) as u16, term_width);

    log_debug!("Loaded {} inventory hosts", host_count);

    AppStateInit {
        hosts: tree_model.hosts,
        host_tree_root,
        inventory_load_error,
        collapsed_folders,
        host_panel_width,
        host_panel_default_percent: session_config.host_view_size_percent,
        host_info_height: compute_host_info_height(term_height, session_config.info_view_size_percent),
        host_info_visible: session_config.host_info_visible,
        quick_connect_default_ssh_logging: session_config.quick_connect_default_ssh_logging,
        last_terminal_size: (term_width, term_height),
        vault_status: load_vault_status(),
        vault_status_events: VaultStatusEventWatcher::new(),
    }
}

#[cfg(test)]
pub(super) fn test_app_state_init() -> AppStateInit {
    AppStateInit {
        hosts: Vec::new(),
        host_tree_root: fallback_host_tree_root(),
        inventory_load_error: None,
        collapsed_folders: HashSet::new(),
        host_panel_width: 25,
        host_panel_default_percent: 25,
        host_info_height: 10,
        host_info_visible: true,
        vault_status: VaultStatus::locked(false),
        quick_connect_default_ssh_logging: false,
        last_terminal_size: DEFAULT_TERMINAL_SIZE,
        vault_status_events: None,
    }
}
