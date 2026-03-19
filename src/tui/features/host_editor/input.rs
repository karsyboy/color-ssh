//! Host editor keyboard handling and persistence integration.

use crate::auth::vault;
use crate::inventory::{FolderId, create_inventory_host_entry, delete_inventory_host_entry, update_inventory_host_entry};
use crate::runtime::{ReloadNoticeToast, format_reload_notice};
use crate::tui::{
    AppState, EditorTabId, EditorTabState, HostContextMenuAction, HostContextMenuState, HostContextMenuTarget, HostDeleteConfirmState, HostEditorField,
    HostEditorMode, HostEditorState, HostEditorVisibleItem, HostTab, HostTreeRowKind,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

impl AppState {
    pub(crate) fn open_host_context_menu_for_selected_host(&mut self, column: u16, row: u16, host_idx: usize) {
        self.host_context_menu = Some(HostContextMenuState::for_host(column, row, host_idx));
        self.mark_ui_dirty();
    }

    pub(crate) fn open_host_context_menu_for_folder(&mut self, column: u16, row: u16, folder_id: FolderId, source_file: PathBuf) {
        self.host_context_menu = Some(HostContextMenuState::for_folder(column, row, folder_id, source_file));
        self.mark_ui_dirty();
    }

    pub(crate) fn open_host_context_menu_for_new_entry(&mut self, column: u16, row: u16, source_file: PathBuf) {
        self.host_context_menu = Some(HostContextMenuState::for_new_entry(column, row, source_file));
        self.mark_ui_dirty();
    }

    pub(crate) fn selected_source_file_for_new_entry(&self) -> PathBuf {
        if let Some(host_idx) = self.selected_host_idx()
            && let Some(host) = self.hosts.get(host_idx)
        {
            return host.source_file.clone();
        }

        if let Some(folder_id) = self.selected_folder_id()
            && let Some(folder) = self.folder_by_id(folder_id)
        {
            return folder.path.clone();
        }

        self.host_tree_root.path.clone()
    }

    fn find_editor_tab_index(&self, target_id: &EditorTabId) -> Option<usize> {
        self.tabs
            .iter()
            .position(|tab| tab.editor().is_some_and(|editor_tab| &editor_tab.id == target_id))
    }

    fn open_or_focus_host_editor_tab(&mut self, editor_id: EditorTabId, editor_state: HostEditorState) {
        self.host_context_menu = None;
        self.host_delete_confirm = None;
        self.folder_picker = None;
        self.folder_create = None;
        self.folder_rename = None;
        self.folder_delete_confirm = None;

        if let Some(tab_idx) = self.find_editor_tab_index(&editor_id) {
            self.selected_tab = tab_idx;
            self.focus_on_manager = false;
            self.search_mode = false;
            self.quick_connect = None;
            self.ensure_tab_visible();
            self.mark_ui_dirty();
            return;
        }

        self.tabs.push(HostTab::new_editor(EditorTabState { id: editor_id, editor_state }));

        self.selected_tab = self.tabs.len().saturating_sub(1);
        self.focus_on_manager = false;
        self.search_mode = false;
        self.quick_connect = None;
        self.ensure_tab_visible();
        self.mark_ui_dirty();
    }

    pub(crate) fn close_selected_editor_tab(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() || self.tabs[self.selected_tab].editor().is_none() {
            return;
        }

        self.host_delete_confirm = None;
        self.tabs.remove(self.selected_tab);

        if self.selected_tab >= self.tabs.len() && self.selected_tab > 0 {
            self.selected_tab -= 1;
        }

        if self.tabs.is_empty() {
            self.focus_manager_panel();
        } else {
            self.ensure_tab_visible();
        }

        self.mark_ui_dirty();
    }

    pub(crate) fn open_host_editor_for_selected_host(&mut self) {
        let Some(host_idx) = self.selected_host_idx() else {
            return;
        };
        self.open_host_editor_for_host_idx(host_idx);
    }

    pub(crate) fn open_host_editor_for_selected_host_duplicate(&mut self) {
        let Some(host_idx) = self.selected_host_idx() else {
            return;
        };
        self.open_host_editor_for_duplicate_host_idx(host_idx);
    }

    fn open_host_editor_for_host_idx(&mut self, host_idx: usize) {
        let Some(host) = self.hosts.get(host_idx).cloned() else {
            return;
        };

        let profiles = self.discover_quick_connect_profiles();
        let vault_entries = self.discover_host_editor_vault_pass_entries();
        let editor_state = HostEditorState::new_edit(&host, profiles, vault_entries);
        let editor_id = EditorTabId::for_existing_host(&host);
        self.open_or_focus_host_editor_tab(editor_id, editor_state);
    }

    fn open_host_editor_for_duplicate_host_idx(&mut self, host_idx: usize) {
        let Some(host) = self.hosts.get(host_idx).cloned() else {
            return;
        };

        let profiles = self.discover_quick_connect_profiles();
        let vault_entries = self.discover_host_editor_vault_pass_entries();
        let mut editor_state = HostEditorState::new_duplicate(&host, profiles, vault_entries);
        let folder_path = self.host_folder_path_in_source(&host);
        editor_state.folder_path.value = Self::format_folder_path(&folder_path);
        editor_state.folder_path.cursor = editor_state.folder_path.value.chars().count();
        editor_state.folder_path.selection = None;
        let editor_id = EditorTabId::for_duplicate_host(&host);
        self.open_or_focus_host_editor_tab(editor_id, editor_state);
    }

    pub(crate) fn open_host_editor_for_new_entry(&mut self, source_file: PathBuf) {
        self.open_host_editor_for_new_entry_with_folder_path(source_file, Vec::new());
    }

    fn open_host_editor_for_new_entry_with_folder_path(&mut self, source_file: PathBuf, folder_path: Vec<String>) {
        let profiles = self.discover_quick_connect_profiles();
        let vault_entries = self.discover_host_editor_vault_pass_entries();
        let mut editor_state = HostEditorState::new_create(source_file.clone(), profiles, vault_entries);
        editor_state.folder_path.value = Self::format_folder_path(&folder_path);
        editor_state.folder_path.cursor = editor_state.folder_path.value.chars().count();
        editor_state.folder_path.selection = None;
        let editor_id = EditorTabId::for_new_entry(source_file);
        self.open_or_focus_host_editor_tab(editor_id, editor_state);
    }

    pub(crate) fn open_host_editor_for_new_entry_from_selection(&mut self) {
        let source_file = self.selected_source_file_for_new_entry();
        self.open_host_editor_for_new_entry(source_file);
    }

    fn discover_host_editor_vault_pass_entries(&self) -> Vec<String> {
        vault::list_entries().unwrap_or_default()
    }

    fn select_visible_host_row_for_host_idx(&mut self, host_idx: usize) -> bool {
        let Some(row_idx) = self
            .visible_host_rows
            .iter()
            .position(|row| matches!(row.kind, HostTreeRowKind::Host(idx) if idx == host_idx))
        else {
            return false;
        };

        self.set_selected_row(row_idx);
        true
    }

    fn open_host_delete_confirmation_with_target(&mut self, source_file: PathBuf, host_name: String, from_editor: bool) {
        let host_name = host_name.trim().to_string();
        if host_name.is_empty() {
            return;
        }

        self.host_context_menu = None;
        self.host_delete_confirm = Some(HostDeleteConfirmState {
            source_file,
            host_name,
            from_editor,
        });
        self.mark_ui_dirty();
    }

    fn open_host_delete_confirmation_for_host_idx(&mut self, host_idx: usize) {
        let Some(host) = self.hosts.get(host_idx) else {
            return;
        };

        self.open_host_delete_confirmation_with_target(host.source_file.clone(), host.name.clone(), false);
    }

    pub(crate) fn open_host_delete_confirmation_for_selected_host(&mut self) {
        let Some(host_idx) = self.selected_host_idx() else {
            return;
        };

        self.open_host_delete_confirmation_for_host_idx(host_idx);
    }

    pub(crate) fn open_folder_rename_for_selected_folder(&mut self) {
        let Some(folder_id) = self.selected_folder_id() else {
            return;
        };
        let Some(folder) = self.folder_by_id(folder_id) else {
            return;
        };

        let source_file = folder.path.clone();
        let folder_path = self.folder_path_segments_by_id_in_source(folder_id, &source_file).unwrap_or_default();
        self.open_folder_rename_modal(source_file, folder_path);
    }

    pub(crate) fn open_folder_delete_confirmation_for_selected_folder(&mut self) {
        let Some(folder_id) = self.selected_folder_id() else {
            return;
        };
        let Some(folder) = self.folder_by_id(folder_id) else {
            return;
        };

        let source_file = folder.path.clone();
        let folder_path = self.folder_path_segments_by_id_in_source(folder_id, &source_file).unwrap_or_default();
        let folder_name = folder.name.clone();
        let removed_entry_count = self.folder_descendant_host_count(folder_id);
        self.open_folder_delete_confirm_modal(source_file, folder_path, folder_name, removed_entry_count);
    }

    pub(crate) fn handle_host_context_menu_key(&mut self, key: KeyEvent) {
        let mut should_close = false;
        let mut action = None;

        if let Some(menu) = self.host_context_menu.as_mut() {
            match key.code {
                KeyCode::Esc => should_close = true,
                KeyCode::Tab | KeyCode::Down => menu.select_next(),
                KeyCode::BackTab | KeyCode::Up => menu.select_prev(),
                KeyCode::Enter => action = menu.selected_action(),
                KeyCode::Char('c') if key.modifiers.is_empty() && menu.has_action(HostContextMenuAction::Connect) => {
                    action = Some(HostContextMenuAction::Connect);
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) && menu.has_action(HostContextMenuAction::DeleteEntry) => {
                    action = Some(HostContextMenuAction::DeleteEntry);
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) && menu.has_action(HostContextMenuAction::DeleteFolder) => {
                    action = Some(HostContextMenuAction::DeleteFolder);
                }
                KeyCode::Char('e') if key.modifiers.is_empty() && menu.has_action(HostContextMenuAction::EditEntry) => {
                    action = Some(HostContextMenuAction::EditEntry);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) && menu.has_action(HostContextMenuAction::DuplicateEntry) => {
                    action = Some(HostContextMenuAction::DuplicateEntry);
                }
                KeyCode::Char('u') if key.modifiers.is_empty() && menu.has_action(HostContextMenuAction::DuplicateEntry) => {
                    action = Some(HostContextMenuAction::DuplicateEntry);
                }
                KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) && menu.has_action(HostContextMenuAction::MoveToFolder) => {
                    action = Some(HostContextMenuAction::MoveToFolder);
                }
                KeyCode::Char('m') if key.modifiers.is_empty() && menu.has_action(HostContextMenuAction::MoveToFolder) => {
                    action = Some(HostContextMenuAction::MoveToFolder);
                }
                KeyCode::Char('n') if key.modifiers.is_empty() && menu.has_action(HostContextMenuAction::NewEntryInFolder) => {
                    action = Some(HostContextMenuAction::NewEntryInFolder);
                }
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) && menu.has_action(HostContextMenuAction::NewFolder) => {
                    action = Some(HostContextMenuAction::NewFolder);
                }
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) && menu.has_action(HostContextMenuAction::RenameFolder) => {
                    action = Some(HostContextMenuAction::RenameFolder);
                }
                _ => {}
            }
        }

        if should_close {
            self.host_context_menu = None;
            self.mark_ui_dirty();
            return;
        }

        if let Some(action) = action {
            self.execute_host_context_menu_action(action);
        }
    }

    pub(crate) fn execute_host_context_menu_action(&mut self, action: HostContextMenuAction) {
        let Some(menu) = self.host_context_menu.clone() else {
            return;
        };
        self.host_context_menu = None;

        match action {
            HostContextMenuAction::EditEntry => {
                if let HostContextMenuTarget::Host { host_idx } = menu.target {
                    self.open_host_editor_for_host_idx(host_idx);
                }
            }
            HostContextMenuAction::DuplicateEntry => {
                if let HostContextMenuTarget::Host { host_idx } = menu.target {
                    self.open_host_editor_for_duplicate_host_idx(host_idx);
                }
            }
            HostContextMenuAction::MoveToFolder => {
                if let HostContextMenuTarget::Host { host_idx } = menu.target {
                    self.open_folder_picker_for_move_host(host_idx);
                }
            }
            HostContextMenuAction::DeleteEntry => {
                if let HostContextMenuTarget::Host { host_idx } = menu.target {
                    self.open_host_delete_confirmation_for_host_idx(host_idx);
                }
            }
            HostContextMenuAction::Connect => {
                if let HostContextMenuTarget::Host { host_idx } = menu.target
                    && self.select_visible_host_row_for_host_idx(host_idx)
                {
                    self.select_host_to_connect();
                }
            }
            HostContextMenuAction::NewEntryInFolder => match menu.target {
                HostContextMenuTarget::Folder { folder_id, source_file } => {
                    let folder_path = self.folder_path_segments_by_id_in_source(folder_id, &source_file).unwrap_or_default();
                    self.open_host_editor_for_new_entry_with_folder_path(source_file, folder_path);
                }
                HostContextMenuTarget::Background { source_file } => {
                    self.open_host_editor_for_new_entry(source_file);
                }
                HostContextMenuTarget::Host { host_idx } => {
                    let Some(host) = self.hosts.get(host_idx).cloned() else {
                        return;
                    };
                    let folder_path = self.host_folder_path_in_source(&host);
                    self.open_host_editor_for_new_entry_with_folder_path(host.source_file.clone(), folder_path);
                }
            },
            HostContextMenuAction::NewFolder => match menu.target {
                HostContextMenuTarget::Folder { folder_id, source_file } => {
                    let parent_folder_path = self.folder_path_segments_by_id_in_source(folder_id, &source_file).unwrap_or_default();
                    self.open_folder_create_modal(source_file, parent_folder_path);
                }
                HostContextMenuTarget::Background { source_file } => {
                    self.open_folder_create_modal(source_file, Vec::new());
                }
                HostContextMenuTarget::Host { host_idx } => {
                    let Some(host) = self.hosts.get(host_idx).cloned() else {
                        return;
                    };
                    let parent_folder_path = self.host_folder_path_in_source(&host);
                    self.open_folder_create_modal(host.source_file.clone(), parent_folder_path);
                }
            },
            HostContextMenuAction::RenameFolder => {
                if let HostContextMenuTarget::Folder { folder_id, source_file } = menu.target {
                    let folder_path = self.folder_path_segments_by_id_in_source(folder_id, &source_file).unwrap_or_default();
                    self.open_folder_rename_modal(source_file, folder_path);
                }
            }
            HostContextMenuAction::DeleteFolder => {
                if let HostContextMenuTarget::Folder { folder_id, source_file } = menu.target {
                    let folder_path = self.folder_path_segments_by_id_in_source(folder_id, &source_file).unwrap_or_default();
                    let folder_name = self
                        .folder_by_id(folder_id)
                        .map(|folder| folder.name.clone())
                        .unwrap_or_else(|| "folder".to_string());
                    let removed_entry_count = self.folder_descendant_host_count(folder_id);
                    self.open_folder_delete_confirm_modal(source_file, folder_path, folder_name, removed_entry_count);
                }
            }
        }
    }

    pub(crate) fn handle_host_editor_key(&mut self, key: KeyEvent) {
        if self.host_delete_confirm.is_some() {
            self.handle_host_delete_confirm_key(key);
            return;
        }

        let mut should_submit = false;
        let mut should_close = false;
        let mut should_open_delete_confirm = false;

        if let Some(form) = self.selected_host_editor_mut() {
            form.finish_mouse_selection();
            match key.code {
                KeyCode::Esc => {
                    should_close = true;
                }
                KeyCode::Tab | KeyCode::Down => form.select_next_field(),
                KeyCode::BackTab | KeyCode::Up => form.select_prev_field(),
                KeyCode::Enter => match form.selected {
                    HostEditorVisibleItem::SectionHeader(section) => form.toggle_section(section),
                    HostEditorVisibleItem::Field(field) => match field {
                        HostEditorField::Protocol => form.toggle_protocol_forward(),
                        HostEditorField::Profile => form.select_next_profile(),
                        HostEditorField::VaultPass => form.select_next_vault_pass(),
                        HostEditorField::FolderPath => self.open_folder_picker_for_editor_placement(),
                        HostEditorField::Save => should_submit = true,
                        HostEditorField::Delete => should_open_delete_confirm = true,
                        HostEditorField::Cancel => should_close = true,
                        HostEditorField::IdentitiesOnly => form.cycle_identities_only_forward(),
                        _ => form.select_next_field(),
                    },
                },
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) && form.mode == HostEditorMode::Edit => {
                    should_open_delete_confirm = true;
                }
                KeyCode::Char(' ') if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => match form.selected {
                    HostEditorVisibleItem::SectionHeader(section) => form.toggle_section(section),
                    HostEditorVisibleItem::Field(HostEditorField::IdentitiesOnly) => form.cycle_identities_only_forward(),
                    HostEditorVisibleItem::Field(HostEditorField::FolderPath) => {
                        self.open_folder_picker_for_editor_placement();
                    }
                    HostEditorVisibleItem::Field(HostEditorField::Description) => {
                        form.insert_char(HostEditorField::Description, ' ');
                        form.error = None;
                    }
                    HostEditorVisibleItem::Field(_) => {}
                },
                KeyCode::Left => match form.selected_field() {
                    Some(HostEditorField::Protocol) => form.toggle_protocol_backward(),
                    Some(HostEditorField::Profile) => form.select_prev_profile(),
                    Some(HostEditorField::VaultPass) => form.select_prev_vault_pass(),
                    Some(HostEditorField::IdentitiesOnly) => form.cycle_identities_only_backward(),
                    Some(field) if field != HostEditorField::FolderPath => form.move_cursor_left(field),
                    Some(_) => {}
                    None => {}
                },
                KeyCode::Right => match form.selected_field() {
                    Some(HostEditorField::Protocol) => form.toggle_protocol_forward(),
                    Some(HostEditorField::Profile) => form.select_next_profile(),
                    Some(HostEditorField::VaultPass) => form.select_next_vault_pass(),
                    Some(HostEditorField::IdentitiesOnly) => form.cycle_identities_only_forward(),
                    Some(field) if field != HostEditorField::FolderPath => form.move_cursor_right(field),
                    Some(_) => {}
                    None => {}
                },
                KeyCode::Home => {
                    if let Some(field) = form.selected_field()
                        && field != HostEditorField::Protocol
                        && field != HostEditorField::FolderPath
                    {
                        form.move_cursor_home(field);
                    }
                }
                KeyCode::End => {
                    if let Some(field) = form.selected_field()
                        && field != HostEditorField::Protocol
                        && field != HostEditorField::FolderPath
                    {
                        form.move_cursor_end(field);
                    }
                }
                KeyCode::Backspace => {
                    if let Some(field) = form.selected_field()
                        && field != HostEditorField::Protocol
                        && field != HostEditorField::FolderPath
                    {
                        form.backspace(field);
                        form.error = None;
                    }
                }
                KeyCode::Delete => {
                    if let Some(field) = form.selected_field()
                        && field != HostEditorField::Protocol
                        && field != HostEditorField::FolderPath
                    {
                        form.delete(field);
                        form.error = None;
                    }
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(field) = form.selected_field()
                        && field != HostEditorField::Protocol
                        && field != HostEditorField::FolderPath
                    {
                        form.move_cursor_home(field);
                    }
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(field) = form.selected_field()
                        && field != HostEditorField::Protocol
                        && field != HostEditorField::FolderPath
                    {
                        form.move_cursor_end(field);
                    }
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                    if let Some(field) = form.selected_field()
                        && field != HostEditorField::Protocol
                        && field != HostEditorField::FolderPath
                        && form.text_field(field).is_some()
                    {
                        form.insert_char(field, ch);
                        form.error = None;
                    }
                }
                _ => {}
            }
        }

        if should_submit {
            self.submit_host_editor();
        } else if should_open_delete_confirm {
            self.open_host_delete_confirmation();
        } else if should_close {
            self.host_delete_confirm = None;
            self.close_selected_editor_tab();
        }
    }

    pub(crate) fn handle_host_editor_paste(&mut self, pasted: &str) {
        if self.host_delete_confirm.is_some() {
            return;
        }

        let Some(form) = self.selected_host_editor_mut() else {
            return;
        };
        form.finish_mouse_selection();

        let Some(field) = form.selected_field() else {
            return;
        };
        if field == HostEditorField::Protocol || field == HostEditorField::FolderPath || form.text_field(field).is_none() {
            return;
        }

        let filtered: String = pasted.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }

        let pasted = if field == HostEditorField::Description {
            filtered
        } else {
            filtered.chars().filter(|ch| *ch != ' ').collect::<String>()
        };

        if pasted.is_empty() {
            return;
        }

        for ch in pasted.chars() {
            form.insert_char(field, ch);
        }
        form.error = None;
    }

    pub(crate) fn open_host_delete_confirmation(&mut self) {
        let Some(form) = self.selected_host_editor() else {
            return;
        };

        if form.mode != HostEditorMode::Edit {
            return;
        }

        let host_name = form
            .original_name
            .clone()
            .or_else(|| {
                let trimmed = form.name.value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .unwrap_or_else(|| "entry".to_string());

        self.open_host_delete_confirmation_with_target(form.source_file.clone(), host_name, true);
    }

    pub(crate) fn handle_host_delete_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') if key.modifiers.is_empty() => {
                self.host_delete_confirm = None;
                self.mark_ui_dirty();
            }
            KeyCode::Enter | KeyCode::Char('y') if key.modifiers.is_empty() => {
                self.confirm_host_delete();
            }
            _ => {}
        }
    }

    fn confirm_host_delete(&mut self) {
        let Some(confirm) = self.host_delete_confirm.clone() else {
            return;
        };

        let source_file = confirm.source_file;
        let host_name = confirm.host_name.trim().to_string();
        let from_editor = confirm.from_editor;

        if host_name.is_empty() {
            self.host_delete_confirm = None;
            if from_editor {
                if let Some(form) = self.selected_host_editor_mut() {
                    form.error = Some("Cannot delete: host name is empty.".to_string());
                }
            } else {
                self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice("Cannot delete: host name is empty.")));
            }
            self.mark_ui_dirty();
            return;
        }

        match delete_inventory_host_entry(&source_file, &host_name) {
            Ok(()) => {
                self.host_delete_confirm = None;
                let root_path = self.host_tree_root.path.clone();
                if let Err(err) = self.reload_inventory_tree_from_path(&root_path) {
                    if from_editor {
                        if let Some(form) = self.selected_host_editor_mut() {
                            form.error = Some(format!("Deleted entry, but reload failed: {err}"));
                        }
                    } else {
                        self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice(&format!(
                            "Deleted entry, but reload failed: {err}"
                        ))));
                    }
                    self.mark_ui_dirty();
                } else if from_editor {
                    self.close_selected_editor_tab();
                }
            }
            Err(err) => {
                self.host_delete_confirm = None;
                if from_editor {
                    if let Some(form) = self.selected_host_editor_mut() {
                        form.error = Some(format!("Failed to delete entry: {err}"));
                    }
                } else {
                    self.reload_notice_toast = Some(ReloadNoticeToast::new(format_reload_notice(&format!("Failed to delete entry: {err}"))));
                }
                self.mark_ui_dirty();
            }
        }
    }

    pub(crate) fn submit_host_editor(&mut self) {
        let Some(form) = self.selected_host_editor() else {
            return;
        };

        let submission = match form.build_submission() {
            Ok(submission) => submission,
            Err(err) => {
                if let Some(form) = self.selected_host_editor_mut() {
                    form.error = Some(err.message());
                }
                self.mark_ui_dirty();
                return;
            }
        };

        if self.host_name_exists(&submission.host.name, submission.original_name.as_deref()) {
            if let Some(form) = self.selected_host_editor_mut() {
                form.error = Some(format!("Host '{}' already exists.", submission.host.name));
            }
            self.mark_ui_dirty();
            return;
        }

        let operation_result = if let Some(original_name) = submission.original_name.as_deref() {
            update_inventory_host_entry(&submission.source_file, original_name, &submission.host)
        } else {
            create_inventory_host_entry(&submission.source_file, &submission.folder_path, &submission.host)
        };

        if let Err(err) = operation_result {
            if let Some(form) = self.selected_host_editor_mut() {
                form.error = Some(format!("Failed to save entry: {err}"));
            }
            self.mark_ui_dirty();
            return;
        }

        let root_path = self.host_tree_root.path.clone();
        if let Err(err) = self.reload_inventory_tree_from_path(&root_path) {
            if let Some(form) = self.selected_host_editor_mut() {
                form.error = Some(format!("Entry saved, but reload failed: {err}"));
            }
            self.mark_ui_dirty();
            return;
        }

        let host_name = submission.host.name.clone();
        let _ = self.select_host_row_by_name(&host_name);
        self.host_delete_confirm = None;
        self.close_selected_editor_tab();
    }

    fn host_name_exists(&self, candidate_name: &str, original_name: Option<&str>) -> bool {
        self.hosts.iter().any(|host| {
            if host.name != candidate_name {
                return false;
            }

            if let Some(original_name) = original_name {
                host.name != original_name
            } else {
                true
            }
        })
    }

    pub(crate) fn select_host_row_by_name(&mut self, name: &str) -> bool {
        let selected_row = self.visible_host_rows.iter().position(|row| {
            if let crate::tui::HostTreeRowKind::Host(host_idx) = row.kind {
                return self.hosts.get(host_idx).is_some_and(|host| host.name == name);
            }

            false
        });

        if let Some(selected_row) = selected_row {
            self.set_selected_row(selected_row);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
#[path = "../../../test/tui/features/host_editor/input.rs"]
mod tests;
