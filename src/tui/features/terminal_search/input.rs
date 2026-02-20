//! Terminal-search keyboard handling.

use crate::tui::SessionManager;
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
    fn terminal_search_query_mut(&mut self) -> Option<(&mut String, &mut usize, &mut Option<(usize, usize)>)> {
        let search = self.current_tab_search_mut()?;
        Some((&mut search.query, &mut search.query_cursor, &mut search.query_selection))
    }

    fn move_terminal_search_cursor_left(&mut self) {
        if let Some((query, cursor, selection)) = self.terminal_search_query_mut() {
            clamp_cursor(query, cursor);
            let active_selection = normalized_selection(query, *selection);
            *selection = None;
            if let Some((start, _)) = active_selection {
                *cursor = start;
            } else if *cursor > 0 {
                *cursor -= 1;
            }
        }
    }

    fn move_terminal_search_cursor_right(&mut self) {
        if let Some((query, cursor, selection)) = self.terminal_search_query_mut() {
            clamp_cursor(query, cursor);
            let len = char_len(query);
            let active_selection = normalized_selection(query, *selection);
            *selection = None;
            if let Some((_, end)) = active_selection {
                *cursor = end;
            } else if *cursor < len {
                *cursor += 1;
            }
        }
    }

    fn move_terminal_search_cursor_home(&mut self) {
        if let Some((_, cursor, selection)) = self.terminal_search_query_mut() {
            *cursor = 0;
            *selection = None;
        }
    }

    fn move_terminal_search_cursor_end(&mut self) {
        if let Some((query, cursor, selection)) = self.terminal_search_query_mut() {
            *cursor = char_len(query);
            *selection = None;
        }
    }

    fn select_all_terminal_search_text(&mut self) {
        if let Some((query, cursor, selection)) = self.terminal_search_query_mut() {
            let len = char_len(query);
            if len == 0 {
                *selection = None;
                *cursor = 0;
            } else {
                *selection = Some((0, len));
                *cursor = len;
            }
        }
    }

    fn insert_terminal_search_char(&mut self, ch: char) -> bool {
        let Some((query, cursor, selection)) = self.terminal_search_query_mut() else {
            return false;
        };

        let _ = delete_selection(query, cursor, selection);
        clamp_cursor(query, cursor);
        let insert_at = byte_index_for_char(query, *cursor);
        query.insert(insert_at, ch);
        *cursor += 1;
        *selection = None;
        true
    }

    fn backspace_terminal_search_text(&mut self) -> bool {
        let Some((query, cursor, selection)) = self.terminal_search_query_mut() else {
            return false;
        };

        if delete_selection(query, cursor, selection) {
            return true;
        }

        clamp_cursor(query, cursor);
        if *cursor == 0 {
            *selection = None;
            return false;
        }

        let end = byte_index_for_char(query, *cursor);
        let start = byte_index_for_char(query, *cursor - 1);
        query.replace_range(start..end, "");
        *cursor -= 1;
        *selection = None;
        true
    }

    fn delete_terminal_search_text(&mut self) -> bool {
        let Some((query, cursor, selection)) = self.terminal_search_query_mut() else {
            return false;
        };

        if delete_selection(query, cursor, selection) {
            return true;
        }

        clamp_cursor(query, cursor);
        let len = char_len(query);
        if *cursor >= len {
            *selection = None;
            return false;
        }

        let start = byte_index_for_char(query, *cursor);
        let end = byte_index_for_char(query, *cursor + 1);
        query.replace_range(start..end, "");
        *selection = None;
        true
    }

    // Search state lifecycle.
    pub(crate) fn clear_terminal_search(&mut self) {
        if let Some(search) = self.current_tab_search_mut() {
            search.active = false;
            search.query.clear();
            search.query_cursor = 0;
            search.query_selection = None;
            search.matches.clear();
            search.current = 0;
        }
    }

    // Keyboard handling for search mode.
    pub(crate) fn handle_terminal_search_key(&mut self, key: KeyEvent) -> io::Result<()> {
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
            KeyCode::Left => self.move_terminal_search_cursor_left(),
            KeyCode::Right => self.move_terminal_search_cursor_right(),
            KeyCode::Home => self.move_terminal_search_cursor_home(),
            KeyCode::End => self.move_terminal_search_cursor_end(),
            KeyCode::Backspace => {
                if self.backspace_terminal_search_text() {
                    self.update_terminal_search();
                }
            }
            KeyCode::Delete => {
                if self.delete_terminal_search_text() {
                    self.update_terminal_search();
                }
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.select_all_terminal_search_text();
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.move_terminal_search_cursor_end();
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                if self.insert_terminal_search_char(ch) {
                    self.update_terminal_search();
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn handle_terminal_search_paste(&mut self, pasted: &str) {
        let filtered: String = pasted.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }

        if let Some((query, cursor, selection)) = self.terminal_search_query_mut() {
            let _ = delete_selection(query, cursor, selection);
            for ch in filtered.chars() {
                clamp_cursor(query, cursor);
                let insert_at = byte_index_for_char(query, *cursor);
                query.insert(insert_at, ch);
                *cursor += 1;
            }
            *selection = None;
            self.update_terminal_search();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SessionManager;
    use crate::ssh_config::SshHost;
    use crate::tui::{HostTab, TerminalSearchState};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn app_with_active_search() -> SessionManager {
        let mut app = SessionManager::new_for_tests();
        app.tabs.push(HostTab {
            host: SshHost::new("test-host".to_string()),
            title: "test-host".to_string(),
            session: None,
            scroll_offset: 0,
            terminal_search: TerminalSearchState {
                active: true,
                query: String::new(),
                query_cursor: 0,
                query_selection: None,
                matches: Vec::new(),
                current: 0,
            },
            force_ssh_logging: false,
            last_pty_size: None,
        });
        app.selected_tab = 0;
        app.focus_on_manager = false;
        app
    }

    #[test]
    fn clears_terminal_search_state() {
        let mut app = app_with_active_search();
        if let Some(search) = app.current_tab_search_mut() {
            search.query = "err".to_string();
            search.matches = vec![(0, 1, 3)];
            search.current = 2;
        }

        app.clear_terminal_search();

        let search = app.current_tab_search().expect("search state");
        assert!(!search.active);
        assert!(search.query.is_empty());
        assert!(search.matches.is_empty());
        assert_eq!(search.current, 0);
    }

    #[test]
    fn wraps_terminal_search_navigation_next_and_prev() {
        let mut app = app_with_active_search();
        if let Some(search) = app.current_tab_search_mut() {
            search.matches = vec![(0, 0, 1), (1, 0, 1)];
            search.current = 1;
        }

        app.handle_terminal_search_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            .expect("down key");
        assert_eq!(app.current_tab_search().map(|search| search.current), Some(0));

        app.handle_terminal_search_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)).expect("up key");
        assert_eq!(app.current_tab_search().map(|search| search.current), Some(1));
    }

    #[test]
    fn edits_terminal_search_query_with_char_and_backspace() {
        let mut app = app_with_active_search();

        app.handle_terminal_search_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
            .expect("insert char");
        assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some("a"));

        app.handle_terminal_search_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
            .expect("backspace");
        assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some(""));
    }

    #[test]
    fn edits_terminal_search_query_in_the_middle() {
        let mut app = app_with_active_search();
        if let Some(search) = app.current_tab_search_mut() {
            search.query = "admn".to_string();
            search.query_cursor = 3;
        }

        app.handle_terminal_search_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE))
            .expect("insert char");
        assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some("admin"));

        app.handle_terminal_search_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)).expect("left");
        app.handle_terminal_search_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE))
            .expect("delete");
        assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some("admn"));
    }

    #[test]
    fn paste_appends_terminal_search_query() {
        let mut app = app_with_active_search();
        app.handle_terminal_search_paste("err\nwarn");

        // Control characters are filtered from pasted text.
        assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some("errwarn"));
    }
}
