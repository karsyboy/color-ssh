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
#[path = "../../../test/tui/features/terminal_search/input.rs"]
mod tests;
