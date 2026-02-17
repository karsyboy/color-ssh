//! Keyboard input and terminal-search handling.

use super::{ConnectRequest, QuickConnectField, QuickConnectState, SessionManager};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io::{self, Write};

fn encode_key_event(key: KeyEvent) -> Option<Vec<u8>> {
    let bytes = match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if c.is_ascii_alphabetic() {
                    vec![(c.to_ascii_lowercase() as u8) - b'a' + 1]
                } else {
                    vec![c as u8]
                }
            } else {
                c.to_string().into_bytes()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![127],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![27],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        _ => return None,
    };

    Some(bytes)
}

impl SessionManager {
    fn clear_selection_state(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.is_selecting = false;
    }

    fn focus_manager_panel(&mut self) {
        self.focus_on_manager = true;
        if !self.host_panel_visible {
            self.host_panel_visible = true;
        }
    }

    fn close_current_tab(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        self.tabs.remove(self.selected_tab);
        if self.selected_tab >= self.tabs.len() && self.selected_tab > 0 {
            self.selected_tab -= 1;
        }

        if self.tabs.is_empty() {
            self.focus_manager_panel();
        }
    }

    fn clear_terminal_search(&mut self) {
        if let Some(search) = self.current_tab_search_mut() {
            search.active = false;
            search.query.clear();
            search.matches.clear();
            search.current = 0;
        }
    }

    fn final_right_tab_scroll_offset(&self, available_width: usize) -> usize {
        if self.tabs.is_empty() || available_width == 0 {
            return 0;
        }
        let total_width = self.total_tab_width();
        if total_width <= available_width {
            return 0;
        }

        let visible_with_left_marker = available_width.saturating_sub(1);
        let threshold = total_width.saturating_sub(visible_with_left_marker);

        let mut start = 0usize;
        let mut last_start = 0usize;
        for idx in 0..self.tabs.len() {
            if start >= threshold {
                return start;
            }
            last_start = start;
            start += self.tab_display_width(idx);
        }

        last_start
    }

    pub(super) fn normalize_tab_scroll_offset(&self, raw_offset: usize, available_width: usize) -> usize {
        if self.tabs.is_empty() || available_width == 0 {
            return 0;
        }
        let final_offset = self.final_right_tab_scroll_offset(available_width);
        let clamped = raw_offset.min(final_offset);

        let mut snapped = 0usize;
        let mut start = 0usize;
        for idx in 0..self.tabs.len() {
            if start > clamped {
                break;
            }
            snapped = start;
            start += self.tab_display_width(idx);
        }
        snapped
    }

    fn total_tab_width(&self) -> usize {
        (0..self.tabs.len()).map(|idx| self.tab_display_width(idx)).sum()
    }

    pub(super) fn prev_tab_scroll_offset(&self, raw_offset: usize, available_width: usize) -> Option<usize> {
        if self.tabs.is_empty() || available_width == 0 {
            return None;
        }

        let current = self.normalize_tab_scroll_offset(raw_offset, available_width);
        if current == 0 {
            return None;
        }

        let mut previous = 0usize;
        let mut start = 0usize;
        for idx in 0..self.tabs.len() {
            if start >= current {
                break;
            }
            previous = start;
            start += self.tab_display_width(idx);
        }

        Some(previous)
    }

    pub(super) fn next_tab_scroll_offset(&self, raw_offset: usize, available_width: usize) -> Option<usize> {
        if self.tabs.is_empty() || available_width == 0 {
            return None;
        }

        let total_width = self.total_tab_width();
        if total_width <= available_width {
            return None;
        }

        let current = self.normalize_tab_scroll_offset(raw_offset, available_width);
        let final_offset = self.final_right_tab_scroll_offset(available_width);
        if current >= final_offset {
            return None;
        }

        let mut start = 0usize;
        for idx in 0..self.tabs.len() {
            if start > current {
                return Some(start.min(final_offset));
            }
            start += self.tab_display_width(idx);
        }

        Some(final_offset)
    }

    fn open_quick_connect_modal(&mut self) {
        let profiles = self.discover_quick_connect_profiles();
        self.quick_connect = Some(QuickConnectState::new(self.quick_connect_default_ssh_logging, profiles));
    }

    fn handle_quick_connect_key(&mut self, key: KeyEvent) {
        let mut should_submit = false;
        let mut should_close = false;

        if let Some(form) = self.quick_connect.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    should_close = true;
                }
                KeyCode::Tab | KeyCode::Down => {
                    form.selected = form.selected.next();
                }
                KeyCode::BackTab | KeyCode::Up => {
                    form.selected = form.selected.prev();
                }
                KeyCode::Enter => match form.selected {
                    QuickConnectField::Profile => {
                        form.error = None;
                        form.select_next_profile();
                    }
                    QuickConnectField::Logging => {
                        form.ssh_logging = !form.ssh_logging;
                    }
                    QuickConnectField::Connect => {
                        should_submit = true;
                    }
                    _ => {
                        form.selected = form.selected.next();
                    }
                },
                KeyCode::Char(' ') => {
                    if form.selected == QuickConnectField::Logging {
                        form.ssh_logging = !form.ssh_logging;
                    }
                }
                KeyCode::Left => {
                    if form.selected == QuickConnectField::Profile {
                        form.error = None;
                        form.select_prev_profile();
                    }
                }
                KeyCode::Right => {
                    if form.selected == QuickConnectField::Profile {
                        form.error = None;
                        form.select_next_profile();
                    }
                }
                KeyCode::Backspace => {
                    form.error = None;
                    match form.selected {
                        QuickConnectField::User => {
                            form.user.pop();
                        }
                        QuickConnectField::Host => {
                            form.host.pop();
                        }
                        _ => {}
                    }
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                    form.error = None;
                    match form.selected {
                        QuickConnectField::User => form.user.push(c),
                        QuickConnectField::Host => form.host.push(c),
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if should_submit {
            self.submit_quick_connect_modal();
        } else if should_close {
            self.quick_connect = None;
        }
    }

    fn submit_quick_connect_modal(&mut self) {
        let Some(form) = self.quick_connect.as_mut() else {
            return;
        };

        let user = form.user.trim().to_string();
        let host = form.host.trim().to_string();
        let profile = form.selected_profile_for_cli();
        let force_ssh_logging = form.ssh_logging;

        if host.is_empty() {
            form.error = Some("Host is required".to_string());
            form.selected = QuickConnectField::Host;
            return;
        }

        self.open_quick_connect_host(user, host, profile, force_ssh_logging);
    }

    /// Handle keyboard input.
    pub(super) fn handle_key(&mut self, key: KeyEvent) -> io::Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_exit = true;
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

    fn handle_search_key(&mut self, key: KeyEvent) -> io::Result<()> {
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
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.update_filtered_hosts();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_terminal_search_key(&mut self, key: KeyEvent) -> io::Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.clear_terminal_search();
            }
            KeyCode::Enter | KeyCode::Down => {
                if let Some(search) = self.current_tab_search_mut()
                    && !search.matches.is_empty()
                {
                    search.current = (search.current + 1) % search.matches.len();
                    self.scroll_to_search_match();
                }
            }
            KeyCode::Up => {
                if let Some(search) = self.current_tab_search_mut()
                    && !search.matches.is_empty()
                {
                    if search.current == 0 {
                        search.current = search.matches.len() - 1;
                    } else {
                        search.current -= 1;
                    }
                    self.scroll_to_search_match();
                }
            }
            KeyCode::Backspace => {
                if let Some(search) = self.current_tab_search_mut() {
                    search.query.pop();
                    self.update_terminal_search();
                }
            }
            KeyCode::Char(c) => {
                if let Some(search) = self.current_tab_search_mut() {
                    search.query.push(c);
                    self.update_terminal_search();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_tab_key(&mut self, key: KeyEvent) -> io::Result<()> {
        if self.current_tab_search().map(|s| s.active).unwrap_or(false) {
            return self.handle_terminal_search_key(key);
        }

        match key.code {
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.host_panel_visible = !self.host_panel_visible;
            }
            KeyCode::Char('c') if key.modifiers == KeyModifiers::ALT => {
                if self.selection_start.is_some() && self.selection_end.is_some() {
                    self.copy_selection_to_clipboard();
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
                    if let Some(search) = self.current_tab_search_mut() {
                        search.active = true;
                    }
                } else {
                    self.send_key_to_pty(key)?;
                }
            }
            KeyCode::PageUp if key.modifiers.contains(KeyModifiers::SHIFT) => {
                let max_scrollback = self.max_scrollback_for_tab(self.selected_tab);
                let tab = &mut self.tabs[self.selected_tab];
                tab.scroll_offset = tab.scroll_offset.saturating_add(10).min(max_scrollback);
            }
            KeyCode::PageDown if key.modifiers.contains(KeyModifiers::SHIFT) => {
                let tab = &mut self.tabs[self.selected_tab];
                tab.scroll_offset = tab.scroll_offset.saturating_sub(10);
            }
            KeyCode::Enter => {
                let tab = &self.tabs[self.selected_tab];
                let is_exited = tab.session.as_ref().and_then(|session| session.exited.lock().ok().map(|v| *v)).unwrap_or(true);

                if is_exited {
                    self.reconnect_session();
                } else {
                    self.tabs[self.selected_tab].scroll_offset = 0;
                    self.clear_selection_state();
                    self.send_key_to_pty(key)?;
                }
            }
            _ => {
                self.tabs[self.selected_tab].scroll_offset = 0;
                self.clear_selection_state();
                self.send_key_to_pty(key)?;
            }
        }

        Ok(())
    }

    fn handle_manager_key(&mut self, key: KeyEvent) -> io::Result<()> {
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
                    .any(|row| matches!(row.kind, super::HostTreeRowKind::Folder(_)) && row.expanded);

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

    pub(super) fn send_key_to_pty(&mut self, key: KeyEvent) -> io::Result<()> {
        if self.selected_tab >= self.tabs.len() {
            return Ok(());
        }

        let Some(bytes) = encode_key_event(key) else {
            return Ok(());
        };

        let tab = &mut self.tabs[self.selected_tab];
        if let Some(session) = &mut tab.session
            && let Ok(mut writer) = session.writer.lock()
        {
            let _ = writer.write_all(&bytes);
        }

        Ok(())
    }

    pub(super) fn tab_display_width(&self, idx: usize) -> usize {
        if idx >= self.tabs.len() {
            return 0;
        }
        self.tabs[idx].title.len() + 3
    }

    pub(super) fn ensure_tab_visible(&mut self) {
        if self.tabs.is_empty() {
            self.tab_scroll_offset = 0;
            return;
        }

        let tab_bar_width = self.tab_bar_area.width as usize;
        if tab_bar_width == 0 {
            return;
        }

        let mut start_pos: usize = 0;
        for i in 0..self.selected_tab {
            start_pos += self.tab_display_width(i);
        }
        let end_pos = start_pos + self.tab_display_width(self.selected_tab);

        if start_pos < self.tab_scroll_offset || end_pos > self.tab_scroll_offset + tab_bar_width {
            self.tab_scroll_offset = start_pos;
        }
    }

    fn update_terminal_search(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let selected_tab = self.selected_tab;
        let query_lower = {
            let search = &mut self.tabs[selected_tab].terminal_search;
            search.matches.clear();
            search.current = 0;
            if search.query.is_empty() {
                return;
            }
            search.query.to_lowercase()
        };

        let scroll_offset = self.tabs[selected_tab].scroll_offset;
        let parser_arc = match self.tabs[selected_tab].session.as_ref() {
            Some(session) => session.parser.clone(),
            None => return,
        };

        let mut matches = Vec::new();
        if let Ok(mut parser) = parser_arc.lock() {
            parser.set_scrollback(usize::MAX);
            let max_scrollback = parser.screen().scrollback();

            for scrollback_pos in (0..=max_scrollback).rev() {
                parser.set_scrollback(scrollback_pos);
                let screen = parser.screen();
                let (rows, cols) = screen.size();

                let search_rows: Vec<u16> = if scrollback_pos == 0 { (0..rows).collect() } else { vec![0] };

                for &row in &search_rows {
                    let mut row_text = String::new();
                    let mut col_to_pos = Vec::new();

                    for col in 0..cols {
                        col_to_pos.push(row_text.len());
                        if let Some(cell) = screen.cell(row, col) {
                            if cell.has_contents() {
                                row_text.push_str(&cell.contents());
                            } else {
                                row_text.push(' ');
                            }
                        } else {
                            row_text.push(' ');
                        }
                    }

                    let row_text_lower = row_text.to_lowercase();
                    let mut search_start = 0;
                    while let Some(pos) = row_text_lower[search_start..].find(&query_lower) {
                        let match_pos = search_start + pos;

                        let mut match_col = 0usize;
                        for (col_idx, &string_pos) in col_to_pos.iter().enumerate() {
                            if string_pos == match_pos {
                                match_col = col_idx;
                                break;
                            } else if string_pos > match_pos {
                                break;
                            } else {
                                match_col = col_idx;
                            }
                        }

                        let abs_row = row as i64 - scrollback_pos as i64;
                        matches.push((abs_row, match_col as u16, query_lower.chars().count()));

                        search_start = match_pos + 1;
                    }
                }
            }

            parser.set_scrollback(scroll_offset);
        }

        if let Some(search) = self.tabs.get_mut(selected_tab).map(|tab| &mut tab.terminal_search) {
            search.matches = matches;
            search.current = 0;
        }

        if self.tabs.get(selected_tab).map(|tab| !tab.terminal_search.matches.is_empty()).unwrap_or(false) {
            self.scroll_to_search_match();
        }
    }

    fn scroll_to_search_match(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let selected_tab = self.selected_tab;
        let (abs_row, parser_arc) = {
            let tab = &self.tabs[selected_tab];
            if tab.terminal_search.matches.is_empty() {
                return;
            }
            let Some(session) = &tab.session else {
                return;
            };
            (tab.terminal_search.matches[tab.terminal_search.current].0, session.parser.clone())
        };

        let tab = &mut self.tabs[selected_tab];
        let tab_height = self.tab_content_area.height as i64;

        if let Ok(mut parser) = parser_arc.lock() {
            let max_scrollback = {
                parser.set_scrollback(usize::MAX);
                let max = parser.screen().scrollback();
                parser.set_scrollback(0);
                max
            };

            let target_screen_row = tab_height / 3;
            let needed_scroll = target_screen_row - abs_row;

            if needed_scroll < 0 {
                tab.scroll_offset = 0;
            } else {
                tab.scroll_offset = (needed_scroll as usize).min(max_scrollback);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_key_event_ctrl_char() {
        let key = KeyEvent::new(KeyCode::Char('C'), KeyModifiers::CONTROL);
        assert_eq!(encode_key_event(key), Some(vec![3]));
    }

    #[test]
    fn encode_key_event_arrow() {
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(encode_key_event(key), Some(b"\x1b[A".to_vec()));
    }
}
