//! Terminal-search indexing and viewport sync.

use crate::tui::{SessionManager, TerminalSearchRowSnapshot};
use crate::{debug_enabled, log_debug};
use std::sync::atomic::Ordering as AtomicOrdering;
use std::time::Instant;

fn find_matches_in_cached_rows(rows: &[TerminalSearchRowSnapshot], query_lower: &str, query_char_count: usize) -> Vec<(i64, u16, usize)> {
    let mut matches = Vec::new();
    for row in rows {
        let mut search_start = 0;
        while let Some(pos) = row.row_text_lower[search_start..].find(query_lower) {
            let match_pos = search_start + pos;
            let match_col = SessionManager::match_col_for_start(&row.col_start_byte_offsets, match_pos);
            matches.push((row.abs_row, match_col as u16, query_char_count));
            search_start = match_pos.saturating_add(1);
        }
    }
    matches
}

impl SessionManager {
    fn build_terminal_search_cache_rows(parser: &mut vt100::Parser, restore_scrollback: usize) -> Vec<TerminalSearchRowSnapshot> {
        parser.set_scrollback(usize::MAX);
        let max_scrollback = parser.screen().scrollback();
        let mut rows_out = Vec::new();

        for scrollback_pos in (0..=max_scrollback).rev() {
            parser.set_scrollback(scrollback_pos);
            let screen = parser.screen();
            let (rows, cols) = screen.size();

            let mut collect_row = |row: u16| {
                let mut row_text_lower = String::new();
                let mut col_start_byte_offsets = Vec::with_capacity(cols as usize);
                for col in 0..cols {
                    col_start_byte_offsets.push(row_text_lower.len());
                    if let Some(cell) = screen.cell(row, col) {
                        if cell.has_contents() {
                            row_text_lower.push_str(&cell.contents().to_lowercase());
                        } else {
                            row_text_lower.push(' ');
                        }
                    } else {
                        row_text_lower.push(' ');
                    }
                }
                rows_out.push(TerminalSearchRowSnapshot {
                    abs_row: row as i64 - scrollback_pos as i64,
                    row_text_lower,
                    col_start_byte_offsets,
                });
            };

            if scrollback_pos == 0 {
                for row in 0..rows {
                    collect_row(row);
                }
            } else {
                collect_row(0);
            }
        }

        parser.set_scrollback(restore_scrollback);
        rows_out
    }

    fn match_col_for_start(col_start_byte_offsets: &[usize], start_pos: usize) -> usize {
        match col_start_byte_offsets.binary_search(&start_pos) {
            Ok(col) => col,
            Err(0) => 0,
            Err(insert) => insert.saturating_sub(1),
        }
    }

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

        let (scroll_offset, parser_arc, render_epoch) = match self.tabs[selected_tab].session.as_ref() {
            Some(session) => (
                self.tabs[selected_tab].scroll_offset,
                session.parser.clone(),
                session.render_epoch.load(AtomicOrdering::Relaxed),
            ),
            None => return,
        };

        let cache_stale =
            self.tabs[selected_tab].terminal_search_cache.rows.is_empty() || self.tabs[selected_tab].terminal_search_cache.render_epoch != render_epoch;

        if cache_stale {
            if let Ok(mut parser) = parser_arc.lock() {
                let rows = Self::build_terminal_search_cache_rows(&mut parser, scroll_offset);
                if let Some(tab) = self.tabs.get_mut(selected_tab) {
                    tab.terminal_search_cache.rows = rows;
                    tab.terminal_search_cache.render_epoch = render_epoch;
                }
            } else if let Some(tab) = self.tabs.get_mut(selected_tab) {
                tab.terminal_search_cache.rows.clear();
                tab.terminal_search_cache.render_epoch = render_epoch;
            }
        }

        let matches = find_matches_in_cached_rows(&self.tabs[selected_tab].terminal_search_cache.rows, &query_lower, query_char_count);

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

#[cfg(test)]
mod tests {
    use super::{SessionManager, find_matches_in_cached_rows};
    use crate::tui::TerminalSearchRowSnapshot;
    use vt100::Parser;

    #[test]
    fn restores_scrollback_after_building_search_cache_rows() {
        let mut parser = Parser::new(2, 5, 50);
        parser.process(b"11111\r\n22222\r\n33333\r\n");
        parser.set_scrollback(1);
        let before = parser.screen().cell(0, 0).map(|cell| cell.contents()).unwrap_or_default();

        let rows = SessionManager::build_terminal_search_cache_rows(&mut parser, 1);
        let after = parser.screen().cell(0, 0).map(|cell| cell.contents()).unwrap_or_default();

        assert!(!rows.is_empty());
        assert_eq!(before, after);
    }

    #[test]
    fn maps_start_byte_offset_to_column_with_binary_search() {
        let offsets = vec![0, 1, 2, 3, 4];
        assert_eq!(SessionManager::match_col_for_start(&offsets, 0), 0);
        assert_eq!(SessionManager::match_col_for_start(&offsets, 3), 3);
        assert_eq!(SessionManager::match_col_for_start(&offsets, 10), 4);
    }

    #[test]
    fn finds_multiple_matches_per_row_from_cached_snapshots() {
        let rows = vec![TerminalSearchRowSnapshot {
            abs_row: 3,
            row_text_lower: "alpha alpha".to_string(),
            col_start_byte_offsets: (0..11).collect(),
        }];

        let matches = find_matches_in_cached_rows(&rows, "alpha", 5);
        assert_eq!(matches, vec![(3, 0, 5), (3, 6, 5)]);
    }
}
