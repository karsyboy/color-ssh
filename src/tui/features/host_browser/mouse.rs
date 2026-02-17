//! Host browser mouse helpers.

use crate::tui::SessionManager;

impl SessionManager {
    pub(crate) fn host_scrollbar_x(&self) -> Option<u16> {
        let area = self.host_list_area;
        if !self.host_panel_visible || area.width == 0 || area.height == 0 {
            return None;
        }

        let total_rows = self.visible_host_row_count();
        let viewport_height = area.height as usize;
        if total_rows <= viewport_height {
            return None;
        }

        Some(area.x + area.width.saturating_sub(1))
    }

    pub(crate) fn set_host_scroll_from_scrollbar_row(&mut self, mouse_row: u16) {
        let area = self.host_list_area;
        let total_rows = self.visible_host_row_count();
        let viewport_height = area.height as usize;

        if area.height == 0 || total_rows <= viewport_height {
            return;
        }

        let max_offset = total_rows.saturating_sub(viewport_height);
        let track_len = viewport_height.saturating_sub(1);
        let local_row = mouse_row.saturating_sub(area.y).min(area.height.saturating_sub(1)) as usize;

        let new_offset = if track_len == 0 {
            0
        } else {
            (local_row.saturating_mul(max_offset) + (track_len / 2)) / track_len
        }
        .min(max_offset);

        let relative_row = self
            .selected_host_row
            .saturating_sub(self.host_scroll_offset)
            .min(viewport_height.saturating_sub(1));
        self.host_scroll_offset = new_offset;
        let new_selected = (self.host_scroll_offset + relative_row).min(total_rows.saturating_sub(1));
        self.set_selected_row(new_selected);
    }
}
