//! Launch-time RDP credential modal mouse handling.

use crate::tui::{AppState, RdpCredentialsField};
use crossterm::event::{self, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

const TARGET_ROW: u16 = 0;
const USER_ROW: u16 = 1;
const DOMAIN_ROW: u16 = 2;
const PORT_ROW: u16 = 3;
const PASSWORD_ROW: u16 = 4;
const ACTION_ROW: u16 = 6;
const SUBMIT_LABEL: &str = "[Enter] Launch";
const ACTION_SEPARATOR: &str = "  |  ";
const CANCEL_LABEL: &str = "[Esc] Cancel";
const USER_LABEL_PREFIX: &str = "User: ";
const DOMAIN_LABEL_PREFIX: &str = "Domain: ";
const PORT_LABEL_PREFIX: &str = "Port: ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RdpCredentialsMouseAction {
    Submit,
    Close,
}

impl AppState {
    pub(crate) fn handle_rdp_credentials_mouse(&mut self, mouse: event::MouseEvent) {
        let Some((_, inner_area)) = self.rdp_credentials_modal_layout() else {
            return;
        };

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if !Self::rdp_prompt_point_in_rect(inner_area, mouse.column, mouse.row) {
                    if let Some(prompt) = self.rdp_credentials.as_mut() {
                        prompt.finish_mouse_selection();
                    }
                    return;
                }
                self.handle_rdp_credentials_left_click(mouse.column, mouse.row, inner_area);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.handle_rdp_credentials_left_drag(mouse.column, inner_area);
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.handle_rdp_credentials_left_release(mouse.column, inner_area);
            }
            _ => {}
        }
    }

    fn handle_rdp_credentials_left_click(&mut self, mouse_col: u16, mouse_row: u16, inner_area: Rect) {
        let local_row = mouse_row.saturating_sub(inner_area.y);
        let mut deferred_action = None;
        if let Some(prompt) = self.rdp_credentials.as_mut() {
            match local_row {
                TARGET_ROW => prompt.finish_mouse_selection(),
                USER_ROW => {
                    prompt.selected = RdpCredentialsField::User;
                    if let Some(offset) = Self::rdp_credentials_text_offset(inner_area, RdpCredentialsField::User, mouse_col) {
                        prompt.begin_mouse_selection(RdpCredentialsField::User, offset);
                    }
                    prompt.error = None;
                }
                DOMAIN_ROW => {
                    prompt.selected = RdpCredentialsField::Domain;
                    if let Some(offset) = Self::rdp_credentials_text_offset(inner_area, RdpCredentialsField::Domain, mouse_col) {
                        prompt.begin_mouse_selection(RdpCredentialsField::Domain, offset);
                    }
                    prompt.error = None;
                }
                PORT_ROW => {
                    prompt.selected = RdpCredentialsField::Port;
                    if let Some(offset) = Self::rdp_credentials_text_offset(inner_area, RdpCredentialsField::Port, mouse_col) {
                        prompt.begin_mouse_selection(RdpCredentialsField::Port, offset);
                    }
                    prompt.error = None;
                }
                PASSWORD_ROW => {
                    prompt.finish_mouse_selection();
                    prompt.selected = RdpCredentialsField::Password;
                    prompt.move_cursor_end(RdpCredentialsField::Password);
                    prompt.error = None;
                }
                ACTION_ROW => {
                    prompt.finish_mouse_selection();
                    deferred_action = self.rdp_credentials_mouse_action_at(inner_area, mouse_col);
                }
                _ => prompt.finish_mouse_selection(),
            }
        }

        if let Some(action) = deferred_action {
            match action {
                RdpCredentialsMouseAction::Submit => self.handle_rdp_credentials_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                RdpCredentialsMouseAction::Close => self.handle_rdp_credentials_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            }
        }
    }

    fn handle_rdp_credentials_left_drag(&mut self, mouse_col: u16, inner_area: Rect) {
        let Some(prompt) = self.rdp_credentials.as_mut() else {
            return;
        };

        let Some(field) = prompt.mouse_drag_field() else {
            return;
        };

        if let Some(offset) = Self::rdp_credentials_text_offset(inner_area, field, mouse_col) {
            prompt.extend_mouse_selection(offset);
        }
    }

    fn handle_rdp_credentials_left_release(&mut self, mouse_col: u16, inner_area: Rect) {
        let Some(prompt) = self.rdp_credentials.as_mut() else {
            return;
        };

        if let Some(field) = prompt.mouse_drag_field()
            && let Some(offset) = Self::rdp_credentials_text_offset(inner_area, field, mouse_col)
        {
            prompt.extend_mouse_selection(offset);
        }
        prompt.finish_mouse_selection();
    }

    pub(crate) fn rdp_credentials_modal_layout(&self) -> Option<(Rect, Rect)> {
        let _prompt = self.rdp_credentials.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        let width = full_area.width.clamp(56, 90);
        let height = 9;
        let area = Self::centered_rect(width, height, full_area);
        Some((
            area,
            Rect::new(
                area.x.saturating_add(1),
                area.y.saturating_add(1),
                area.width.saturating_sub(2),
                area.height.saturating_sub(2),
            ),
        ))
    }

    fn rdp_credentials_mouse_action_at(&self, inner_area: Rect, column: u16) -> Option<RdpCredentialsMouseAction> {
        let submit_end = inner_area.x.saturating_add(SUBMIT_LABEL.chars().count() as u16);
        if column >= inner_area.x && column < submit_end {
            return Some(RdpCredentialsMouseAction::Submit);
        }

        let cancel_start = submit_end.saturating_add(ACTION_SEPARATOR.chars().count() as u16);
        let cancel_end = cancel_start.saturating_add(CANCEL_LABEL.chars().count() as u16);
        if column >= cancel_start && column < cancel_end {
            return Some(RdpCredentialsMouseAction::Close);
        }

        None
    }

    fn rdp_credentials_text_offset(inner_area: Rect, field: RdpCredentialsField, mouse_col: u16) -> Option<usize> {
        let start_col = match field {
            RdpCredentialsField::User => inner_area.x.saturating_add(USER_LABEL_PREFIX.chars().count() as u16),
            RdpCredentialsField::Domain => inner_area.x.saturating_add(DOMAIN_LABEL_PREFIX.chars().count() as u16),
            RdpCredentialsField::Port => inner_area.x.saturating_add(PORT_LABEL_PREFIX.chars().count() as u16),
            RdpCredentialsField::Password => return None,
        };
        Some(mouse_col.saturating_sub(start_col) as usize)
    }

    fn rdp_prompt_point_in_rect(rect: Rect, col: u16, row: u16) -> bool {
        rect.width > 0 && rect.height > 0 && col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
    }
}
