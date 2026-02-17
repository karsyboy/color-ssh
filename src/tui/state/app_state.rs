//! Core TUI state and initialization.

use super::host_browser_state::{HostSearchEntry, HostTreeRow};
use super::quick_connect_state::QuickConnectState;
use super::tab_state::{HostTab, TerminalSearchState};
use crate::ssh_config::{FolderId, SshHost, TreeFolder, load_ssh_host_tree};
use crate::{config, log_debug, log_error};
use ratatui::layout::Rect;
use std::sync::atomic::Ordering as AtomicOrdering;
use std::{
    collections::{HashMap, HashSet},
    io,
    time::{Duration, Instant},
};

pub(crate) const HOST_PANEL_MIN_WIDTH: u16 = 15;
pub(crate) const HOST_PANEL_MAX_WIDTH: u16 = 80;

/// Connection request emitted when exiting the session manager into a direct `cossh` run.
#[derive(Debug, Clone)]
pub(crate) struct ConnectRequest {
    pub(crate) target: String,
    pub(crate) profile: Option<String>,
    pub(crate) force_ssh_logging: bool,
}

/// Main application state.
pub struct AppState {
    pub(crate) hosts: Vec<SshHost>,
    pub(crate) host_search_index: Vec<HostSearchEntry>,
    pub(crate) host_tree_root: TreeFolder,
    pub(crate) visible_host_rows: Vec<HostTreeRow>,
    pub(crate) selected_host_row: usize,
    pub(crate) host_match_scores: HashMap<usize, i32>,
    pub(crate) collapsed_folders: HashSet<FolderId>,
    pub(crate) search_query: String,
    pub(crate) search_mode: bool,
    pub(crate) should_exit: bool,
    pub(crate) selected_host_to_connect: Option<ConnectRequest>,
    pub(crate) host_list_area: Rect,
    pub(crate) host_info_area: Rect,
    pub(crate) host_scroll_offset: usize,
    pub(crate) host_panel_width: u16,
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
    pub(crate) tab_scroll_offset: usize,
    pub(crate) history_buffer: usize,
    pub(crate) host_panel_visible: bool,
    pub(crate) host_info_visible: bool,
    pub(crate) quick_connect: Option<QuickConnectState>,
    pub(crate) quick_connect_default_ssh_logging: bool,
    pub(crate) last_terminal_size: (u16, u16),
    pub(crate) ui_dirty: bool,
    pub(crate) last_draw_at: Instant,
    pub(crate) last_seen_render_epoch: u64,
}

impl AppState {
    fn clamp_host_panel_width_for_terminal(width: u16, term_width: u16) -> u16 {
        let max_for_terminal = term_width.saturating_sub(1).max(1);
        let upper = HOST_PANEL_MAX_WIDTH.min(max_for_terminal);
        let lower = HOST_PANEL_MIN_WIDTH.min(upper);
        width.clamp(lower, upper)
    }

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
        } else {
            self.host_panel_width = Self::clamp_host_panel_width_for_terminal(self.host_panel_width, term_width);
        }

        self.last_terminal_size = (term_width, term_height);
    }
        
    pub(crate) fn collect_descendant_folder_ids(folder: &TreeFolder, out: &mut HashSet<FolderId>) {
        for child in &folder.children {
            out.insert(child.id);
            Self::collect_descendant_folder_ids(child, out);
        }
    }

    pub(crate) fn current_tab_search(&self) -> Option<&TerminalSearchState> {
        self.tabs.get(self.selected_tab).map(|tab| &tab.terminal_search)
    }

    pub(crate) fn current_tab_search_mut(&mut self) -> Option<&mut TerminalSearchState> {
        self.tabs.get_mut(self.selected_tab).map(|tab| &mut tab.terminal_search)
    }

    /// Create a new AppState instance.
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
        let host_panel_width = Self::clamp_host_panel_width_for_terminal(((term_width as u32 * host_view_size_percent as u32) / 100) as u16, term_width);
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
            last_terminal_size: (term_width, term_height),
            ui_dirty: true,
            last_draw_at: Instant::now(),
            last_seen_render_epoch: 0,
        };

        app.update_filtered_hosts();
        Ok(app)
    }
}

pub(crate) type SessionManager = AppState;

#[cfg(test)]
mod tests {
    use super::AppState;

    #[test]
    fn terminal_resize_scales_host_panel_width_proportionally() {
        let mut app = AppState::new().expect("app should initialize");
        app.last_terminal_size = (100, 30);
        app.host_panel_width = 25;

        app.handle_terminal_resize(200, 30);
        assert_eq!(app.host_panel_width, 50);

        app.handle_terminal_resize(120, 30);
        assert_eq!(app.host_panel_width, 30);
    }

    #[test]
    fn terminal_resize_clamps_host_panel_width_for_small_windows() {
        let mut app = AppState::new().expect("app should initialize");
        app.last_terminal_size = (120, 30);
        app.host_panel_width = 30;

        app.handle_terminal_resize(10, 30);
        assert_eq!(app.host_panel_width, 9);
    }
}