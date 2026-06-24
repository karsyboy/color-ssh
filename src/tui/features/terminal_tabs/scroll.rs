//! Tab-bar scrolling helpers.

use crate::tui::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TabBarViewportMetrics {
    pub(crate) scroll_offset: usize,
    pub(crate) has_left_overflow: bool,
    pub(crate) has_right_overflow: bool,
    pub(crate) left_slot: usize,
    pub(crate) right_slot: usize,
    pub(crate) visible_tab_width: usize,
}

impl AppState {
    pub(crate) fn tab_start_offset(&self, tab_idx: usize) -> usize {
        (0..tab_idx.min(self.tabs.len())).map(|idx| self.tab_display_width(idx)).sum()
    }

    pub(crate) fn tab_bar_viewport_metrics(&self, raw_offset: usize, available_width: usize) -> TabBarViewportMetrics {
        if self.tabs.is_empty() || available_width == 0 {
            return TabBarViewportMetrics {
                scroll_offset: 0,
                has_left_overflow: false,
                has_right_overflow: false,
                left_slot: 0,
                right_slot: 0,
                visible_tab_width: 0,
            };
        }

        let scroll_offset = self.normalize_tab_scroll_offset(raw_offset, available_width);
        let has_left_overflow = self.prev_tab_scroll_offset(scroll_offset, available_width).is_some();
        let left_slot = usize::from(has_left_overflow);
        let has_right_overflow = self.next_tab_scroll_offset(scroll_offset, available_width).is_some();
        let right_slot = usize::from(has_right_overflow);

        TabBarViewportMetrics {
            scroll_offset,
            has_left_overflow,
            has_right_overflow,
            left_slot,
            right_slot,
            visible_tab_width: available_width.saturating_sub(left_slot + right_slot),
        }
    }

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
