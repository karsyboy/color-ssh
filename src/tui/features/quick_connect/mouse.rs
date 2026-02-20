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
const CONNECT_LABEL: &str = "[ Enter ] Connect";
const ACTION_SEPARATOR: &str = " | ";
const CANCEL_LABEL: &str = "[ Esc ] Cancel";
const USER_LABEL_PREFIX: &str = "User: ";
const HOST_LABEL_PREFIX: &str = "Host: ";
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
                    if let Some(form) = self.quick_connect.as_mut() {
                        form.finish_mouse_selection();
                    }
                    return;
                }

                if !Self::point_in_rect(inner_area, mouse.column, mouse.row) {
                    if let Some(form) = self.quick_connect.as_mut() {
                        form.finish_mouse_selection();
                    }
                    return;
                }

                self.handle_quick_connect_left_click(mouse.column, mouse.row, inner_area);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.handle_quick_connect_left_drag(mouse.column, inner_area);
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.handle_quick_connect_left_release(mouse.column, inner_area);
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
        let mut should_close = false;

        if let Some(form) = self.quick_connect.as_mut() {
            match local_row {
                USER_ROW => {
                    form.selected = QuickConnectField::User;
                    if let Some(offset) = Self::quick_connect_text_offset(inner_area, QuickConnectField::User, mouse_col) {
                        form.begin_mouse_selection(QuickConnectField::User, offset);
                    }
                }
                HOST_ROW => {
                    form.selected = QuickConnectField::Host;
                    if let Some(offset) = Self::quick_connect_text_offset(inner_area, QuickConnectField::Host, mouse_col) {
                        form.begin_mouse_selection(QuickConnectField::Host, offset);
                    }
                }
                PROFILE_ROW => {
                    form.finish_mouse_selection();
                    form.selected = QuickConnectField::Profile;
                }
                PROFILE_OPTIONS_ROW => {
                    form.finish_mouse_selection();
                    form.selected = QuickConnectField::Profile;
                    if let Some(profile_index) =
                        Self::quick_connect_profile_index_at_column(&form.profile_options, inner_area.x + PROFILE_LIST_PREFIX.chars().count() as u16, mouse_col)
                    {
                        form.profile_index = profile_index;
                    }
                }
                LOGGING_ROW => {
                    form.finish_mouse_selection();
                    form.selected = QuickConnectField::Logging;
                    form.ssh_logging = !form.ssh_logging;
                }
                CONNECT_ROW => {
                    form.finish_mouse_selection();
                    if let Some(action_field) = Self::quick_connect_action_hit(inner_area, mouse_col, mouse_row) {
                        form.selected = action_field;
                        match action_field {
                            QuickConnectField::Connect => should_submit = true,
                            QuickConnectField::Cancel => should_close = true,
                            _ => {}
                        }
                    }
                }
                _ => {
                    form.finish_mouse_selection();
                }
            }
        }

        if should_submit {
            self.submit_quick_connect_modal();
        } else if should_close {
            self.quick_connect = None;
        }
    }

    fn handle_quick_connect_left_drag(&mut self, mouse_col: u16, inner_area: Rect) {
        let Some(form) = self.quick_connect.as_mut() else {
            return;
        };

        let Some(field) = form.mouse_drag_field() else {
            return;
        };

        if let Some(offset) = Self::quick_connect_text_offset(inner_area, field, mouse_col) {
            form.extend_mouse_selection(offset);
        }
    }

    fn handle_quick_connect_left_release(&mut self, mouse_col: u16, inner_area: Rect) {
        let Some(form) = self.quick_connect.as_mut() else {
            return;
        };

        if let Some(field) = form.mouse_drag_field()
            && let Some(offset) = Self::quick_connect_text_offset(inner_area, field, mouse_col)
        {
            form.extend_mouse_selection(offset);
        }

        form.finish_mouse_selection();
    }

    // Layout + hit testing helpers.
    fn quick_connect_modal_layout(&self) -> Option<(Rect, Rect)> {
        let _form = self.quick_connect.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        let width = full_area.width.clamp(44, 74);
        let height = 9;
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

    fn quick_connect_action_hit(inner_area: Rect, col: u16, row: u16) -> Option<QuickConnectField> {
        if !Self::point_in_rect(inner_area, col, row) {
            return None;
        }

        let local_row = row.saturating_sub(inner_area.y);
        if local_row != CONNECT_ROW {
            return None;
        }

        let connect_start_col = inner_area.x;
        let connect_end_col = connect_start_col.saturating_add(CONNECT_LABEL.chars().count() as u16);
        if col >= connect_start_col && col < connect_end_col {
            return Some(QuickConnectField::Connect);
        }

        let cancel_start_col = connect_end_col.saturating_add(ACTION_SEPARATOR.chars().count() as u16);
        let cancel_end_col = cancel_start_col.saturating_add(CANCEL_LABEL.chars().count() as u16);
        if col >= cancel_start_col && col < cancel_end_col {
            return Some(QuickConnectField::Cancel);
        }

        None
    }

    fn quick_connect_text_offset(inner_area: Rect, field: QuickConnectField, mouse_col: u16) -> Option<usize> {
        let start_col = match field {
            QuickConnectField::User => inner_area.x.saturating_add(USER_LABEL_PREFIX.chars().count() as u16),
            QuickConnectField::Host => inner_area.x.saturating_add(HOST_LABEL_PREFIX.chars().count() as u16),
            _ => return None,
        };
        Some(mouse_col.saturating_sub(start_col) as usize)
    }

    // Profile selection updates.
    fn select_prev_quick_connect_profile(&mut self) {
        if let Some(form) = self.quick_connect.as_mut() {
            form.selected = QuickConnectField::Profile;
            form.select_prev_profile();
        }
    }

    fn select_next_quick_connect_profile(&mut self) {
        if let Some(form) = self.quick_connect.as_mut() {
            form.selected = QuickConnectField::Profile;
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
