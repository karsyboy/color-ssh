//! Host browser keyboard handling.

use crate::tui::{ConnectRequest, HostTreeRowKind, SessionManager};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io;

fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn clamp_cursor(text: &str, cursor: &mut usize) {
    *cursor = (*cursor).min(char_len(text));
}

fn normalized_selection(text: &str, selection: Option<(usize, usize)>) -> Option<(usize, usize)> {
    let (start, end) = selection?;
    let len = char_len(text);
    let start = start.min(len);
    let end = end.min(len);
    if start == end {
        None
    } else if start < end {
        Some((start, end))
    } else {
        Some((end, start))
    }
}

fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }

    let max = char_len(text);
    let clamped = char_index.min(max);
    if clamped == max {
        return text.len();
    }

    text.char_indices().nth(clamped).map_or(text.len(), |(byte_index, _)| byte_index)
}

fn delete_selection(text: &mut String, cursor: &mut usize, selection: &mut Option<(usize, usize)>) -> bool {
    let Some((start, end)) = normalized_selection(text, *selection) else {
        *selection = None;
        return false;
    };

    let start_byte = byte_index_for_char(text, start);
    let end_byte = byte_index_for_char(text, end);
    text.replace_range(start_byte..end_byte, "");
    *cursor = start;
    *selection = None;
    true
}

impl SessionManager {
    fn clear_host_search_query(&mut self) {
        self.search_query.clear();
        self.search_query_cursor = 0;
        self.search_query_selection = None;
    }

    fn move_host_search_cursor_left(&mut self) {
        clamp_cursor(&self.search_query, &mut self.search_query_cursor);
        let active_selection = normalized_selection(&self.search_query, self.search_query_selection);
        self.search_query_selection = None;
        if let Some((start, _)) = active_selection {
            self.search_query_cursor = start;
        } else if self.search_query_cursor > 0 {
            self.search_query_cursor -= 1;
        }
    }

    fn move_host_search_cursor_right(&mut self) {
        clamp_cursor(&self.search_query, &mut self.search_query_cursor);
        let len = char_len(&self.search_query);
        let active_selection = normalized_selection(&self.search_query, self.search_query_selection);
        self.search_query_selection = None;
        if let Some((_, end)) = active_selection {
            self.search_query_cursor = end;
        } else if self.search_query_cursor < len {
            self.search_query_cursor += 1;
        }
    }

    fn move_host_search_cursor_home(&mut self) {
        self.search_query_cursor = 0;
        self.search_query_selection = None;
    }

    fn move_host_search_cursor_end(&mut self) {
        self.search_query_cursor = char_len(&self.search_query);
        self.search_query_selection = None;
    }

    fn select_all_host_search_text(&mut self) {
        let len = char_len(&self.search_query);
        if len == 0 {
            self.search_query_selection = None;
            self.search_query_cursor = 0;
        } else {
            self.search_query_selection = Some((0, len));
            self.search_query_cursor = len;
        }
    }

    fn insert_host_search_char(&mut self, ch: char) -> bool {
        let _ = delete_selection(&mut self.search_query, &mut self.search_query_cursor, &mut self.search_query_selection);
        clamp_cursor(&self.search_query, &mut self.search_query_cursor);
        let insert_at = byte_index_for_char(&self.search_query, self.search_query_cursor);
        self.search_query.insert(insert_at, ch);
        self.search_query_cursor += 1;
        self.search_query_selection = None;
        true
    }

    fn backspace_host_search_text(&mut self) -> bool {
        if delete_selection(&mut self.search_query, &mut self.search_query_cursor, &mut self.search_query_selection) {
            return true;
        }

        clamp_cursor(&self.search_query, &mut self.search_query_cursor);
        if self.search_query_cursor == 0 {
            self.search_query_selection = None;
            return false;
        }

        let end = byte_index_for_char(&self.search_query, self.search_query_cursor);
        let start = byte_index_for_char(&self.search_query, self.search_query_cursor - 1);
        self.search_query.replace_range(start..end, "");
        self.search_query_cursor -= 1;
        self.search_query_selection = None;
        true
    }

    fn delete_host_search_text(&mut self) -> bool {
        if delete_selection(&mut self.search_query, &mut self.search_query_cursor, &mut self.search_query_selection) {
            return true;
        }

        clamp_cursor(&self.search_query, &mut self.search_query_cursor);
        let len = char_len(&self.search_query);
        if self.search_query_cursor >= len {
            self.search_query_selection = None;
            return false;
        }

        let start = byte_index_for_char(&self.search_query, self.search_query_cursor);
        let end = byte_index_for_char(&self.search_query, self.search_query_cursor + 1);
        self.search_query.replace_range(start..end, "");
        self.search_query_selection = None;
        true
    }

    // Search-mode input.
    pub(crate) fn handle_search_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_mode = false;
                self.clear_host_search_query();
                self.update_filtered_hosts();
            }
            KeyCode::Esc => {
                self.search_mode = false;
                self.clear_host_search_query();
                self.update_filtered_hosts();
            }
            KeyCode::Enter => {
                self.search_mode = false;
                self.search_query_selection = None;
            }
            KeyCode::Left => self.move_host_search_cursor_left(),
            KeyCode::Right => self.move_host_search_cursor_right(),
            KeyCode::Home => self.move_host_search_cursor_home(),
            KeyCode::End => self.move_host_search_cursor_end(),
            KeyCode::Backspace => {
                if self.backspace_host_search_text() {
                    self.update_filtered_hosts();
                }
            }
            KeyCode::Delete => {
                if self.delete_host_search_text() {
                    self.update_filtered_hosts();
                }
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.select_all_host_search_text();
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_host_search_cursor_end();
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                if self.insert_host_search_char(ch) {
                    self.update_filtered_hosts();
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_search_paste(&mut self, pasted: &str) {
        let filtered: String = pasted.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }

        let _ = delete_selection(&mut self.search_query, &mut self.search_query_cursor, &mut self.search_query_selection);
        for ch in filtered.chars() {
            let _ = self.insert_host_search_char(ch);
        }
        self.update_filtered_hosts();
    }

    // Host-browser / manager input.
    pub(crate) fn handle_manager_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match key.code {
            KeyCode::Esc => {
                if !self.focus_on_manager {
                    self.focus_manager_panel();
                }
            }
            KeyCode::BackTab => {
                if !self.tabs.is_empty() {
                    self.focus_on_manager = !self.focus_on_manager;
                    if self.focus_on_manager {
                        self.focus_manager_panel();
                    }
                }
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.focus_on_manager && !self.tabs.is_empty() {
                    self.close_current_tab();
                }
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.focus_on_manager && !self.tabs.is_empty() {
                    let tab = &self.tabs[self.selected_tab];
                    self.selected_host_to_connect = Some(ConnectRequest {
                        target: tab.host.name.clone(),
                        profile: tab.host.profile.clone(),
                        force_ssh_logging: tab.force_ssh_logging,
                    });
                    self.should_exit = true;
                }
            }
            KeyCode::Left if !self.focus_on_manager => {
                if self.selected_tab > 0 {
                    self.selected_tab -= 1;
                }
            }
            KeyCode::Right if !self.focus_on_manager => {
                if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() - 1 {
                    self.selected_tab += 1;
                }
            }
            KeyCode::Char('f') if self.focus_on_manager && key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_mode = true;
                self.search_query_cursor = char_len(&self.search_query);
                self.search_query_selection = None;
            }
            KeyCode::Char('q') if self.focus_on_manager && key.modifiers.is_empty() => {
                self.open_quick_connect_modal();
            }
            KeyCode::Char('i') if self.focus_on_manager && key.modifiers.is_empty() => {
                self.host_info_visible = !self.host_info_visible;
                self.is_dragging_host_info_divider = false;
            }
            KeyCode::Char('c') if self.focus_on_manager && key.modifiers.is_empty() && self.search_query.is_empty() => {
                let any_expanded_folder = self
                    .visible_host_rows
                    .iter()
                    .any(|row| matches!(row.kind, HostTreeRowKind::Folder(_)) && row.expanded);

                self.collapsed_folders.clear();
                if any_expanded_folder {
                    Self::collect_descendant_folder_ids(&self.host_tree_root, &mut self.collapsed_folders);
                }

                self.update_filtered_hosts();
            }
            KeyCode::Char('c') if self.focus_on_manager && key.modifiers.contains(KeyModifiers::CONTROL) && !self.search_query.is_empty() => {
                self.search_mode = false;
                self.clear_host_search_query();
                self.update_filtered_hosts();
            }
            KeyCode::Left if self.focus_on_manager && self.host_panel_visible && key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.host_panel_width = self.host_panel_width.saturating_sub(5).max(15);
            }
            KeyCode::Right if self.focus_on_manager && self.host_panel_visible && key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.host_panel_width = (self.host_panel_width + 5).min(80);
            }
            KeyCode::Left if self.focus_on_manager && key.modifiers.is_empty() => {
                if let Some(folder_id) = self.selected_folder_id() {
                    self.set_folder_expanded(folder_id, false);
                }
            }
            KeyCode::Right if self.focus_on_manager && key.modifiers.is_empty() => {
                if let Some(folder_id) = self.selected_folder_id() {
                    self.set_folder_expanded(folder_id, true);
                }
            }
            KeyCode::Up if self.focus_on_manager => {
                if self.visible_host_row_count() > 0 && self.selected_host_row > 0 {
                    self.set_selected_row(self.selected_host_row - 1);
                }
            }
            KeyCode::Down if self.focus_on_manager => {
                let row_count = self.visible_host_row_count();
                if row_count > 0 && self.selected_host_row < row_count - 1 {
                    self.set_selected_row(self.selected_host_row + 1);
                }
            }
            KeyCode::PageUp if self.focus_on_manager => {
                if self.visible_host_row_count() > 0 {
                    let page_size = 10.max(self.host_list_area.height as usize);
                    self.set_selected_row(self.selected_host_row.saturating_sub(page_size));
                }
            }
            KeyCode::PageDown if self.focus_on_manager => {
                let row_count = self.visible_host_row_count();
                if row_count > 0 {
                    let page_size = 10.max(self.host_list_area.height as usize);
                    self.set_selected_row((self.selected_host_row + page_size).min(row_count.saturating_sub(1)));
                }
            }
            KeyCode::Home if self.focus_on_manager => {
                if self.visible_host_row_count() > 0 {
                    self.set_selected_row(0);
                }
            }
            KeyCode::End if self.focus_on_manager => {
                let row_count = self.visible_host_row_count();
                if row_count > 0 {
                    self.set_selected_row(row_count.saturating_sub(1));
                }
            }
            KeyCode::Enter if self.focus_on_manager => {
                if let Some(folder_id) = self.selected_folder_id() {
                    self.toggle_folder(folder_id);
                } else {
                    self.select_host_to_connect();
                }
            }
            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../test/tui/features/host_browser/input.rs"]
mod tests;
