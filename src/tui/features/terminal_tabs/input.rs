//! Keyboard input handling and PTY write helpers.

use crate::log_error;
use crate::tui::SessionManager;
use crate::tui::features::terminal_session::pty::encode_key_event_bytes;
use crate::tui::ui::theme::display_width;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::io::{self, Write};

fn tab_title_display_width(title: &str) -> usize {
    display_width(title)
}

impl SessionManager {
    pub(crate) fn clear_selection_state(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
        self.is_selecting = false;
    }

    pub(crate) fn focus_manager_panel(&mut self) {
        self.focus_on_manager = true;
        if !self.host_panel_visible {
            self.host_panel_visible = true;
        }
    }

    pub(crate) fn close_current_tab(&mut self) {
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

    /// Handle keyboard input.
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> io::Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        let terminal_view_active = !self.focus_on_manager && !self.tabs.is_empty() && self.selected_tab < self.tabs.len();
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            if !terminal_view_active {
                self.should_exit = true;
                return Ok(());
            }
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

    pub(crate) fn handle_tab_key(&mut self, key: KeyEvent) -> io::Result<()> {
        if self.current_tab_search().map(|search_state| search_state.active).unwrap_or(false) {
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
                let is_exited = tab
                    .session
                    .as_ref()
                    .and_then(|session| session.exited.lock().ok().map(|exited| *exited))
                    .unwrap_or(true);

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

    pub(crate) fn write_bytes_to_active_pty(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.selected_tab >= self.tabs.len() {
            return Ok(());
        }

        let tab = &mut self.tabs[self.selected_tab];
        let Some(session) = &mut tab.session else {
            return Ok(());
        };

        let mut writer = match session.writer.lock() {
            Ok(writer) => writer,
            Err(lock_err) => {
                log_error!("Failed to lock PTY writer: {}", lock_err);
                return Ok(());
            }
        };

        if let Err(err) = writer.write_all(bytes) {
            log_error!("Failed to write to PTY: {}", err);
            if let Ok(mut exited) = session.exited.lock() {
                *exited = true;
            }
            return Ok(());
        }

        Ok(())
    }

    pub(crate) fn send_key_to_pty(&mut self, key: KeyEvent) -> io::Result<()> {
        let Some(bytes) = encode_key_event_bytes(key) else {
            return Ok(());
        };

        self.write_bytes_to_active_pty(&bytes)
    }

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
        tab_title_display_width(&self.tabs[idx].title)
    }

    pub(crate) fn ensure_tab_visible(&mut self) {
        if self.tabs.is_empty() {
            self.tab_scroll_offset = 0;
            return;
        }

        let tab_bar_width = self.tab_bar_area.width as usize;
        if tab_bar_width == 0 {
            return;
        }

        let mut start_pos: usize = 0;
        for tab_index in 0..self.selected_tab {
            start_pos += self.tab_display_width(tab_index);
        }
        let end_pos = start_pos + self.tab_display_width(self.selected_tab);

        if start_pos < self.tab_scroll_offset || end_pos > self.tab_scroll_offset + tab_bar_width {
            self.tab_scroll_offset = start_pos;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_key_event_ctrl_char() {
        let key = KeyEvent::new(KeyCode::Char('C'), KeyModifiers::CONTROL);
        assert_eq!(encode_key_event_bytes(key), Some(vec![3]));
    }

    #[test]
    fn encode_key_event_ctrl_bracket_variants() {
        let open = KeyEvent::new(KeyCode::Char('['), KeyModifiers::CONTROL);
        let backslash = KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL);
        let close = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL);
        let at = KeyEvent::new(KeyCode::Char('@'), KeyModifiers::CONTROL);

        assert_eq!(encode_key_event_bytes(open), Some(vec![27]));
        assert_eq!(encode_key_event_bytes(backslash), Some(vec![28]));
        assert_eq!(encode_key_event_bytes(close), Some(vec![29]));
        assert_eq!(encode_key_event_bytes(at), Some(vec![0]));
    }

    #[test]
    fn encode_key_event_arrow() {
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(encode_key_event_bytes(key), Some(b"\x1b[A".to_vec()));
    }

    #[test]
    fn encode_key_event_alt_char_prefixes_escape() {
        let key = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT);
        assert_eq!(encode_key_event_bytes(key), Some(vec![0x1b, b'f']));
    }

    #[test]
    fn encode_key_event_alt_arrow_prefixes_escape() {
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        assert_eq!(encode_key_event_bytes(key), Some(b"\x1b\x1b[A".to_vec()));
    }

    #[test]
    fn tab_title_display_width_uses_unicode_display_width() {
        assert_eq!(tab_title_display_width("a界"), 3);
    }

    #[test]
    fn handle_key_ctrl_q_sets_should_exit() {
        let mut app = SessionManager::new().expect("app should initialize");
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        app.handle_key(key).expect("handle_key should succeed");
        assert!(app.should_exit);
    }

    #[test]
    fn handle_key_ctrl_q_does_not_exit_in_terminal_view() {
        let mut app = SessionManager::new().expect("app should initialize");
        app.tabs.push(crate::tui::HostTab {
            host: crate::ssh_config::SshHost::new("test-host".to_string()),
            title: "test-host".to_string(),
            session: None,
            scroll_offset: 0,
            terminal_search: crate::tui::TerminalSearchState::default(),
            terminal_search_cache: crate::tui::TerminalSearchCache::default(),
            force_ssh_logging: false,
            last_pty_size: None,
        });
        app.selected_tab = 0;
        app.focus_on_manager = false;

        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        app.handle_key(key).expect("handle_key should succeed");
        assert!(!app.should_exit);
    }
}
