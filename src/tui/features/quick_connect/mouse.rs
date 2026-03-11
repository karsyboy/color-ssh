//! Quick-connect mouse handling.

use crate::tui::{AppState, QuickConnectField, QuickConnectRow};
use crossterm::event::{self, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

const CONNECT_LABEL: &str = "[ Enter ] Connect";
const ACTION_SEPARATOR: &str = " | ";
const CANCEL_LABEL: &str = "[ Esc ] Cancel";
const USER_LABEL_PREFIX: &str = "User: ";
const HOST_LABEL_PREFIX: &str = "Host: ";
const PORT_LABEL_PREFIX: &str = "Port: ";
const DOMAIN_LABEL_PREFIX: &str = "Domain: ";
const PROFILE_LIST_PREFIX: &str = "Profiles: ";

impl AppState {
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
                if self.quick_connect_profile_rows(inner_area, mouse.column, mouse.row) {
                    self.select_prev_quick_connect_profile();
                }
            }
            MouseEventKind::ScrollDown => {
                if self.quick_connect_profile_rows(inner_area, mouse.column, mouse.row) {
                    self.select_next_quick_connect_profile();
                }
            }
            _ => {}
        }
    }

    fn handle_quick_connect_left_click(&mut self, mouse_col: u16, mouse_row: u16, inner_area: Rect) {
        let local_row = mouse_row.saturating_sub(inner_area.y);
        let mut should_submit = false;
        let mut should_close = false;
        let row_kind = self.quick_connect.as_ref().and_then(|form| form.modal_rows().get(local_row as usize).copied());

        if let Some(form) = self.quick_connect.as_mut() {
            match row_kind {
                Some(QuickConnectRow::Field(QuickConnectField::Protocol)) => {
                    form.finish_mouse_selection();
                    form.selected = QuickConnectField::Protocol;
                    form.toggle_protocol_forward();
                }
                Some(QuickConnectRow::Field(QuickConnectField::User)) => {
                    form.selected = QuickConnectField::User;
                    if let Some(offset) = Self::quick_connect_text_offset(inner_area, QuickConnectField::User, mouse_col) {
                        form.begin_mouse_selection(QuickConnectField::User, offset);
                    }
                }
                Some(QuickConnectRow::Field(QuickConnectField::Host)) => {
                    form.selected = QuickConnectField::Host;
                    if let Some(offset) = Self::quick_connect_text_offset(inner_area, QuickConnectField::Host, mouse_col) {
                        form.begin_mouse_selection(QuickConnectField::Host, offset);
                    }
                }
                Some(QuickConnectRow::Field(QuickConnectField::Port)) => {
                    form.selected = QuickConnectField::Port;
                    if let Some(offset) = Self::quick_connect_text_offset(inner_area, QuickConnectField::Port, mouse_col) {
                        form.begin_mouse_selection(QuickConnectField::Port, offset);
                    }
                }
                Some(QuickConnectRow::Field(QuickConnectField::Domain)) => {
                    form.selected = QuickConnectField::Domain;
                    if let Some(offset) = Self::quick_connect_text_offset(inner_area, QuickConnectField::Domain, mouse_col) {
                        form.begin_mouse_selection(QuickConnectField::Domain, offset);
                    }
                }
                Some(QuickConnectRow::Field(QuickConnectField::Password)) => {
                    form.finish_mouse_selection();
                    form.selected = QuickConnectField::Password;
                    form.move_cursor_end(QuickConnectField::Password);
                }
                Some(QuickConnectRow::Field(QuickConnectField::Profile)) => {
                    form.finish_mouse_selection();
                    form.selected = QuickConnectField::Profile;
                }
                Some(QuickConnectRow::ProfileOptions) => {
                    form.finish_mouse_selection();
                    form.selected = QuickConnectField::Profile;
                    if let Some(profile_index) =
                        Self::quick_connect_profile_index_at_column(&form.profile_options, inner_area.x + PROFILE_LIST_PREFIX.chars().count() as u16, mouse_col)
                    {
                        form.profile_index = profile_index;
                    }
                }
                Some(QuickConnectRow::Field(QuickConnectField::Logging)) => {
                    form.finish_mouse_selection();
                    form.selected = QuickConnectField::Logging;
                    form.ssh_logging = !form.ssh_logging;
                }
                Some(QuickConnectRow::Actions) => {
                    form.finish_mouse_selection();
                    if let Some(action_field) = Self::quick_connect_action_hit(inner_area, mouse_col) {
                        form.selected = action_field;
                        match action_field {
                            QuickConnectField::Connect => should_submit = true,
                            QuickConnectField::Cancel => should_close = true,
                            _ => {}
                        }
                    }
                }
                Some(QuickConnectRow::Message) => {
                    form.finish_mouse_selection();
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

    fn quick_connect_modal_layout(&self) -> Option<(Rect, Rect)> {
        let form = self.quick_connect.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        let width = full_area.width.clamp(58, 86);
        let height = form.modal_height();
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

    fn quick_connect_profile_rows(&self, inner_area: Rect, col: u16, row: u16) -> bool {
        if !Self::point_in_rect(inner_area, col, row) {
            return false;
        }

        let Some(form) = self.quick_connect.as_ref() else {
            return false;
        };
        let local_row = row.saturating_sub(inner_area.y) as usize;
        matches!(
            form.modal_rows().get(local_row),
            Some(QuickConnectRow::Field(QuickConnectField::Profile)) | Some(QuickConnectRow::ProfileOptions)
        )
    }

    fn quick_connect_action_hit(inner_area: Rect, col: u16) -> Option<QuickConnectField> {
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
            QuickConnectField::Port => inner_area.x.saturating_add(PORT_LABEL_PREFIX.chars().count() as u16),
            QuickConnectField::Domain => inner_area.x.saturating_add(DOMAIN_LABEL_PREFIX.chars().count() as u16),
            _ => return None,
        };
        Some(mouse_col.saturating_sub(start_col) as usize)
    }

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
