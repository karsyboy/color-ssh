//! Terminal-search indexing and viewport sync.

use crate::tui::SessionManager;
use crate::{debug_enabled, log_debug};
use std::time::Instant;

impl SessionManager {
    pub(crate) fn update_terminal_search(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }
        let search_started_at = Instant::now();

        let selected_tab = self.selected_tab;
        let query = {
            let search = &mut self.tabs[selected_tab].terminal_search;
            search.matches.clear();
            search.current = 0;
            if search.query.is_empty() {
                return;
            }
            search.query.clone()
        };

        let parser_arc = match self.tabs[selected_tab].session.as_ref() {
            Some(session) => session.parser.clone(),
            None => return,
        };

        let matches = if let Ok(parser) = parser_arc.lock() {
            parser.search_literal_matches(&query)
        } else {
            Vec::new()
        };

        if let Some(search) = self.tabs.get_mut(selected_tab).map(|tab| &mut tab.terminal_search) {
            search.matches = matches;
            search.current = 0;
        }

        if self.tabs.get(selected_tab).map(|tab| !tab.terminal_search.matches.is_empty()).unwrap_or(false) {
            self.scroll_to_search_match();
        }

        if debug_enabled!() {
            let elapsed = search_started_at.elapsed();
            let match_count = self.tabs.get(selected_tab).map(|tab| tab.terminal_search.matches.len()).unwrap_or(0);
            log_debug!("Terminal search updated in {:?} (matches: {})", elapsed, match_count);
        }
    }

    pub(crate) fn scroll_to_search_match(&mut self) {
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

        if let Ok(parser) = parser_arc.lock() {
            let max_scrollback = parser.max_scrollback();

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
    use crate::tui::terminal_emulator::Parser;

    #[test]
    fn search_literal_matches_finds_multiple_matches_on_same_row() {
        let mut parser = Parser::new(2, 20, 50);
        parser.process(b"alpha alpha\\r\\n");
        let matches = parser.search_literal_matches("alpha");
        assert_eq!(matches, vec![(0, 0, 5), (0, 6, 5)]);
    }

    #[test]
    fn search_literal_matches_is_case_insensitive() {
        let mut parser = Parser::new(2, 20, 50);
        parser.process(b"Status STATUS status\\r\\n");
        let matches = parser.search_literal_matches("status");
        assert_eq!(matches.len(), 3);
    }
}
