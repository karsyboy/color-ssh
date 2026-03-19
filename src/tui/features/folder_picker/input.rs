//! Folder picker and folder-management keyboard handling.

use crate::inventory::{
    FolderId, InventoryHost, TreeFolder, create_inventory_folder, delete_inventory_folder, move_inventory_host_entry, relocate_inventory_folder,
};
use crate::runtime::{ReloadNoticeToast, format_reload_notice};
use crate::tui::text_edit;
use crate::tui::{
    AppState, FolderCreateState, FolderDeleteConfirmState, FolderPickerMode, FolderPickerRow, FolderPickerState, FolderRenameState, HostEditorMode,
    HostTreeRowKind,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::{Path, PathBuf};

impl AppState {
    pub(crate) fn open_folder_picker_for_editor_placement(&mut self) {
        let Some(form) = self.selected_host_editor() else {
            return;
        };
        if form.mode != HostEditorMode::Create {
            return;
        }

        let source_file = form.source_file.clone();
        let initial_folder_path = Self::parse_folder_path_display(&form.folder_path.value);
        self.open_folder_picker(source_file, FolderPickerMode::CreatePlacement, initial_folder_path);
    }

    pub(crate) fn open_folder_picker_for_move_host(&mut self, host_idx: usize) {
        let Some(host) = self.hosts.get(host_idx).cloned() else {
            return;
        };

        let source_file = host.source_file.clone();
        let initial_folder_path = self.host_folder_path_in_source(&host);
        self.open_folder_picker(source_file, FolderPickerMode::MoveHost { host_name: host.name.clone() }, initial_folder_path);
    }

    pub(crate) fn open_folder_create_for_selected_row(&mut self) {
        if let Some(folder_id) = self.selected_folder_id()
            && let Some(folder) = self.folder_by_id(folder_id)
        {
            let source_file = folder.path.clone();
            let parent_folder_path = self.folder_path_segments_by_id_in_source(folder_id, &source_file).unwrap_or_default();
            self.open_folder_create_modal(source_file, parent_folder_path);
            return;
        }

        if let Some(host_idx) = self.selected_host_idx()
            && let Some(host) = self.hosts.get(host_idx).cloned()
        {
            let source_file = host.source_file.clone();
            let parent_folder_path = self.host_folder_path_in_source(&host);
            self.open_folder_create_modal(source_file, parent_folder_path);
            return;
        }

        self.open_folder_create_modal(self.selected_source_file_for_new_entry(), Vec::new());
    }

    pub(crate) fn open_folder_create_modal(&mut self, source_file: PathBuf, parent_folder_path: Vec<String>) {
        self.host_context_menu = None;
        self.folder_picker = None;
        self.folder_rename = None;
        self.folder_delete_confirm = None;
        self.folder_create = Some(FolderCreateState::new(source_file, parent_folder_path));
        self.mark_ui_dirty();
    }

    pub(crate) fn open_folder_picker_for_create_folder_parent(&mut self) {
        let Some(state) = self.folder_create.clone() else {
            return;
        };

        let source_file = state.source_file.clone();
        let initial_folder_path = state.parent_folder_path.clone();
        self.open_folder_picker(
            source_file,
            FolderPickerMode::CreateFolderParent {
                folder_name: state.name,
                name_cursor: state.cursor,
                name_selection: state.selection,
                parent_folder_path: initial_folder_path.clone(),
            },
            initial_folder_path,
        );
    }

    pub(crate) fn open_folder_picker_for_rename_folder_parent(&mut self) {
        let Some(state) = self.folder_rename.clone() else {
            return;
        };

        let source_file = state.source_file.clone();
        let initial_folder_path = state.parent_folder_path.clone();
        self.open_folder_picker(
            source_file,
            FolderPickerMode::RenameFolderParent {
                source_folder_path: state.folder_path,
                folder_name: state.name,
                name_cursor: state.cursor,
                name_selection: state.selection,
                parent_folder_path: initial_folder_path.clone(),
            },
            initial_folder_path,
        );
    }

    pub(crate) fn open_folder_rename_modal(&mut self, source_file: PathBuf, folder_path: Vec<String>) {
        if folder_path.is_empty() {
            self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice("Cannot rename the inventory root folder.")));
            self.mark_ui_dirty();
            return;
        }

        self.host_context_menu = None;
        self.folder_picker = None;
        self.folder_create = None;
        self.folder_delete_confirm = None;
        self.folder_rename = Some(FolderRenameState::new(source_file, folder_path));
        self.mark_ui_dirty();
    }

    pub(crate) fn open_folder_delete_confirm_modal(&mut self, source_file: PathBuf, folder_path: Vec<String>, folder_name: String, removed_entry_count: usize) {
        if folder_path.is_empty() {
            self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice("Cannot delete the inventory root folder.")));
            self.mark_ui_dirty();
            return;
        }

        self.host_context_menu = None;
        self.folder_picker = None;
        self.folder_create = None;
        self.folder_rename = None;
        self.folder_delete_confirm = Some(FolderDeleteConfirmState {
            source_file,
            folder_path,
            folder_name,
            removed_entry_count,
        });
        self.mark_ui_dirty();
    }

    pub(crate) fn handle_folder_picker_key(&mut self, key: KeyEvent) {
        if self.folder_picker.is_none() {
            return;
        }

        let mut should_close = false;
        let mut should_submit = false;

        if let Some(picker) = self.folder_picker.as_mut() {
            match key.code {
                KeyCode::Esc => should_close = true,
                KeyCode::Tab | KeyCode::Down => picker.select_next(),
                KeyCode::BackTab | KeyCode::Up => picker.select_prev(),
                KeyCode::Enter => should_submit = true,
                _ => {}
            }
        }

        if should_close {
            self.cancel_folder_picker();
            return;
        }

        if should_submit {
            self.submit_folder_picker();
            return;
        }

        self.mark_ui_dirty();
    }

    pub(crate) fn handle_folder_create_key(&mut self, key: KeyEvent) {
        if self.folder_create.is_none() {
            return;
        }

        let mut should_submit = false;
        let mut should_close = false;
        let mut should_pick_parent = false;
        if let Some(state) = self.folder_create.as_mut() {
            state.drag_anchor = None;
            match key.code {
                KeyCode::Esc => should_close = true,
                KeyCode::Enter => should_submit = true,
                KeyCode::Tab | KeyCode::BackTab => should_pick_parent = true,
                KeyCode::Left => text_edit::move_cursor_left(&state.name, &mut state.cursor, &mut state.selection),
                KeyCode::Right => text_edit::move_cursor_right(&state.name, &mut state.cursor, &mut state.selection),
                KeyCode::Home => text_edit::move_cursor_home(&mut state.cursor, &mut state.selection),
                KeyCode::End => text_edit::move_cursor_end(&state.name, &mut state.cursor, &mut state.selection),
                KeyCode::Backspace => {
                    text_edit::backspace(&mut state.name, &mut state.cursor, &mut state.selection);
                    state.error = None;
                }
                KeyCode::Delete => {
                    text_edit::delete_char(&mut state.name, &mut state.cursor, &mut state.selection);
                    state.error = None;
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    text_edit::select_all(&state.name, &mut state.cursor, &mut state.selection)
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    text_edit::move_cursor_end(&state.name, &mut state.cursor, &mut state.selection)
                }
                KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => should_pick_parent = true,
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                    text_edit::insert_char(&mut state.name, &mut state.cursor, &mut state.selection, ch);
                    state.error = None;
                }
                _ => {}
            }
        }

        if should_pick_parent {
            self.open_folder_picker_for_create_folder_parent();
            return;
        }
        if should_submit {
            self.submit_folder_create();
            return;
        }
        if should_close {
            self.folder_create = None;
            self.mark_ui_dirty();
            return;
        }

        self.mark_ui_dirty();
    }

    pub(crate) fn handle_folder_rename_key(&mut self, key: KeyEvent) {
        if self.folder_rename.is_none() {
            return;
        }

        let mut should_submit = false;
        let mut should_close = false;
        let mut should_pick_parent = false;
        if let Some(state) = self.folder_rename.as_mut() {
            state.drag_anchor = None;
            match key.code {
                KeyCode::Esc => should_close = true,
                KeyCode::Enter => should_submit = true,
                KeyCode::Tab | KeyCode::BackTab => should_pick_parent = true,
                KeyCode::Left => text_edit::move_cursor_left(&state.name, &mut state.cursor, &mut state.selection),
                KeyCode::Right => text_edit::move_cursor_right(&state.name, &mut state.cursor, &mut state.selection),
                KeyCode::Home => text_edit::move_cursor_home(&mut state.cursor, &mut state.selection),
                KeyCode::End => text_edit::move_cursor_end(&state.name, &mut state.cursor, &mut state.selection),
                KeyCode::Backspace => {
                    text_edit::backspace(&mut state.name, &mut state.cursor, &mut state.selection);
                    state.error = None;
                }
                KeyCode::Delete => {
                    text_edit::delete_char(&mut state.name, &mut state.cursor, &mut state.selection);
                    state.error = None;
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    text_edit::select_all(&state.name, &mut state.cursor, &mut state.selection)
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    text_edit::move_cursor_end(&state.name, &mut state.cursor, &mut state.selection)
                }
                KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => should_pick_parent = true,
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                    text_edit::insert_char(&mut state.name, &mut state.cursor, &mut state.selection, ch);
                    state.error = None;
                }
                _ => {}
            }
        }

        if should_pick_parent {
            self.open_folder_picker_for_rename_folder_parent();
            return;
        }
        if should_submit {
            self.submit_folder_rename();
            return;
        }
        if should_close {
            self.folder_rename = None;
            self.mark_ui_dirty();
            return;
        }

        self.mark_ui_dirty();
    }

    pub(crate) fn handle_folder_delete_confirm_key(&mut self, key: KeyEvent) {
        if self.folder_delete_confirm.is_none() {
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('n') if key.modifiers.is_empty() => {
                self.folder_delete_confirm = None;
                self.mark_ui_dirty();
            }
            KeyCode::Enter | KeyCode::Char('y') if key.modifiers.is_empty() => {
                self.confirm_folder_delete();
            }
            _ => {}
        }
    }

    pub(crate) fn handle_folder_rename_paste(&mut self, pasted: &str) {
        let Some(state) = self.folder_rename.as_mut() else {
            return;
        };

        state.drag_anchor = None;
        let filtered: String = pasted.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }

        for ch in filtered.chars() {
            text_edit::insert_char(&mut state.name, &mut state.cursor, &mut state.selection, ch);
        }
        state.error = None;
        self.mark_ui_dirty();
    }

    pub(crate) fn handle_folder_create_paste(&mut self, pasted: &str) {
        let Some(state) = self.folder_create.as_mut() else {
            return;
        };

        state.drag_anchor = None;
        let filtered: String = pasted.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }

        for ch in filtered.chars() {
            text_edit::insert_char(&mut state.name, &mut state.cursor, &mut state.selection, ch);
        }
        state.error = None;
        self.mark_ui_dirty();
    }

    pub(crate) fn folder_path_segments_by_id_in_source(&self, folder_id: FolderId, source_file: &Path) -> Option<Vec<String>> {
        let global_path = self.folder_path_segments_by_id_global(folder_id)?;
        Some(self.strip_source_file_folder_prefix(source_file, &global_path))
    }

    pub(crate) fn host_folder_path_in_source(&self, host: &InventoryHost) -> Vec<String> {
        self.strip_source_file_folder_prefix(&host.source_file, &host.source_folder_path)
    }

    pub(crate) fn format_folder_path(folder_path: &[String]) -> String {
        if folder_path.is_empty() {
            "/".to_string()
        } else {
            format!("/{}/", folder_path.join("/"))
        }
    }

    fn parse_folder_path_display(path: &str) -> Vec<String> {
        let trimmed = path.trim();
        if trimmed.is_empty() || trimmed == "/" {
            return Vec::new();
        }
        let inner = trimmed.trim_matches('/');
        if inner.is_empty() {
            return Vec::new();
        }
        inner
            .split('/')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect()
    }

    fn open_folder_picker(&mut self, source_file: PathBuf, mode: FolderPickerMode, initial_folder_path: Vec<String>) {
        let rows = self.folder_picker_rows_for_mode(&source_file, &mode);
        let selected = rows.iter().position(|row| row.folder_path == initial_folder_path).unwrap_or(0);

        self.host_context_menu = None;
        self.folder_create = None;
        self.folder_rename = None;
        self.folder_delete_confirm = None;
        self.folder_picker = Some(FolderPickerState::new(source_file, mode, rows, selected));
        self.mark_ui_dirty();
    }

    fn folder_create_state_from_picker_mode(source_file: PathBuf, parent_folder_path: Vec<String>, mode: &FolderPickerMode) -> Option<FolderCreateState> {
        let FolderPickerMode::CreateFolderParent {
            folder_name,
            name_cursor,
            name_selection,
            ..
        } = mode
        else {
            return None;
        };

        let mut state = FolderCreateState::new(source_file, parent_folder_path);
        state.name = folder_name.clone();
        state.cursor = (*name_cursor).min(text_edit::char_len(&state.name));
        state.selection = text_edit::normalized_selection(&state.name, *name_selection);
        state.drag_anchor = None;
        Some(state)
    }

    fn folder_rename_state_from_picker_mode(source_file: PathBuf, parent_folder_path: Vec<String>, mode: &FolderPickerMode) -> Option<FolderRenameState> {
        let FolderPickerMode::RenameFolderParent {
            source_folder_path,
            folder_name,
            name_cursor,
            name_selection,
            ..
        } = mode
        else {
            return None;
        };
        if source_folder_path.is_empty() {
            return None;
        }

        let mut state = FolderRenameState::new(source_file, source_folder_path.clone());
        state.parent_folder_path = parent_folder_path;
        state.name = folder_name.clone();
        state.cursor = (*name_cursor).min(text_edit::char_len(&state.name));
        state.selection = text_edit::normalized_selection(&state.name, *name_selection);
        state.drag_anchor = None;
        Some(state)
    }

    pub(crate) fn cancel_folder_picker(&mut self) {
        let Some(picker) = self.folder_picker.clone() else {
            return;
        };
        self.folder_picker = None;

        if let FolderPickerMode::CreateFolderParent { parent_folder_path, .. } = &picker.mode
            && let Some(state) = Self::folder_create_state_from_picker_mode(picker.source_file.clone(), parent_folder_path.clone(), &picker.mode)
        {
            self.folder_create = Some(state);
        }

        if let FolderPickerMode::RenameFolderParent { parent_folder_path, .. } = &picker.mode
            && let Some(state) = Self::folder_rename_state_from_picker_mode(picker.source_file.clone(), parent_folder_path.clone(), &picker.mode)
        {
            self.folder_rename = Some(state);
        }

        self.mark_ui_dirty();
    }

    pub(crate) fn submit_folder_picker(&mut self) {
        let Some(picker) = self.folder_picker.clone() else {
            return;
        };
        let source_file = picker.source_file.clone();
        let selected_folder_path = picker.selected_folder_path();
        self.folder_picker = None;

        match picker.mode {
            FolderPickerMode::CreatePlacement => {
                if let Some(form) = self.selected_host_editor_mut()
                    && form.mode == HostEditorMode::Create
                    && form.source_file == source_file
                {
                    form.folder_path.value = Self::format_folder_path(&selected_folder_path);
                    form.folder_path.cursor = text_edit::char_len(&form.folder_path.value);
                    form.folder_path.selection = None;
                    form.error = None;
                }
                self.mark_ui_dirty();
            }
            FolderPickerMode::CreateFolderParent { .. } => {
                if let Some(state) = Self::folder_create_state_from_picker_mode(source_file, selected_folder_path, &picker.mode) {
                    self.folder_create = Some(state);
                }
                self.mark_ui_dirty();
            }
            FolderPickerMode::RenameFolderParent { .. } => {
                if let Some(state) = Self::folder_rename_state_from_picker_mode(source_file, selected_folder_path, &picker.mode) {
                    self.folder_rename = Some(state);
                }
                self.mark_ui_dirty();
            }
            FolderPickerMode::MoveHost { host_name } => {
                match move_inventory_host_entry(&source_file, &host_name, &selected_folder_path) {
                    Ok(()) => {
                        let root_path = self.host_tree_root.path.clone();
                        if let Err(err) = self.reload_inventory_tree_from_path(&root_path) {
                            self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice(&format!("Moved entry, but reload failed: {err}"))));
                        } else {
                            let _ = self.select_host_row_by_name(&host_name);
                        }
                    }
                    Err(err) => {
                        self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice(&format!("Failed to move entry: {err}"))));
                    }
                }
                self.mark_ui_dirty();
            }
        }
    }

    pub(crate) fn submit_folder_rename(&mut self) {
        let Some(state) = self.folder_rename.clone() else {
            return;
        };

        let new_name = state.name.trim().to_string();
        match relocate_inventory_folder(&state.source_file, &state.folder_path, &state.parent_folder_path, &new_name) {
            Ok(()) => {
                let mut renamed_path = state.parent_folder_path.clone();
                renamed_path.push(new_name.clone());
                self.folder_rename = None;
                let root_path = self.host_tree_root.path.clone();
                if let Err(err) = self.reload_inventory_tree_from_path(&root_path) {
                    self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice(&format!(
                        "Folder renamed, but reload failed: {err}"
                    ))));
                } else {
                    let _ = self.select_folder_row_by_source_and_path(&state.source_file, &renamed_path);
                }
                self.mark_ui_dirty();
            }
            Err(err) => {
                if let Some(rename_state) = self.folder_rename.as_mut() {
                    rename_state.error = Some(format!("Failed to rename folder: {err}"));
                }
                self.mark_ui_dirty();
            }
        }
    }

    pub(crate) fn submit_folder_create(&mut self) {
        let Some(state) = self.folder_create.clone() else {
            return;
        };

        let folder_name = state.name.trim().to_string();
        match create_inventory_folder(&state.source_file, &state.parent_folder_path, &folder_name) {
            Ok(()) => {
                let mut created_path = state.parent_folder_path.clone();
                created_path.push(folder_name);
                self.folder_create = None;
                let root_path = self.host_tree_root.path.clone();
                if let Err(err) = self.reload_inventory_tree_from_path(&root_path) {
                    self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice(&format!(
                        "Folder created, but reload failed: {err}"
                    ))));
                } else {
                    let _ = self.select_folder_row_by_source_and_path(&state.source_file, &created_path);
                }
                self.mark_ui_dirty();
            }
            Err(err) => {
                if let Some(create_state) = self.folder_create.as_mut() {
                    create_state.error = Some(format!("Failed to create folder: {err}"));
                }
                self.mark_ui_dirty();
            }
        }
    }

    pub(crate) fn confirm_folder_delete(&mut self) {
        let Some(confirm) = self.folder_delete_confirm.clone() else {
            return;
        };
        self.folder_delete_confirm = None;

        match delete_inventory_folder(&confirm.source_file, &confirm.folder_path) {
            Ok(_) => {
                let root_path = self.host_tree_root.path.clone();
                if let Err(err) = self.reload_inventory_tree_from_path(&root_path) {
                    self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice(&format!(
                        "Folder deleted, but reload failed: {err}"
                    ))));
                }
            }
            Err(err) => {
                self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice(&format!("Failed to delete folder: {err}"))));
            }
        }

        self.mark_ui_dirty();
    }

    fn folder_picker_rows_for_source_file(&self, source_file: &Path) -> Vec<FolderPickerRow> {
        let mut rows = vec![FolderPickerRow {
            folder_path: Vec::new(),
            depth: 0,
            label: "/".to_string(),
        }];

        let Some(source_root) = Self::find_source_root_folder_recursive(&self.host_tree_root, source_file, false) else {
            return rows;
        };

        let mut current_path = Vec::new();
        Self::collect_folder_picker_rows(source_root, source_file, 1, &mut current_path, &mut rows);
        rows
    }

    fn folder_picker_rows_for_mode(&self, source_file: &Path, mode: &FolderPickerMode) -> Vec<FolderPickerRow> {
        let rows = self.folder_picker_rows_for_source_file(source_file);
        let FolderPickerMode::RenameFolderParent { source_folder_path, .. } = mode else {
            return rows;
        };

        rows.into_iter()
            .filter(|row| row.folder_path != *source_folder_path && !row.folder_path.starts_with(source_folder_path))
            .collect()
    }

    fn collect_folder_picker_rows(folder: &TreeFolder, source_file: &Path, depth: usize, current_path: &mut Vec<String>, rows: &mut Vec<FolderPickerRow>) {
        for child in &folder.children {
            if child.path != source_file {
                continue;
            }

            current_path.push(child.name.clone());
            rows.push(FolderPickerRow {
                folder_path: current_path.clone(),
                depth,
                label: child.name.clone(),
            });
            Self::collect_folder_picker_rows(child, source_file, depth + 1, current_path, rows);
            current_path.pop();
        }
    }

    fn find_source_root_folder_recursive<'a>(folder: &'a TreeFolder, source_file: &Path, parent_matches: bool) -> Option<&'a TreeFolder> {
        let matches_source = folder.path == source_file;
        if matches_source && !parent_matches {
            return Some(folder);
        }

        for child in &folder.children {
            if let Some(found) = Self::find_source_root_folder_recursive(child, source_file, matches_source) {
                return Some(found);
            }
        }

        None
    }

    fn source_file_folder_prefix_segments(&self, source_file: &Path) -> Vec<String> {
        if self.host_tree_root.path == source_file {
            return Vec::new();
        }

        Self::source_file_folder_prefix_segments_recursive(&self.host_tree_root, source_file, &mut Vec::new(), false).unwrap_or_default()
    }

    fn source_file_folder_prefix_segments_recursive(
        folder: &TreeFolder,
        source_file: &Path,
        path_segments: &mut Vec<String>,
        parent_matches: bool,
    ) -> Option<Vec<String>> {
        let matches_source = folder.path == source_file;
        if matches_source && !parent_matches {
            return Some(path_segments.clone());
        }

        for child in &folder.children {
            path_segments.push(child.name.clone());
            if let Some(found) = Self::source_file_folder_prefix_segments_recursive(child, source_file, path_segments, matches_source) {
                return Some(found);
            }
            path_segments.pop();
        }

        None
    }

    fn strip_source_file_folder_prefix(&self, source_file: &Path, global_folder_path: &[String]) -> Vec<String> {
        let prefix = self.source_file_folder_prefix_segments(source_file);
        if prefix.is_empty() {
            return global_folder_path.to_vec();
        }

        if global_folder_path.starts_with(prefix.as_slice()) {
            return global_folder_path[prefix.len()..].to_vec();
        }

        global_folder_path.to_vec()
    }

    fn folder_path_segments_by_id_global(&self, folder_id: FolderId) -> Option<Vec<String>> {
        let mut segments = Vec::new();
        if Self::folder_segments_for_folder_id(&self.host_tree_root, folder_id, &mut segments) {
            Some(segments)
        } else {
            None
        }
    }

    fn folder_segments_for_folder_id(folder: &TreeFolder, folder_id: FolderId, segments: &mut Vec<String>) -> bool {
        for child in &folder.children {
            segments.push(child.name.clone());
            if child.id == folder_id {
                return true;
            }
            if Self::folder_segments_for_folder_id(child, folder_id, segments) {
                return true;
            }
            let _ = segments.pop();
        }

        false
    }

    fn select_folder_row_by_source_and_path(&mut self, source_file: &Path, folder_path: &[String]) -> bool {
        let selected_row = self.visible_host_rows.iter().position(|row| {
            let HostTreeRowKind::Folder(folder_id) = row.kind else {
                return false;
            };

            self.folder_by_id(folder_id).is_some_and(|folder| folder.path == source_file)
                && self
                    .folder_path_segments_by_id_in_source(folder_id, source_file)
                    .is_some_and(|segments| segments == folder_path)
        });

        if let Some(selected_row) = selected_row {
            self.set_selected_row(selected_row);
            true
        } else {
            false
        }
    }
}
