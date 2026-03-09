//! Core TUI state and initialization.

mod init;

use super::host_browser::{HostSearchEntry, HostTreeRow};
use super::quick_connect::QuickConnectState;
use super::tabs::{HostTab, TerminalSearchState};
use super::vault::{VaultStatusModalState, VaultUnlockState};
use crate::auth::ipc::{self, VaultStatus, VaultStatusEvent, VaultStatusEventKind};
use crate::inventory::{ConnectionProtocol, FolderId, InventoryHost, TreeFolder};
use crate::log_debug;
use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::io;
use std::time::{Duration, Instant};

use self::init::{AppStateInit, VaultStatusEventWatcher, load_app_state_init, load_vault_status};

pub(crate) const HOST_PANEL_MIN_WIDTH: u16 = 15;
pub(crate) const HOST_PANEL_MAX_WIDTH: u16 = 80;

/// Connection request emitted when exiting the session manager into a direct `cossh` run.
#[derive(Debug, Clone)]
pub(crate) struct ConnectRequest {
    pub(crate) target: String,
    pub(crate) profile: Option<String>,
    pub(crate) force_ssh_logging: bool,
    pub(crate) protocol: ConnectionProtocol,
}

/// Main application state.
pub(crate) struct AppState {
    pub(crate) hosts: Vec<InventoryHost>,
    pub(crate) host_search_index: Vec<HostSearchEntry>,
    pub(crate) host_tree_root: TreeFolder,
    pub(crate) inventory_load_error: Option<String>,
    pub(crate) visible_host_rows: Vec<HostTreeRow>,
    pub(crate) selected_host_row: usize,
    pub(crate) host_match_scores: HashMap<usize, i32>,
    pub(crate) collapsed_folders: HashSet<FolderId>,
    pub(crate) search_query: String,
    pub(crate) search_query_cursor: usize,
    pub(crate) search_query_selection: Option<(usize, usize)>,
    pub(crate) search_mode: bool,
    pub(crate) should_exit: bool,
    pub(crate) selected_host_to_connect: Option<ConnectRequest>,
    pub(crate) host_list_area: Rect,
    pub(crate) host_info_area: Rect,
    pub(crate) host_scroll_offset: usize,
    pub(crate) host_panel_width: u16,
    pub(crate) host_panel_default_percent: u16,
    pub(crate) host_info_height: u16,
    pub(crate) tabs: Vec<HostTab>,
    pub(crate) selected_tab: usize,
    pub(crate) focus_on_manager: bool,
    pub(crate) selection_start: Option<(i64, u16)>,
    pub(crate) selection_end: Option<(i64, u16)>,
    pub(crate) is_selecting: bool,
    pub(crate) selection_dragged: bool,
    pub(crate) tab_content_area: Rect,
    pub(crate) tab_bar_area: Rect,
    pub(crate) host_panel_area: Rect,
    pub(crate) last_click: Option<(Instant, u16, u16)>,
    pub(crate) is_dragging_divider: bool,
    pub(crate) is_dragging_host_scrollbar: bool,
    pub(crate) is_dragging_host_info_divider: bool,
    pub(crate) dragging_tab: Option<usize>,
    pub(crate) tab_scroll_offset: usize,
    pub(crate) host_panel_visible: bool,
    pub(crate) host_info_visible: bool,
    pub(crate) quick_connect: Option<QuickConnectState>,
    pub(crate) vault_unlock: Option<VaultUnlockState>,
    pub(crate) vault_status_modal: Option<VaultStatusModalState>,
    pub(crate) vault_status: VaultStatus,
    pub(crate) quick_connect_default_ssh_logging: bool,
    pub(crate) last_terminal_size: (u16, u16),
    pub(crate) ui_dirty: bool,
    pub(crate) last_draw_at: Instant,
    pub(crate) last_seen_render_epoch: u64,
    pub(crate) last_vault_status_refresh_at: Instant,
    vault_status_events: Option<VaultStatusEventWatcher>,
}

impl AppState {
    // Layout sizing helpers.
    fn clamp_host_panel_width_for_terminal(width: u16, term_width: u16) -> u16 {
        let max_for_terminal = term_width.saturating_sub(1).max(1);
        let upper = HOST_PANEL_MAX_WIDTH.min(max_for_terminal);
        let lower = HOST_PANEL_MIN_WIDTH.min(upper);
        width.clamp(lower, upper)
    }

    // Search indexing helpers.
    fn build_host_search_index(hosts: &[InventoryHost]) -> Vec<HostSearchEntry> {
        hosts
            .iter()
            .map(|host| HostSearchEntry {
                name_lower: host.name.to_lowercase(),
                host_lower: Some(host.host.to_lowercase()),
                user_lower: host.user.as_ref().map(|user| user.to_lowercase()),
                hidden: host.hidden,
            })
            .collect()
    }

    // Render invalidation / draw throttling.
    fn current_render_epoch(&self) -> u64 {
        self.tabs
            .iter()
            .filter_map(|tab| tab.session.as_ref())
            .fold(0u64, |acc, session| acc.wrapping_add(session.render_epoch()))
    }

    pub(crate) fn should_draw(&self, heartbeat: Duration) -> bool {
        self.ui_dirty || self.current_render_epoch() != self.last_seen_render_epoch || self.last_draw_at.elapsed() >= heartbeat
    }

    pub(crate) fn mark_ui_dirty(&mut self) {
        self.ui_dirty = true;
    }

    pub(crate) fn mark_drawn(&mut self) {
        self.last_draw_at = Instant::now();
        self.last_seen_render_epoch = self.current_render_epoch();
        self.ui_dirty = false;
    }

    // Terminal resize handling.
    pub(crate) fn handle_terminal_resize(&mut self, term_width: u16, term_height: u16) {
        if term_width == 0 || term_height == 0 {
            return;
        }

        let (prev_width, prev_height) = self.last_terminal_size;
        if prev_width == term_width && prev_height == term_height {
            return;
        }

        if prev_width > 0 && term_width != prev_width {
            let scaled = ((self.host_panel_width as u32 * term_width as u32) + (prev_width as u32 / 2)) / prev_width as u32;
            self.host_panel_width = Self::clamp_host_panel_width_for_terminal(scaled as u16, term_width);
            if term_width > prev_width {
                let default_width =
                    Self::clamp_host_panel_width_for_terminal(((term_width as u32 * self.host_panel_default_percent as u32) / 100) as u16, term_width);
                self.host_panel_width = self.host_panel_width.min(default_width);
            }
        } else {
            self.host_panel_width = Self::clamp_host_panel_width_for_terminal(self.host_panel_width, term_width);
        }

        self.last_terminal_size = (term_width, term_height);
    }

    // Folder tree helpers.
    pub(crate) fn collect_descendant_folder_ids(folder: &TreeFolder, out: &mut HashSet<FolderId>) {
        for child in &folder.children {
            out.insert(child.id);
            Self::collect_descendant_folder_ids(child, out);
        }
    }

    // Current-tab search accessors.
    pub(crate) fn current_tab_search(&self) -> Option<&TerminalSearchState> {
        self.tabs.get(self.selected_tab).map(|tab| &tab.terminal_search)
    }

    pub(crate) fn current_tab_search_mut(&mut self) -> Option<&mut TerminalSearchState> {
        self.tabs.get_mut(self.selected_tab).map(|tab| &mut tab.terminal_search)
    }

    pub(crate) fn set_vault_status(&mut self, status: VaultStatus) {
        self.last_vault_status_refresh_at = Instant::now();
        if self.vault_status != status {
            self.vault_status = status;
            self.mark_ui_dirty();
        }
    }

    pub(crate) fn refresh_vault_status(&mut self) {
        let status = load_vault_status();
        self.last_vault_status_refresh_at = Instant::now();
        if self.vault_status != status {
            self.vault_status = status;
            self.mark_ui_dirty();
        }
    }

    pub(crate) fn refresh_vault_status_if_stale(&mut self, refresh_interval: Duration) {
        if self.vault_status_modal.is_none() {
            return;
        }
        if self.last_vault_status_refresh_at.elapsed() < refresh_interval {
            return;
        }
        self.refresh_vault_status();
    }

    pub(crate) fn handle_vault_status_notification(&mut self, event: VaultStatusEvent) {
        log_debug!("Received password vault status notification: {:?}", event.kind);
        self.set_vault_status(event.status);
        if let Some(modal) = self.vault_status_modal.as_mut() {
            let message = match event.kind {
                VaultStatusEventKind::Locked => "Vault locked.",
                VaultStatusEventKind::Unlocked => "Vault unlocked.",
            };
            modal.set_message(message.to_string(), false);
            self.mark_ui_dirty();
        }
    }

    pub(crate) fn apply_vault_status_notifications(&mut self) {
        if let Some(paths) = self.vault_status_events.as_ref().and_then(VaultStatusEventWatcher::take_pending_paths) {
            match ipc::read_vault_status_event(&paths) {
                Ok(event) => self.handle_vault_status_notification(event),
                Err(err) => log_debug!("Failed to read password vault status notification: {}", err),
            }
        }
    }

    // Construction.
    fn build_from_init(init: AppStateInit) -> Self {
        let host_search_index = Self::build_host_search_index(&init.hosts);
        let now = Instant::now();
        let mut app = Self {
            hosts: init.hosts,
            host_search_index,
            host_tree_root: init.host_tree_root,
            inventory_load_error: init.inventory_load_error,
            visible_host_rows: Vec::new(),
            selected_host_row: 0,
            host_match_scores: HashMap::new(),
            collapsed_folders: init.collapsed_folders,
            host_list_area: Rect::default(),
            host_info_area: Rect::default(),
            host_scroll_offset: 0,
            host_panel_width: init.host_panel_width,
            host_panel_default_percent: init.host_panel_default_percent,
            host_info_height: init.host_info_height,
            search_query: String::new(),
            search_query_cursor: 0,
            search_query_selection: None,
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
            dragging_tab: None,
            tab_scroll_offset: 0,
            host_panel_visible: true,
            host_info_visible: init.host_info_visible,
            quick_connect: None,
            vault_unlock: None,
            vault_status_modal: None,
            vault_status: init.vault_status,
            quick_connect_default_ssh_logging: init.quick_connect_default_ssh_logging,
            last_terminal_size: init.last_terminal_size,
            ui_dirty: true,
            last_draw_at: now,
            last_seen_render_epoch: 0,
            last_vault_status_refresh_at: now,
            vault_status_events: init.vault_status_events,
        };

        app.update_filtered_hosts();
        app
    }

    /// Create a new AppState instance.
    pub(crate) fn new() -> io::Result<Self> {
        log_debug!("Initializing session manager");
        Ok(Self::build_from_init(load_app_state_init()))
    }

    // Test scaffolding.
    #[cfg(test)]
    pub(crate) fn new_for_tests() -> Self {
        Self::build_from_init(init::test_app_state_init())
    }
}

#[cfg(test)]
#[path = "../../test/tui/app_state.rs"]
mod tests;
