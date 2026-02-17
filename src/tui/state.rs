//! Core TUI state and initialization.

use crate::ssh_config::{FolderId, SshHost, TreeFolder, load_ssh_host_tree};
use crate::{config, log_debug, log_error};
use portable_pty::{Child, MasterPty};
use ratatui::layout::Rect;
use std::io::Write;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering as AtomicOrdering},
};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fs, io,
    time::{Duration, Instant},
};
use vt100::Parser;

/// Connection request emitted when exiting the session manager into a direct `cossh` run.
#[derive(Debug, Clone)]
pub(crate) struct ConnectRequest {
    pub(super) target: String,
    pub(super) profile: Option<String>,
    pub(super) force_ssh_logging: bool,
}

/// Represents an SSH session output buffer.
pub struct SshSession {
    pub(super) pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub(super) writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub(super) _child: Box<dyn Child + Send>,
    pub(super) parser: Arc<Mutex<Parser>>,
    pub(super) exited: Arc<Mutex<bool>>,
    pub(super) clear_pending: Arc<Mutex<bool>>,
    pub(super) render_epoch: Arc<AtomicU64>,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalSearchState {
    pub(super) active: bool,
    pub(super) query: String,
    pub(super) matches: Vec<(i64, u16, usize)>,
    pub(super) current: usize,
}

/// Represents an open host tab.
pub struct HostTab {
    pub(super) host: SshHost,
    pub(super) title: String,
    pub(super) session: Option<SshSession>,
    pub(super) scroll_offset: usize,
    pub(super) terminal_search: TerminalSearchState,
    pub(super) force_ssh_logging: bool,
    pub(super) last_pty_size: Option<(u16, u16)>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct HostSearchEntry {
    pub(super) name_lower: String,
    pub(super) hostname_lower: Option<String>,
    pub(super) user_lower: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuickConnectField {
    User,
    Host,
    Profile,
    Logging,
    Connect,
}

impl QuickConnectField {
    pub(super) fn next(self) -> Self {
        match self {
            Self::User => Self::Host,
            Self::Host => Self::Profile,
            Self::Profile => Self::Logging,
            Self::Logging => Self::Connect,
            Self::Connect => Self::User,
        }
    }

    pub(super) fn prev(self) -> Self {
        match self {
            Self::User => Self::Connect,
            Self::Host => Self::User,
            Self::Profile => Self::Host,
            Self::Logging => Self::Profile,
            Self::Connect => Self::Logging,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct QuickConnectState {
    pub(crate) user: String,
    pub(crate) host: String,
    pub(crate) profile_options: Vec<String>,
    pub(crate) profile_index: usize,
    pub(crate) ssh_logging: bool,
    pub(crate) selected: QuickConnectField,
    pub(crate) error: Option<String>,
}

impl QuickConnectState {
    pub(super) fn new(default_ssh_logging: bool, mut profile_options: Vec<String>) -> Self {
        if profile_options.is_empty() {
            profile_options.push("default".to_string());
        }
        let profile_index = profile_options.iter().position(|profile| profile.eq_ignore_ascii_case("default")).unwrap_or(0);

        Self {
            user: String::new(),
            host: String::new(),
            profile_options,
            profile_index,
            ssh_logging: default_ssh_logging,
            selected: QuickConnectField::User,
            error: None,
        }
    }

    pub(super) fn selected_profile_label(&self) -> &str {
        self.profile_options.get(self.profile_index).map(String::as_str).unwrap_or("default")
    }

    pub(super) fn selected_profile_for_cli(&self) -> Option<String> {
        let profile = self.selected_profile_label();
        if profile.eq_ignore_ascii_case("default") {
            None
        } else {
            Some(profile.to_string())
        }
    }

    pub(super) fn select_next_profile(&mut self) {
        if self.profile_options.is_empty() {
            return;
        }
        self.profile_index = (self.profile_index + 1) % self.profile_options.len();
    }

    pub(super) fn select_prev_profile(&mut self) {
        if self.profile_options.is_empty() {
            return;
        }
        if self.profile_index == 0 {
            self.profile_index = self.profile_options.len() - 1;
        } else {
            self.profile_index -= 1;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostTreeRowKind {
    Folder(FolderId),
    Host(usize),
}

#[derive(Debug, Clone)]
pub(crate) struct HostTreeRow {
    pub(crate) kind: HostTreeRowKind,
    pub(crate) depth: usize,
    pub(crate) display_name: String,
    pub(crate) expanded: bool,
}

/// Main application state.
pub struct SessionManager {
    pub(super) hosts: Vec<SshHost>,
    pub(super) host_search_index: Vec<HostSearchEntry>,
    pub(super) host_tree_root: TreeFolder,
    pub(super) visible_host_rows: Vec<HostTreeRow>,
    pub(super) selected_host_row: usize,
    pub(super) host_match_scores: HashMap<usize, i32>,
    pub(super) collapsed_folders: HashSet<FolderId>,
    pub(super) search_query: String,
    pub(super) search_mode: bool,
    pub(super) should_exit: bool,
    pub(super) selected_host_to_connect: Option<ConnectRequest>,
    pub(super) host_list_area: Rect,
    pub(super) host_info_area: Rect,
    pub(super) host_scroll_offset: usize,
    pub(super) host_panel_width: u16,
    pub(super) host_info_height: u16,
    pub(super) tabs: Vec<HostTab>,
    pub(super) selected_tab: usize,
    pub(super) focus_on_manager: bool,
    pub(super) selection_start: Option<(i64, u16)>,
    pub(super) selection_end: Option<(i64, u16)>,
    pub(super) is_selecting: bool,
    pub(super) selection_dragged: bool,
    pub(super) tab_content_area: Rect,
    pub(super) tab_bar_area: Rect,
    pub(super) host_panel_area: Rect,
    pub(super) last_click: Option<(Instant, u16, u16)>,
    pub(super) is_dragging_divider: bool,
    pub(super) is_dragging_host_scrollbar: bool,
    pub(super) is_dragging_host_info_divider: bool,
    pub(super) tab_scroll_offset: usize,
    pub(super) history_buffer: usize,
    pub(super) host_panel_visible: bool,
    pub(super) host_info_visible: bool,
    pub(super) quick_connect: Option<QuickConnectState>,
    pub(super) quick_connect_default_ssh_logging: bool,
    pub(super) ui_dirty: bool,
    pub(super) last_draw_at: Instant,
    pub(super) last_seen_render_epoch: u64,
}

impl SessionManager {
    fn build_host_search_index(hosts: &[SshHost]) -> Vec<HostSearchEntry> {
        hosts
            .iter()
            .map(|host| HostSearchEntry {
                name_lower: host.name.to_lowercase(),
                hostname_lower: host.hostname.as_ref().map(|hostname| hostname.to_lowercase()),
                user_lower: host.user.as_ref().map(|user| user.to_lowercase()),
            })
            .collect()
    }

    fn current_render_epoch(&self) -> u64 {
        self.tabs
            .iter()
            .filter_map(|tab| tab.session.as_ref())
            .fold(0u64, |acc, session| acc.wrapping_add(session.render_epoch.load(AtomicOrdering::Relaxed)))
    }

    pub(super) fn should_draw(&self, heartbeat: Duration) -> bool {
        self.ui_dirty || self.current_render_epoch() != self.last_seen_render_epoch || self.last_draw_at.elapsed() >= heartbeat
    }

    pub(super) fn mark_ui_dirty(&mut self) {
        self.ui_dirty = true;
    }

    pub(super) fn mark_drawn(&mut self) {
        self.last_draw_at = Instant::now();
        self.last_seen_render_epoch = self.current_render_epoch();
        self.ui_dirty = false;
    }

    pub(super) fn discover_quick_connect_profiles(&self) -> Vec<String> {
        let mut profiles: HashSet<String> = HashSet::new();
        profiles.insert("default".to_string());

        let config_dir = config::SESSION_CONFIG
            .get()
            .and_then(|config_lock| {
                config_lock
                    .read()
                    .ok()
                    .map(|cfg| cfg.metadata.config_path.parent().map(|config_path| config_path.to_path_buf()))
            })
            .flatten();

        if let Some(config_dir) = config_dir
            && let Ok(entries) = fs::read_dir(config_dir)
        {
            for entry in entries.flatten() {
                let filename = entry.file_name();
                let Some(filename) = filename.to_str() else {
                    continue;
                };

                if filename == ".cossh-config.yaml" {
                    profiles.insert("default".to_string());
                    continue;
                }

                if let Some(profile_name) = filename.strip_suffix(".cossh-config.yaml")
                    && !profile_name.is_empty()
                    && !profile_name.starts_with('.')
                {
                    profiles.insert(profile_name.to_string());
                }
            }
        }

        let mut profile_list: Vec<String> = profiles.into_iter().collect();
        profile_list.sort_by(
            |left, right| match (left.eq_ignore_ascii_case("default"), right.eq_ignore_ascii_case("default")) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                _ => left.to_lowercase().cmp(&right.to_lowercase()),
            },
        );

        profile_list
    }

    pub(super) fn collect_descendant_folder_ids(folder: &TreeFolder, out: &mut HashSet<FolderId>) {
        for child in &folder.children {
            out.insert(child.id);
            Self::collect_descendant_folder_ids(child, out);
        }
    }

    pub(super) fn current_tab_search(&self) -> Option<&TerminalSearchState> {
        self.tabs.get(self.selected_tab).map(|tab| &tab.terminal_search)
    }

    pub(super) fn current_tab_search_mut(&mut self) -> Option<&mut TerminalSearchState> {
        self.tabs.get_mut(self.selected_tab).map(|tab| &mut tab.terminal_search)
    }

    /// Create a new SessionManager instance.
    pub fn new() -> io::Result<Self> {
        log_debug!("Initializing session manager");

        let tree_model = load_ssh_host_tree().unwrap_or_else(|err| {
            log_error!("Failed to load SSH hosts: {}", err);
            let fallback_root = TreeFolder {
                id: 0,
                name: "config".to_string(),
                path: std::path::PathBuf::from("~/.ssh/config"),
                children: Vec::new(),
                host_indices: Vec::new(),
            };
            crate::ssh_config::SshHostTreeModel {
                root: fallback_root,
                hosts: Vec::new(),
            }
        });
        let hosts = tree_model.hosts;
        let host_search_index = Self::build_host_search_index(&hosts);
        let host_tree_root = tree_model.root;
        let (history_buffer, host_tree_start_collapsed, host_info_visible, host_view_size_percent, info_view_size_percent) = config::SESSION_CONFIG
            .get()
            .and_then(|config_lock| {
                config_lock.read().ok().and_then(|cfg| {
                    cfg.interactive_settings.as_ref().map(|interactive| {
                        (
                            interactive.history_buffer,
                            interactive.host_tree_starts_collapsed(),
                            interactive.info_view,
                            interactive.host_view_size,
                            interactive.info_view_size,
                        )
                    })
                })
            })
            .unwrap_or((1000, true, true, 25, 40));

        let quick_connect_default_ssh_logging = config::SESSION_CONFIG
            .get()
            .and_then(|config_lock| config_lock.read().ok().map(|cfg| cfg.settings.ssh_logging))
            .unwrap_or(false);

        let (term_width, term_height) = crossterm::terminal::size().unwrap_or((100, 30));
        let host_panel_width = (((term_width as u32 * host_view_size_percent as u32) / 100) as u16).clamp(15, 80);
        let content_height = term_height.saturating_sub(2);
        let mut host_info_height = ((content_height as u32 * info_view_size_percent as u32) / 100) as u16;
        host_info_height = host_info_height.max(3);
        if content_height > 4 {
            host_info_height = host_info_height.min(content_height.saturating_sub(4));
        }

        let mut collapsed_folders = HashSet::new();
        if host_tree_start_collapsed {
            Self::collect_descendant_folder_ids(&host_tree_root, &mut collapsed_folders);
        }

        log_debug!("Loaded {} SSH hosts", hosts.len());

        let mut app = Self {
            hosts,
            host_search_index,
            host_tree_root,
            visible_host_rows: Vec::new(),
            selected_host_row: 0,
            host_match_scores: HashMap::new(),
            collapsed_folders,
            host_list_area: Rect::default(),
            host_info_area: Rect::default(),
            host_scroll_offset: 0,
            host_panel_width,
            host_info_height,
            search_query: String::new(),
            search_mode: false,
            should_exit: false,
            selected_host_to_connect: None,
            tabs: Vec::new(),
            selected_tab: 0,
            focus_on_manager: true,
            selection_start: None,
            selection_end: None,
            is_selecting: false,
            selection_dragged: false,
            tab_content_area: Rect::default(),
            tab_bar_area: Rect::default(),
            host_panel_area: Rect::default(),
            last_click: None,
            is_dragging_divider: false,
            is_dragging_host_scrollbar: false,
            is_dragging_host_info_divider: false,
            tab_scroll_offset: 0,
            history_buffer,
            host_panel_visible: true,
            host_info_visible,
            quick_connect: None,
            quick_connect_default_ssh_logging,
            ui_dirty: true,
            last_draw_at: Instant::now(),
            last_seen_render_epoch: 0,
        };

        app.update_filtered_hosts();
        Ok(app)
    }
}
