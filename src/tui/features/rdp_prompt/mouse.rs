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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RdpCredentialsMouseAction {
    Submit,
    Close,
}

impl AppState {
    pub(crate) fn handle_rdp_credentials_mouse(&mut self, mouse: event::MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }

        let Some((_, inner_area)) = self.rdp_credentials_modal_layout() else {
            return;
        };
        if !Self::rdp_prompt_point_in_rect(inner_area, mouse.column, mouse.row) {
            return;
        }

        let local_row = mouse.row.saturating_sub(inner_area.y);
        let mut deferred_action = None;
        if let Some(prompt) = self.rdp_credentials.as_mut() {
            match local_row {
                TARGET_ROW => {}
                USER_ROW => {
                    prompt.selected = RdpCredentialsField::User;
                    prompt.move_cursor_end(RdpCredentialsField::User);
                    prompt.error = None;
                }
                DOMAIN_ROW => {
                    prompt.selected = RdpCredentialsField::Domain;
                    prompt.move_cursor_end(RdpCredentialsField::Domain);
                    prompt.error = None;
                }
                PORT_ROW => {
                    prompt.selected = RdpCredentialsField::Port;
                    prompt.move_cursor_end(RdpCredentialsField::Port);
                    prompt.error = None;
                }
                PASSWORD_ROW => {
                    prompt.selected = RdpCredentialsField::Password;
                    prompt.move_cursor_end(RdpCredentialsField::Password);
                    prompt.error = None;
                }
                ACTION_ROW => {
                    deferred_action = self.rdp_credentials_mouse_action_at(inner_area, mouse.column);
                }
                _ => {}
            }
        }

        if let Some(action) = deferred_action {
            match action {
                RdpCredentialsMouseAction::Submit => self.handle_rdp_credentials_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                RdpCredentialsMouseAction::Close => self.handle_rdp_credentials_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            }
        }
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

    fn rdp_prompt_point_in_rect(rect: Rect, col: u16, row: u16) -> bool {
        rect.width > 0 && rect.height > 0 && col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
    }
}
