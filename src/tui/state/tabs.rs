//! Per-tab session state.

use crate::inventory::InventoryHost;
use crate::terminal_core::highlight_overlay::HighlightOverlayEngine;
use std::collections::HashMap;

pub(crate) use crate::terminal_core::TerminalSession as ManagedSession;

#[derive(Debug, Clone, Default)]
pub(crate) struct TerminalSearchState {
    pub(crate) active: bool,
    pub(crate) query: String,
    pub(crate) query_cursor: usize,
    pub(crate) query_selection: Option<(usize, usize)>,
    pub(crate) matches: Vec<(i64, u16, u16)>,
    pub(crate) current: usize,
    pub(crate) highlight_row_ranges: HashMap<i64, Vec<(u16, u16)>>,
    pub(crate) current_highlight_range: Option<(i64, u16, u16)>,
    pub(crate) last_search_query: String,
    pub(crate) last_scanned_render_epoch: u64,
}

/// Represents an open host tab.
pub(crate) struct HostTab {
    pub(crate) host: InventoryHost,
    pub(crate) title: String,
    pub(crate) session: Option<ManagedSession>,
    pub(crate) session_error: Option<String>,
    pub(crate) highlight_overlay: HighlightOverlayEngine,
    pub(crate) scroll_offset: usize,
    pub(crate) terminal_search: TerminalSearchState,
    pub(crate) force_ssh_logging: bool,
    pub(crate) last_pty_size: Option<(u16, u16)>,
}
