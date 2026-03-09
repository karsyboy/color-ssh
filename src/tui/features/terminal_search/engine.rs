//! Terminal-search indexing and viewport sync.

use crate::tui::AppState;
use crate::tui::features::terminal_search::render_highlight::build_search_row_ranges_from_matches;
use crate::{debug_enabled, log_debug};
use std::time::Instant;

impl AppState {
    fn selected_tab_render_epoch(&self, tab_idx: usize) -> u64 {
        self.tabs
            .get(tab_idx)
            .and_then(|tab| tab.session.as_ref())
            .map_or(0, |session| session.render_epoch())
    }

    pub(crate) fn refresh_current_terminal_search_range(&mut self) {
        if let Some(search) = self.current_tab_search_mut() {
            search.current_highlight_range = search.matches.get(search.current).map(|(row, col, len)| {
                let end_col = col.saturating_add(*len as u16);
                (*row, *col, end_col)
            });
        }
    }

    fn rebuild_terminal_search_highlight_cache(&mut self, tab_idx: usize) {
        if let Some(search) = self.tabs.get_mut(tab_idx).map(|tab| &mut tab.terminal_search) {
            let (row_ranges, current_range) = build_search_row_ranges_from_matches(&search.matches, search.current);
            search.highlight_row_ranges = row_ranges;
            search.current_highlight_range = current_range;
        }
    }

    fn clear_terminal_search_matches(&mut self, tab_idx: usize, scanned_epoch: u64) {
        if let Some(search) = self.tabs.get_mut(tab_idx).map(|tab| &mut tab.terminal_search) {
            search.matches.clear();
            search.current = 0;
            search.highlight_row_ranges.clear();
            search.current_highlight_range = None;
            search.last_scanned_render_epoch = scanned_epoch;
        }
    }

    pub(crate) fn refresh_active_terminal_search_if_needed(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        if self.tabs[self.selected_tab].terminal_search.active {
            self.update_terminal_search();
        }
    }

    // Rebuild match list from current query.
    pub(crate) fn update_terminal_search(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }
        let search_started_at = Instant::now();

        let selected_tab = self.selected_tab;
        let selected_tab_epoch = self.selected_tab_render_epoch(selected_tab);
        enum SearchDecision {
            Skip,
            Clear,
            Recompute { query: String, query_changed: bool },
        }
        let decision = {
            let search = &mut self.tabs[selected_tab].terminal_search;
            if !search.active {
                SearchDecision::Skip
            } else if search.query.is_empty() {
                search.last_search_query.clear();
                SearchDecision::Clear
            } else {
                let query_changed = search.query != search.last_search_query;
                let epoch_changed = search.last_scanned_render_epoch != selected_tab_epoch;
                if !query_changed && !epoch_changed {
                    SearchDecision::Skip
                } else {
                    SearchDecision::Recompute {
                        query: search.query.clone(),
                        query_changed,
                    }
                }
            }
        };

        let (query, query_changed) = match decision {
            SearchDecision::Skip => return,
            SearchDecision::Clear => {
                self.clear_terminal_search_matches(selected_tab, selected_tab_epoch);
                return;
            }
            SearchDecision::Recompute { query, query_changed } => (query, query_changed),
        };

        let engine_handle = match self.tabs[selected_tab].session.as_ref() {
            Some(session) => session.engine_handle(),
            None => {
                if let Some(search) = self.tabs.get_mut(selected_tab).map(|tab| &mut tab.terminal_search) {
                    search.last_search_query = query;
                }
                self.clear_terminal_search_matches(selected_tab, selected_tab_epoch);
                return;
            }
        };

        let matches = if let Ok(engine) = engine_handle.lock() {
            engine.search_literal_matches(&query)
        } else {
            Vec::new()
        };

        if let Some(search) = self.tabs.get_mut(selected_tab).map(|tab| &mut tab.terminal_search) {
            let previous_current = search.current;
            search.matches = matches;
            search.current = if search.matches.is_empty() || query_changed {
                0
            } else {
                previous_current.min(search.matches.len().saturating_sub(1))
            };
            search.last_search_query = query;
            search.last_scanned_render_epoch = selected_tab_epoch;
        }
        self.rebuild_terminal_search_highlight_cache(selected_tab);

        if self.tabs.get(selected_tab).map(|tab| !tab.terminal_search.matches.is_empty()).unwrap_or(false) {
            self.scroll_to_search_match();
        }

        if debug_enabled!() {
            let elapsed = search_started_at.elapsed();
            let match_count = self.tabs.get(selected_tab).map(|tab| tab.terminal_search.matches.len()).unwrap_or(0);
            log_debug!("Terminal search updated in {:?} (matches: {})", elapsed, match_count);
        }
    }

    // Keep active match visible in viewport.
    pub(crate) fn scroll_to_search_match(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let selected_tab = self.selected_tab;
        let (abs_row, engine_handle) = {
            let tab = &self.tabs[selected_tab];
            if tab.terminal_search.matches.is_empty() {
                return;
            }
            let Some(session) = &tab.session else {
                return;
            };
            (tab.terminal_search.matches[tab.terminal_search.current].0, session.engine_handle())
        };

        let tab = &mut self.tabs[selected_tab];
        let tab_height = self.tab_content_area.height as i64;

        if let Ok(engine) = engine_handle.lock() {
            let max_scrollback = engine.max_scrollback();

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
