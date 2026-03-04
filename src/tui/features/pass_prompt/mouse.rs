//! Password vault modal mouse handling.

use crate::tui::AppState;
use crossterm::event::{self, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::layout::Rect;

const VAULT_UNLOCK_ACTION_ROW: u16 = 3;
const VAULT_STATUS_ACTION_ROW: u16 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VaultUnlockMouseAction {
    Submit,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VaultStatusMouseAction {
    Lock,
    Unlock,
    Close,
}

fn action_separator() -> &'static str {
    "  |  "
}

fn vault_status_primary_label(unlocked: bool) -> &'static str {
    if unlocked { "[l] Lock" } else { "[v] Unlock" }
}

fn vault_status_close_label(unlocked: bool) -> &'static str {
    if unlocked { "[Enter/Esc/v] Close" } else { "[Enter/Esc] Close" }
}

impl AppState {
    pub(crate) fn handle_vault_unlock_mouse(&mut self, mouse: event::MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }

        let Some(action) = self.vault_unlock_modal_action_at(mouse.column, mouse.row) else {
            return;
        };

        match action {
            VaultUnlockMouseAction::Submit => self.handle_vault_unlock_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            VaultUnlockMouseAction::Close => self.handle_vault_unlock_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        }
    }

    pub(crate) fn handle_vault_status_modal_mouse(&mut self, mouse: event::MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }

        let Some(action) = self.vault_status_modal_action_at(mouse.column, mouse.row) else {
            return;
        };

        match action {
            VaultStatusMouseAction::Lock => self.handle_vault_status_modal_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE)),
            VaultStatusMouseAction::Unlock => self.handle_vault_status_modal_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE)),
            VaultStatusMouseAction::Close => self.handle_vault_status_modal_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        }
    }

    pub(crate) fn vault_unlock_modal_layout(&self) -> Option<(Rect, Rect)> {
        let _prompt = self.vault_unlock.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        let width = full_area.width.clamp(44, 72);
        let height = 6;
        let area = Self::centered_rect(width, height, full_area);
        Some((area, Self::modal_inner_rect(area)))
    }

    pub(crate) fn vault_status_modal_layout(&self) -> Option<(Rect, Rect)> {
        let _modal = self.vault_status_modal.as_ref()?;
        let full_area = Rect::new(0, 0, self.last_terminal_size.0, self.last_terminal_size.1);
        let width = full_area.width.clamp(52, 80);
        let height = 7;
        let area = Self::centered_rect(width, height, full_area);
        Some((area, Self::modal_inner_rect(area)))
    }

    fn vault_unlock_modal_action_at(&self, col: u16, row: u16) -> Option<VaultUnlockMouseAction> {
        let (_, inner_area) = self.vault_unlock_modal_layout()?;
        if !Self::pass_prompt_point_in_rect(inner_area, col, row) || row.saturating_sub(inner_area.y) != VAULT_UNLOCK_ACTION_ROW {
            return None;
        }

        let prompt = self.vault_unlock.as_ref()?;
        let submit_label = prompt.action.prompt_submit_hint();
        let cancel_label = prompt.action.prompt_cancel_hint();

        if Self::column_in_label_range(inner_area.x, submit_label, col) {
            return Some(VaultUnlockMouseAction::Submit);
        }

        let cancel_start = inner_area
            .x
            .saturating_add(submit_label.chars().count() as u16)
            .saturating_add(action_separator().chars().count() as u16);
        if Self::column_in_label_range(cancel_start, cancel_label, col) {
            return Some(VaultUnlockMouseAction::Close);
        }

        None
    }

    fn vault_status_modal_action_at(&self, col: u16, row: u16) -> Option<VaultStatusMouseAction> {
        let (_, inner_area) = self.vault_status_modal_layout()?;
        if !Self::pass_prompt_point_in_rect(inner_area, col, row) || row.saturating_sub(inner_area.y) != VAULT_STATUS_ACTION_ROW {
            return None;
        }

        let primary_label = vault_status_primary_label(self.vault_status.unlocked);
        let primary_action = if self.vault_status.unlocked {
            VaultStatusMouseAction::Lock
        } else {
            VaultStatusMouseAction::Unlock
        };
        let close_label = vault_status_close_label(self.vault_status.unlocked);

        if Self::column_in_label_range(inner_area.x, primary_label, col) {
            return Some(primary_action);
        }

        let close_start = inner_area
            .x
            .saturating_add(primary_label.chars().count() as u16)
            .saturating_add(action_separator().chars().count() as u16);
        if Self::column_in_label_range(close_start, close_label, col) {
            return Some(VaultStatusMouseAction::Close);
        }

        None
    }

    fn modal_inner_rect(area: Rect) -> Rect {
        Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        )
    }

    fn pass_prompt_point_in_rect(rect: Rect, col: u16, row: u16) -> bool {
        rect.width > 0 && rect.height > 0 && col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
    }

    fn column_in_label_range(start_col: u16, label: &str, col: u16) -> bool {
        let end_col = start_col.saturating_add(label.chars().count() as u16);
        col >= start_col && col < end_col
    }
}
