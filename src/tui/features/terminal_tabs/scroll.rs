//! Tab-bar scrolling helpers.

use crate::tui::SessionManager;

impl SessionManager {
    // Right-most snap point when tabs overflow.
    pub(crate) fn final_right_tab_scroll_offset(&self, available_width: usize) -> usize {
        if self.tabs.is_empty() || available_width == 0 {
            return 0;
        }
        let total_width = self.total_tab_width();
        if total_width <= available_width {
            return 0;
        }

        let visible_with_left_marker = available_width.saturating_sub(1);
        let threshold = total_width.saturating_sub(visible_with_left_marker);

        let mut start = 0usize;
        let mut last_start = 0usize;
        for idx in 0..self.tabs.len() {
            if start >= threshold {
                return start;
            }
            last_start = start;
            start += self.tab_display_width(idx);
        }

        last_start
    }

    // Clamp and snap arbitrary scroll offsets to tab boundaries.
    pub(crate) fn normalize_tab_scroll_offset(&self, raw_offset: usize, available_width: usize) -> usize {
        if self.tabs.is_empty() || available_width == 0 {
            return 0;
        }
        let final_offset = self.final_right_tab_scroll_offset(available_width);
        let clamped = raw_offset.min(final_offset);

        let mut snapped = 0usize;
        let mut start = 0usize;
        for idx in 0..self.tabs.len() {
            if start > clamped {
                break;
            }
            snapped = start;
            start += self.tab_display_width(idx);
        }
        snapped
    }

    // Aggregate tab strip width.
    pub(crate) fn total_tab_width(&self) -> usize {
        (0..self.tabs.len()).map(|idx| self.tab_display_width(idx)).sum()
    }

    // Adjacent scroll positions for marker clicks.
    pub(crate) fn prev_tab_scroll_offset(&self, raw_offset: usize, available_width: usize) -> Option<usize> {
        if self.tabs.is_empty() || available_width == 0 {
            return None;
        }

        let current = self.normalize_tab_scroll_offset(raw_offset, available_width);
        if current == 0 {
            return None;
        }

        let mut previous = 0usize;
        let mut start = 0usize;
        for idx in 0..self.tabs.len() {
            if start >= current {
                break;
            }
            previous = start;
            start += self.tab_display_width(idx);
        }

        Some(previous)
    }

    pub(crate) fn next_tab_scroll_offset(&self, raw_offset: usize, available_width: usize) -> Option<usize> {
        if self.tabs.is_empty() || available_width == 0 {
            return None;
        }

        let total_width = self.total_tab_width();
        if total_width <= available_width {
            return None;
        }

        let current = self.normalize_tab_scroll_offset(raw_offset, available_width);
        let final_offset = self.final_right_tab_scroll_offset(available_width);
        if current >= final_offset {
            return None;
        }

        let mut start = 0usize;
        for idx in 0..self.tabs.len() {
            if start > current {
                return Some(start.min(final_offset));
            }
            start += self.tab_display_width(idx);
        }

        Some(final_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::SessionManager;
    use crate::ssh_config::SshHost;
    use crate::tui::{HostTab, TerminalSearchState};

    fn app_with_tab_titles(titles: &[&str]) -> SessionManager {
        let mut app = SessionManager::new_for_tests();
        for title in titles {
            app.tabs.push(HostTab {
                host: SshHost::new((*title).to_string()),
                title: (*title).to_string(),
                session: None,
                scroll_offset: 0,
                terminal_search: TerminalSearchState::default(),
                force_ssh_logging: false,
                last_pty_size: None,
            });
        }
        app
    }

    #[test]
    fn normalizes_tab_scroll_offset_by_snapping_and_clamping() {
        let app = app_with_tab_titles(&["aaa", "bbb", "ccc"]);
        assert_eq!(app.normalize_tab_scroll_offset(7, 8), 6);
        assert_eq!(app.normalize_tab_scroll_offset(999, 8), 12);
    }

    #[test]
    fn computes_prev_and_next_tab_scroll_offsets_with_overflow() {
        let app = app_with_tab_titles(&["aaa", "bbb", "ccc"]);
        assert_eq!(app.prev_tab_scroll_offset(6, 8), Some(0));
        assert_eq!(app.next_tab_scroll_offset(6, 8), Some(12));
        assert_eq!(app.next_tab_scroll_offset(12, 8), None);
    }

    #[test]
    fn computes_final_right_offset_with_wide_titles() {
        let app = app_with_tab_titles(&["a界", "bbbb", "cc"]);
        assert_eq!(app.final_right_tab_scroll_offset(10), 13);
    }
}
