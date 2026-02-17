//! Host browser keyboard handling.

use crate::tui::{ConnectRequest, HostTreeRowKind, SessionManager};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io;

impl SessionManager {
    pub(crate) fn handle_search_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_mode = false;
                self.search_query.clear();
                self.update_filtered_hosts();
            }
            KeyCode::Esc => {
                self.search_mode = false;
                self.search_query.clear();
                self.update_filtered_hosts();
            }
            KeyCode::Enter => {
                self.search_mode = false;
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.update_filtered_hosts();
            }
            KeyCode::Char(ch) => {
                self.search_query.push(ch);
                self.update_filtered_hosts();
            }
            _ => {}
        }
        Ok(())
    }

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
                self.search_query.clear();
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
