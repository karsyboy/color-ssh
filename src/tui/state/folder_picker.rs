//! Folder picker and folder-management modal state.

use crate::tui::text_edit;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FolderPickerMode {
    CreatePlacement,
    CreateFolderParent {
        folder_name: String,
        name_cursor: usize,
        name_selection: Option<(usize, usize)>,
        parent_folder_path: Vec<String>,
    },
    MoveHost {
        host_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FolderPickerRow {
    pub(crate) folder_path: Vec<String>,
    pub(crate) depth: usize,
    pub(crate) label: String,
}

#[derive(Debug, Clone)]
pub(crate) struct FolderPickerState {
    pub(crate) source_file: PathBuf,
    pub(crate) mode: FolderPickerMode,
    pub(crate) rows: Vec<FolderPickerRow>,
    pub(crate) selected: usize,
}

impl FolderPickerState {
    pub(crate) fn new(source_file: PathBuf, mode: FolderPickerMode, rows: Vec<FolderPickerRow>, selected: usize) -> Self {
        let mut state = Self {
            source_file,
            mode,
            rows,
            selected,
        };
        state.selected = state.selected.min(state.rows.len().saturating_sub(1));
        state
    }

    pub(crate) fn title(&self) -> &'static str {
        match self.mode {
            FolderPickerMode::CreatePlacement => " Select Placement Folder ",
            FolderPickerMode::CreateFolderParent { .. } => " Select Parent Folder ",
            FolderPickerMode::MoveHost { .. } => " Move Entry to Folder ",
        }
    }

    pub(crate) fn selected_row(&self) -> Option<&FolderPickerRow> {
        self.rows.get(self.selected)
    }

    pub(crate) fn selected_folder_path(&self) -> Vec<String> {
        self.selected_row().map(|row| row.folder_path.clone()).unwrap_or_default()
    }

    pub(crate) fn select_next(&mut self) {
        if self.rows.is_empty() {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + 1) % self.rows.len();
    }

    pub(crate) fn select_prev(&mut self) {
        if self.rows.is_empty() {
            self.selected = 0;
            return;
        }
        if self.selected == 0 {
            self.selected = self.rows.len().saturating_sub(1);
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FolderRenameState {
    pub(crate) source_file: PathBuf,
    pub(crate) folder_path: Vec<String>,
    pub(crate) name: String,
    pub(crate) cursor: usize,
    pub(crate) selection: Option<(usize, usize)>,
    pub(crate) error: Option<String>,
}

impl FolderRenameState {
    pub(crate) fn new(source_file: PathBuf, folder_path: Vec<String>) -> Self {
        let current_name = folder_path.last().cloned().unwrap_or_default();
        let cursor = text_edit::char_len(&current_name);
        Self {
            source_file,
            folder_path,
            name: current_name,
            cursor,
            selection: None,
            error: None,
        }
    }

    pub(crate) fn folder_display_path(&self) -> String {
        if self.folder_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.folder_path.join("/"))
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FolderCreateState {
    pub(crate) source_file: PathBuf,
    pub(crate) parent_folder_path: Vec<String>,
    pub(crate) name: String,
    pub(crate) cursor: usize,
    pub(crate) selection: Option<(usize, usize)>,
    pub(crate) error: Option<String>,
}

impl FolderCreateState {
    pub(crate) fn new(source_file: PathBuf, parent_folder_path: Vec<String>) -> Self {
        Self {
            source_file,
            parent_folder_path,
            name: String::new(),
            cursor: 0,
            selection: None,
            error: None,
        }
    }

    pub(crate) fn parent_display_path(&self) -> String {
        if self.parent_folder_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.parent_folder_path.join("/"))
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FolderDeleteConfirmState {
    pub(crate) source_file: PathBuf,
    pub(crate) folder_path: Vec<String>,
    pub(crate) folder_name: String,
    pub(crate) removed_entry_count: usize,
}

impl FolderDeleteConfirmState {
    pub(crate) fn folder_display_path(&self) -> String {
        if self.folder_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.folder_path.join("/"))
        }
    }
}
