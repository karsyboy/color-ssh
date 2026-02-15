//! Fuzzy search and host filtering logic

use super::App;

/// Fuzzy match scoring for host search
///
/// Returns a score if `pattern` fuzzy-matches against `text`, or `None` if
/// there is no match. Consecutive character matches score higher.
pub fn fuzzy_match(text: &str, pattern: &str) -> Option<i32> {
    let text = text.to_lowercase();
    let pattern = pattern.to_lowercase();

    let mut text_chars = text.chars().peekable();
    let mut pattern_chars = pattern.chars().peekable();
    let mut score = 0;
    let mut consecutive = 0;

    while let Some(&pattern_char) = pattern_chars.peek() {
        let mut found = false;

        while let Some(&text_char) = text_chars.peek() {
            text_chars.next();
            if text_char == pattern_char {
                score += 1 + consecutive;
                consecutive += 1;
                pattern_chars.next();
                found = true;
                break;
            } else {
                consecutive = 0;
            }
        }

        if !found {
            return None; // Pattern not found
        }
    }

    Some(score)
}

impl App {
    /// Update the filtered hosts based on search query with fuzzy matching
    pub(super) fn update_filtered_hosts(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_hosts = (0..self.hosts.len()).map(|i| (i, 0)).collect();
        } else {
            let query = &self.search_query;
            let mut matches: Vec<(usize, i32)> = self
                .hosts
                .iter()
                .enumerate()
                .filter_map(|(idx, host)| {
                    let mut best_score = None;

                    // Try matching against name
                    if let Some(score) = fuzzy_match(&host.name, query) {
                        best_score = Some(score + 100); // Boost name matches
                    }

                    // Try matching against hostname
                    if let Some(hostname) = &host.hostname
                        && let Some(score) = fuzzy_match(hostname, query)
                    {
                        best_score = Some(best_score.unwrap_or(0).max(score + 50));
                    }

                    // Try matching against user
                    if let Some(user) = &host.user
                        && let Some(score) = fuzzy_match(user, query)
                    {
                        best_score = Some(best_score.unwrap_or(0).max(score + 30));
                    }

                    best_score.map(|score| (idx, score))
                })
                .collect();

            // Sort by score descending
            matches.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered_hosts = matches;
        }

        // Reset selection
        self.selected_host = 0;
        if !self.filtered_hosts.is_empty() {
            self.host_list_state.select(Some(0));
        } else {
            self.host_list_state.select(None);
        }
        self.host_scroll_offset = 0;
    }

    /// Update host list scroll to keep selection visible
    pub(super) fn update_host_scroll(&mut self, viewport_height: usize) {
        if self.filtered_hosts.is_empty() {
            return;
        }

        // Symmetric scrolling: keep cursor in viewport before scrolling
        if self.selected_host < self.host_scroll_offset {
            self.host_scroll_offset = self.selected_host;
        } else if self.selected_host >= self.host_scroll_offset + viewport_height {
            self.host_scroll_offset = self.selected_host.saturating_sub(viewport_height - 1);
        }
    }
}
