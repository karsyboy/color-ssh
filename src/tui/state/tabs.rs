//! Per-tab session state.

use super::host_editor::{HostEditorMode, HostEditorState};
use crate::inventory::InventoryHost;
use crate::terminal::highlight_overlay::HighlightOverlayEngine;
use std::collections::HashMap;
use std::path::PathBuf;

pub(crate) use crate::terminal::TerminalSession as ManagedSession;

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
    pub(crate) kind: TabKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditorTabId {
    NewEntry { source_file: PathBuf },
    DuplicateEntry { source_file: PathBuf, source_host_name: String },
    ExistingHost { source_file: PathBuf, host_name: String },
}

#[derive(Debug, Clone)]
pub(crate) struct EditorTabState {
    pub(crate) id: EditorTabId,
    pub(crate) editor_state: HostEditorState,
}

pub(crate) struct TerminalTabState {
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

pub(crate) enum TabKind {
    Terminal { terminal: TerminalTabState },
    Editor { editor: EditorTabState },
}

impl EditorTabId {
    pub(crate) fn for_existing_host(host: &InventoryHost) -> Self {
        Self::ExistingHost {
            source_file: host.source_file.clone(),
            host_name: host.name.clone(),
        }
    }

    pub(crate) fn for_new_entry(source_file: PathBuf) -> Self {
        Self::NewEntry { source_file }
    }

    pub(crate) fn for_duplicate_host(host: &InventoryHost) -> Self {
        Self::DuplicateEntry {
            source_file: host.source_file.clone(),
            source_host_name: host.name.clone(),
        }
    }
}

impl HostTab {
    pub(crate) fn new_terminal(terminal: TerminalTabState) -> Self {
        Self {
            kind: TabKind::Terminal { terminal },
        }
    }

    pub(crate) fn new_editor(editor: EditorTabState) -> Self {
        Self {
            kind: TabKind::Editor { editor },
        }
    }

    pub(crate) fn title(&self) -> String {
        match &self.kind {
            TabKind::Terminal { terminal } => terminal.title.clone(),
            TabKind::Editor { editor } => match editor.editor_state.mode {
                HostEditorMode::Create => "✎ New Entry".to_string(),
                HostEditorMode::Edit => {
                    let name = editor.editor_state.name.value.trim();
                    if !name.is_empty() {
                        format!("✎ {name}")
                    } else if let Some(original_name) = editor.editor_state.original_name.as_deref() {
                        format!("✎ {original_name}")
                    } else {
                        "✎ Entry".to_string()
                    }
                }
            },
        }
    }

    pub(crate) fn terminal(&self) -> Option<&TerminalTabState> {
        match &self.kind {
            TabKind::Terminal { terminal } => Some(terminal),
            TabKind::Editor { .. } => None,
        }
    }

    pub(crate) fn terminal_mut(&mut self) -> Option<&mut TerminalTabState> {
        match &mut self.kind {
            TabKind::Terminal { terminal } => Some(terminal),
            TabKind::Editor { .. } => None,
        }
    }

    pub(crate) fn editor(&self) -> Option<&EditorTabState> {
        match &self.kind {
            TabKind::Editor { editor } => Some(editor),
            TabKind::Terminal { .. } => None,
        }
    }

    pub(crate) fn editor_mut(&mut self) -> Option<&mut EditorTabState> {
        match &mut self.kind {
            TabKind::Editor { editor } => Some(editor),
            TabKind::Terminal { .. } => None,
        }
    }

    pub(crate) fn editor_state(&self) -> Option<&HostEditorState> {
        self.editor().map(|editor| &editor.editor_state)
    }
}
