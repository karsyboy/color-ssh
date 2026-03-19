//! Keyboard input handling and PTY write helpers.

use crate::log_error;
use crate::tui::AppState;
use crate::tui::features::terminal_session::pty::{encode_key_event_bytes, encode_paste_bytes};
use crate::tui::text_edit;
use crate::tui::ui::theme::display_width;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io;

fn tab_title_display_width(title: &str) -> usize {
    display_width(title)
}

impl AppState {
    // Selection/focus helpers.
    pub(crate) fn clear_selection_state(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.is_selecting = false;
        self.selection_dragged = false;
    }

    pub(crate) fn focus_manager_panel(&mut self) {
        self.focus_on_manager = true;
        if !self.host_panel_visible {
            self.host_panel_visible = true;
        }
    }

    // Tab lifecycle.
    pub(crate) fn move_tab(&mut self, from_idx: usize, to_idx: usize) -> bool {
        if from_idx >= self.tabs.len() || to_idx >= self.tabs.len() || from_idx == to_idx {
            return false;
        }

        let selected_before = self.selected_tab.min(self.tabs.len().saturating_sub(1));
        let moved_tab = self.tabs.remove(from_idx);
        self.tabs.insert(to_idx, moved_tab);

        self.selected_tab = if selected_before == from_idx {
            to_idx
        } else if from_idx < selected_before && to_idx >= selected_before {
            selected_before.saturating_sub(1)
        } else if from_idx > selected_before && to_idx <= selected_before {
            (selected_before + 1).min(self.tabs.len().saturating_sub(1))
        } else {
            selected_before
        };

        self.clear_selection_state();
        self.ensure_tab_visible();
        true
    }

    pub(crate) fn move_selected_tab_left(&mut self) {
        if self.selected_tab > 0 {
            let from_idx = self.selected_tab;
            let to_idx = from_idx - 1;
            let _ = self.move_tab(from_idx, to_idx);
        }
    }

    pub(crate) fn move_selected_tab_right(&mut self) {
        if self.selected_tab + 1 < self.tabs.len() {
            let from_idx = self.selected_tab;
            let to_idx = from_idx + 1;
            let _ = self.move_tab(from_idx, to_idx);
        }
    }

    pub(crate) fn close_current_tab(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let idx = self.selected_tab;
        let closing_editor_tab = self.tabs[idx].editor().is_some();
        if let Some(session) = self.terminal_tab_mut(idx).and_then(|terminal| terminal.session.take()) {
            let mut session = session;
            session.terminate();
        }

        if closing_editor_tab {
            self.host_delete_confirm = None;
        }

        self.tabs.remove(idx);
        if self.selected_tab >= self.tabs.len() && self.selected_tab > 0 {
            self.selected_tab -= 1;
        }
        self.dragging_tab = None;

        if self.tabs.is_empty() {
            self.focus_manager_panel();
        } else {
            self.ensure_tab_visible();
        }
    }

    // Top-level key routing.
    /// Handle keyboard input.
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> io::Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        let terminal_view_active = !self.focus_on_manager && !self.tabs.is_empty() && self.selected_tab < self.tabs.len();
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) && !terminal_view_active {
            self.should_exit = true;
            return Ok(());
        }

        if self.host_context_menu.is_some() {
            self.handle_host_context_menu_key(key);
            return Ok(());
        }

        if self.host_delete_confirm.is_some() {
            self.handle_host_delete_confirm_key(key);
            return Ok(());
        }

        if self.folder_picker.is_some() {
            self.handle_folder_picker_key(key);
            return Ok(());
        }

        if self.folder_create.is_some() {
            self.handle_folder_create_key(key);
            return Ok(());
        }

        if self.folder_rename.is_some() {
            self.handle_folder_rename_key(key);
            return Ok(());
        }

        if self.folder_delete_confirm.is_some() {
            self.handle_folder_delete_confirm_key(key);
            return Ok(());
        }

        if self.vault_unlock.is_some() {
            self.handle_vault_unlock_key(key);
            return Ok(());
        }

        if self.rdp_credentials.is_some() {
            self.handle_rdp_credentials_key(key);
            return Ok(());
        }

        if self.vault_status_modal.is_some() {
            self.handle_vault_status_modal_key(key);
            return Ok(());
        }

        if self.quick_connect.is_some() {
            self.handle_quick_connect_key(key);
            return Ok(());
        }

        if self.search_mode {
            return self.handle_search_key(key);
        }

        if !self.focus_on_manager && !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
            return self.handle_tab_key(key);
        }

        self.handle_manager_key(key)
    }

    pub(crate) fn handle_paste(&mut self, pasted: String) -> io::Result<()> {
        if pasted.is_empty() {
            return Ok(());
        }

        if self.vault_unlock.is_some() {
            self.handle_vault_unlock_paste(&pasted);
            return Ok(());
        }

        if self.folder_rename.is_some() {
            self.handle_folder_rename_paste(&pasted);
            return Ok(());
        }

        if self.folder_create.is_some() {
            self.handle_folder_create_paste(&pasted);
            return Ok(());
        }

        if self.rdp_credentials.is_some() {
            self.handle_rdp_credentials_paste(&pasted);
            return Ok(());
        }

        if self.vault_status_modal.is_some() {
            return Ok(());
        }

        if self.quick_connect.is_some() {
            self.handle_quick_connect_paste(&pasted);
            return Ok(());
        }

        if self.search_mode {
            self.handle_search_paste(&pasted);
            return Ok(());
        }

        if !self.focus_on_manager && self.is_selected_tab_editor() {
            self.handle_host_editor_paste(&pasted);
            return Ok(());
        }

        if !self.focus_on_manager && self.current_tab_search().map(|search_state| search_state.active).unwrap_or(false) {
            self.handle_terminal_search_paste(&pasted);
            return Ok(());
        }

        if !self.focus_on_manager && !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
            if let Some(terminal) = self.selected_terminal_tab_mut() {
                terminal.scroll_offset = 0;
            }
            self.clear_selection_state();
            let bracketed = self.pty_bracketed_paste_enabled();
            let bytes = encode_paste_bytes(&pasted, bracketed);
            self.write_bytes_to_active_pty(&bytes)?;
        }

        Ok(())
    }

    // Terminal-tab key handling.
    pub(crate) fn handle_tab_key(&mut self, key: KeyEvent) -> io::Result<()> {
        if self.is_selected_tab_editor() {
            match key.code {
                KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.host_panel_visible = !self.host_panel_visible;
                }
                KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.close_current_tab();
                }
                KeyCode::BackTab => {
                    self.focus_manager_panel();
                }
                KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.move_selected_tab_left();
                }
                KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.move_selected_tab_right();
                }
                KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                    if self.selected_tab > 0 {
                        self.selected_tab -= 1;
                        self.clear_selection_state();
                        self.ensure_tab_visible();
                    }
                }
                KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                    if self.selected_tab < self.tabs.len().saturating_sub(1) {
                        self.selected_tab += 1;
                        self.clear_selection_state();
                        self.ensure_tab_visible();
                    }
                }
                _ => self.handle_host_editor_key(key),
            }
            return Ok(());
        }

        if !self.is_selected_tab_terminal() {
            return Ok(());
        }

        if self.current_tab_search().map(|search_state| search_state.active).unwrap_or(false) {
            return self.handle_terminal_search_key(key);
        }

        match key.code {
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.host_panel_visible = !self.host_panel_visible;
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::ALT => {
                if self.current_selection().is_some() {
                    self.copy_selection_to_clipboard();
                    self.clear_selection_state();
                }
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.close_current_tab();
            }
            KeyCode::Tab if key.modifiers.is_empty() => {
                self.send_key_to_pty(key)?;
            }
            KeyCode::BackTab => {
                self.focus_manager_panel();
            }
            KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selected_tab_left();
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_selected_tab_right();
            }
            KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                if self.selected_tab > 0 {
                    self.selected_tab -= 1;
                    self.clear_selection_state();
                    self.ensure_tab_visible();
                }
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                if self.selected_tab < self.tabs.len() - 1 {
                    self.selected_tab += 1;
                    self.clear_selection_state();
                    self.ensure_tab_visible();
                }
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.is_pty_mouse_mode_active() {
                    let mut should_recompute_search = false;
                    if let Some(search) = self.current_tab_search_mut() {
                        search.active = true;
                        search.query_cursor = text_edit::char_len(&search.query);
                        search.query_selection = None;
                        search.last_search_query.clear();
                        search.last_scanned_render_epoch = 0;
                        should_recompute_search = !search.query.is_empty();
                    }
                    if should_recompute_search {
                        self.update_terminal_search();
                    }
                } else {
                    self.send_key_to_pty(key)?;
                }
            }
            KeyCode::PageUp if key.modifiers.contains(KeyModifiers::SHIFT) => {
                let max_scrollback = self.max_scrollback_for_tab(self.selected_tab);
                if let Some(terminal) = self.selected_terminal_tab_mut() {
                    terminal.scroll_offset = terminal.scroll_offset.saturating_add(10).min(max_scrollback);
                }
            }
            KeyCode::PageDown if key.modifiers.contains(KeyModifiers::SHIFT) => {
                if let Some(terminal) = self.selected_terminal_tab_mut() {
                    terminal.scroll_offset = terminal.scroll_offset.saturating_sub(10);
                }
            }
            KeyCode::Enter => {
                let is_exited = self
                    .selected_terminal_tab()
                    .and_then(|terminal| terminal.session.as_ref())
                    .map(|session| session.is_exited())
                    .unwrap_or(true);

                if is_exited {
                    self.reconnect_session();
                } else {
                    if let Some(terminal) = self.selected_terminal_tab_mut() {
                        terminal.scroll_offset = 0;
                    }
                    self.clear_selection_state();
                    self.send_key_to_pty(key)?;
                }
            }
            _ => {
                if let Some(terminal) = self.selected_terminal_tab_mut() {
                    terminal.scroll_offset = 0;
                }
                self.clear_selection_state();
                self.send_key_to_pty(key)?;
            }
        }

        Ok(())
    }

    // PTY write helpers.
    pub(crate) fn write_bytes_to_active_pty(&mut self, bytes: &[u8]) -> io::Result<()> {
        let Some(terminal) = self.selected_terminal_tab_mut() else {
            return Ok(());
        };
        let Some(session) = &mut terminal.session else {
            return Ok(());
        };
        if let Err(err) = session.write_input(bytes) {
            log_error!("Failed to write to PTY: {}", err);
            session.mark_exited();
            return Ok(());
        }

        Ok(())
    }

    fn pty_bracketed_paste_enabled(&self) -> bool {
        let Some(terminal) = self.selected_terminal_tab() else {
            return false;
        };
        let Some(session) = &terminal.session else {
            return false;
        };

        if let Ok(engine) = session.engine().lock() {
            return engine.view_model().bracketed_paste_enabled();
        }

        false
    }

    // Key-event encoding dispatch.
    pub(crate) fn send_key_to_pty(&mut self, key: KeyEvent) -> io::Result<()> {
        let Some(bytes) = encode_key_event_bytes(key) else {
            return Ok(());
        };

        self.write_bytes_to_active_pty(&bytes)
    }

    // Tab strip geometry helpers.
    pub(crate) fn tab_display_width(&self, idx: usize) -> usize {
        if idx >= self.tabs.len() {
            return 0;
        }
        self.tab_title_display_width(idx) + 3
    }

    pub(crate) fn tab_title_display_width(&self, idx: usize) -> usize {
        if idx >= self.tabs.len() {
            return 0;
        }
        tab_title_display_width(&self.tabs[idx].title())
    }

    // Keep selected tab visible after focus/selection moves.
    pub(crate) fn ensure_tab_visible(&mut self) {
        if self.tabs.is_empty() {
            self.tab_scroll_offset = 0;
            return;
        }

        let tab_bar_width = self.tab_bar_area.width as usize;
        if tab_bar_width == 0 {
            return;
        }

        let selected_start = self.tab_start_offset(self.selected_tab);
        let selected_width = self.tab_display_width(self.selected_tab);
        let selected_end = selected_start + selected_width;

        let mut target_offset = self.normalize_tab_scroll_offset(self.tab_scroll_offset, tab_bar_width);
        if selected_start < target_offset {
            target_offset = selected_start;
        } else {
            loop {
                let metrics = self.tab_bar_viewport_metrics(target_offset, tab_bar_width);
                if metrics.visible_tab_width == 0 {
                    break;
                }

                if selected_width >= metrics.visible_tab_width || selected_end <= metrics.scroll_offset + metrics.visible_tab_width {
                    target_offset = metrics.scroll_offset;
                    break;
                }

                let Some(next_offset) = self.next_tab_scroll_offset(metrics.scroll_offset, tab_bar_width) else {
                    target_offset = metrics.scroll_offset;
                    break;
                };
                if next_offset == metrics.scroll_offset {
                    target_offset = metrics.scroll_offset;
                    break;
                }
                target_offset = next_offset;
            }
        }

        self.tab_scroll_offset = self.normalize_tab_scroll_offset(target_offset, tab_bar_width);
    }
}
