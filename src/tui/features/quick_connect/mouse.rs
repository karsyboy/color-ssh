//! Quick-connect mouse handling.

use crate::tui::{QuickConnectField, SessionManager};
use crossterm::event::{self, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

const USER_ROW: u16 = 0;
const HOST_ROW: u16 = 1;
const PROFILE_ROW: u16 = 2;
const PROFILE_OPTIONS_ROW: u16 = 3;
const LOGGING_ROW: u16 = 4;
const CONNECT_ROW: u16 = 6;
const PROFILE_LIST_PREFIX: &str = "Profiles: ";

impl SessionManager {
    // Modal mouse event entry point.
    pub(crate) fn handle_quick_connect_mouse(&mut self, mouse: event::MouseEvent) {
        let Some((modal_area, inner_area)) = self.quick_connect_modal_layout() else {
            return;
        };

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if !Self::point_in_rect(modal_area, mouse.column, mouse.row) {
                    return;
                }

                if !Self::point_in_rect(inner_area, mouse.column, mouse.row) {
                    return;
                }

                self.handle_quick_connect_left_click(mouse.column, mouse.row, inner_area);
            }
            MouseEventKind::ScrollUp => {
                if Self::quick_connect_profile_rows(inner_area, mouse.column, mouse.row) {
                    self.select_prev_quick_connect_profile();
                }
            }
            MouseEventKind::ScrollDown => {
                if Self::quick_connect_profile_rows(inner_area, mouse.column, mouse.row) {
                    self.select_next_quick_connect_profile();
                }
            }
            _ => {}
        }
    }

    // Left-click routing by form row.
    fn handle_quick_connect_left_click(&mut self, mouse_col: u16, mouse_row: u16, inner_area: Rect) {
        let local_row = mouse_row.saturating_sub(inner_area.y);
        let mut should_submit = false;

        if let Some(form) = self.quick_connect.as_mut() {
            match local_row {
                USER_ROW => {
                    form.selected = QuickConnectField::User;
                }
                HOST_ROW => {
                    form.selected = QuickConnectField::Host;
                }
                PROFILE_ROW => {
                    form.selected = QuickConnectField::Profile;
                }
                PROFILE_OPTIONS_ROW => {
                    form.selected = QuickConnectField::Profile;
                    if let Some(profile_index) =
                        Self::quick_connect_profile_index_at_column(&form.profile_options, inner_area.x + PROFILE_LIST_PREFIX.chars().count() as u16, mouse_col)
                    {
                        form.error = None;
                        form.profile_index = profile_index;
                    }
                }
                LOGGING_ROW => {
                    form.selected = QuickConnectField::Logging;
                    form.ssh_logging = !form.ssh_logging;
                }
                CONNECT_ROW => {
                    form.selected = QuickConnectField::Connect;
                    should_submit = true;
                }
                _ => {}
            }
        }

        if should_submit {
            self.submit_quick_connect_modal();
        }
    }

    // Layout + hit testing helpers.
    fn quick_connect_modal_layout(&self) -> Option<(Rect, Rect)> {
        let form = self.quick_connect.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        let width = full_area.width.clamp(44, 74);
        let height = if form.error.is_some() { 12 } else { 11 };
        let area = Self::centered_rect(width, height, full_area);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        Some((area, inner))
    }

    fn point_in_rect(rect: Rect, col: u16, row: u16) -> bool {
        rect.width > 0 && rect.height > 0 && col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
    }

    fn quick_connect_profile_rows(inner_area: Rect, col: u16, row: u16) -> bool {
        if !Self::point_in_rect(inner_area, col, row) {
            return false;
        }
        let local_row = row.saturating_sub(inner_area.y);
        local_row == PROFILE_ROW || local_row == PROFILE_OPTIONS_ROW
    }

    // Profile selection updates.
    fn select_prev_quick_connect_profile(&mut self) {
        if let Some(form) = self.quick_connect.as_mut() {
            form.selected = QuickConnectField::Profile;
            form.error = None;
            form.select_prev_profile();
        }
    }

    fn select_next_quick_connect_profile(&mut self) {
        if let Some(form) = self.quick_connect.as_mut() {
            form.selected = QuickConnectField::Profile;
            form.error = None;
            form.select_next_profile();
        }
    }

    // Horizontal hit test for the profile list line.
    fn quick_connect_profile_index_at_column(profile_options: &[String], start_col: u16, mouse_col: u16) -> Option<usize> {
        if mouse_col < start_col {
            return None;
        }

        let mut cursor = start_col;
        for (idx, profile_name) in profile_options.iter().enumerate() {
            let width = profile_name.chars().count() as u16;
            let end = cursor.saturating_add(width);
            if mouse_col >= cursor && mouse_col < end {
                return Some(idx);
            }
            cursor = end;
            if idx + 1 < profile_options.len() {
                cursor = cursor.saturating_add(3);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::SessionManager;

    #[test]
    fn profile_column_hit_test_maps_to_profile_index() {
        let options = vec!["default".to_string(), "prod".to_string(), "staging".to_string()];
        let start = 10;

        assert_eq!(SessionManager::quick_connect_profile_index_at_column(&options, start, start), Some(0));
        assert_eq!(SessionManager::quick_connect_profile_index_at_column(&options, start, start + 10), Some(1));
        assert_eq!(SessionManager::quick_connect_profile_index_at_column(&options, start, start + 17), Some(2));
        assert_eq!(SessionManager::quick_connect_profile_index_at_column(&options, start, start + 30), None);
    }
}
