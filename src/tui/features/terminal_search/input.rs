//! Terminal-search keyboard handling.

use crate::tui::SessionManager;
use crossterm::event::{KeyCode, KeyEvent};
use std::io;

impl SessionManager {
    pub(crate) fn clear_terminal_search(&mut self) {
        if let Some(search) = self.current_tab_search_mut() {
            search.active = false;
            search.query.clear();
            search.matches.clear();
            search.current = 0;
        }
    }

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
            KeyCode::Backspace => {
                if let Some(search) = self.current_tab_search_mut() {
                    search.query.pop();
                    self.update_terminal_search();
                }
            }
            KeyCode::Char(ch) => {
                if let Some(search) = self.current_tab_search_mut() {
                    search.query.push(ch);
                    self.update_terminal_search();
                }
            }
            _ => {}
        }
        Ok(())
    }
}
