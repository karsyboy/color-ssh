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
        let (query_lower, query_char_count) = {
            let search = &mut self.tabs[selected_tab].terminal_search;
            search.matches.clear();
            search.current = 0;
            if search.query.is_empty() {
                return;
            }
            let query_lower = search.query.to_lowercase();
            let query_char_count = query_lower.chars().count();
            (query_lower, query_char_count)
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
            let mut row_text = String::new();
            let mut col_to_pos = Vec::new();

            for scrollback_pos in (0..=max_scrollback).rev() {
                parser.set_scrollback(scrollback_pos);
                let screen = parser.screen();
                let (rows, cols) = screen.size();

                let mut scan_row = |row: u16, collected: &mut Vec<(i64, u16, usize)>| {
                    row_text.clear();
                    col_to_pos.clear();

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
                            }
                            if string_pos > match_pos {
                                break;
                            }
                            match_col = col_idx;
                        }

                        let abs_row = row as i64 - scrollback_pos as i64;
                        collected.push((abs_row, match_col as u16, query_char_count));
                        search_start = match_pos + 1;
                    }
                };

                if scrollback_pos == 0 {
                    for row in 0..rows {
                        scan_row(row, &mut matches);
                    }
                } else {
                    scan_row(0, &mut matches);
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
