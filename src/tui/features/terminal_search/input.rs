//! Terminal-search keyboard handling.

use crate::tui::AppState;
use crate::tui::text_edit;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io;

type SearchSelection = Option<(usize, usize)>;
type TerminalSearchQueryMut<'a> = (&'a mut String, &'a mut usize, &'a mut SearchSelection);

impl AppState {
    fn terminal_search_query_mut(&mut self) -> Option<TerminalSearchQueryMut<'_>> {
        let search = self.current_tab_search_mut()?;
        Some((&mut search.query, &mut search.query_cursor, &mut search.query_selection))
    }

    fn move_terminal_search_cursor_left(&mut self) {
        if let Some((query, cursor, selection)) = self.terminal_search_query_mut() {
            text_edit::move_cursor_left(query, cursor, selection);
        }
    }

    fn move_terminal_search_cursor_right(&mut self) {
        if let Some((query, cursor, selection)) = self.terminal_search_query_mut() {
            text_edit::move_cursor_right(query, cursor, selection);
        }
    }

    fn move_terminal_search_cursor_home(&mut self) {
        if let Some((_, cursor, selection)) = self.terminal_search_query_mut() {
            text_edit::move_cursor_home(cursor, selection);
        }
    }

    fn move_terminal_search_cursor_end(&mut self) {
        if let Some((query, cursor, selection)) = self.terminal_search_query_mut() {
            text_edit::move_cursor_end(query, cursor, selection);
        }
    }

    fn select_all_terminal_search_text(&mut self) {
        if let Some((query, cursor, selection)) = self.terminal_search_query_mut() {
            text_edit::select_all(query, cursor, selection);
        }
    }

    fn insert_terminal_search_char(&mut self, ch: char) -> bool {
        let Some((query, cursor, selection)) = self.terminal_search_query_mut() else {
            return false;
        };

        text_edit::insert_char(query, cursor, selection, ch);
        true
    }

    fn backspace_terminal_search_text(&mut self) -> bool {
        let Some((query, cursor, selection)) = self.terminal_search_query_mut() else {
            return false;
        };

        let len_before = text_edit::char_len(query);
        text_edit::backspace(query, cursor, selection);
        text_edit::char_len(query) != len_before
    }

    fn delete_terminal_search_text(&mut self) -> bool {
        let Some((query, cursor, selection)) = self.terminal_search_query_mut() else {
            return false;
        };

        let len_before = text_edit::char_len(query);
        text_edit::delete_char(query, cursor, selection);
        text_edit::char_len(query) != len_before
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
            search.highlight_row_ranges.clear();
            search.current_highlight_range = None;
            search.last_search_query.clear();
            search.last_scanned_render_epoch = 0;
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
                    self.refresh_current_terminal_search_range();
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
                    self.refresh_current_terminal_search_range();
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
            let _ = text_edit::delete_selection(query, cursor, selection);
            for ch in filtered.chars() {
                text_edit::insert_char(query, cursor, selection, ch);
            }
            self.update_terminal_search();
        }
    }
}
