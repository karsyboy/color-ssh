use super::*;
use crate::auth::ipc::VaultStatus;
use crate::tui::{VaultStatusModalState, VaultUnlockAction, VaultUnlockState};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

#[test]
fn vault_unlock_modal_action_hit_detects_submit_and_close() {
    let mut app = SessionManager::new_for_tests();
    app.vault_unlock = Some(VaultUnlockState::new("shared".to_string(), VaultUnlockAction::UnlockVault));

    let (_, inner_area) = app.vault_unlock_modal_layout().expect("vault unlock modal layout");
    let prompt = app.vault_unlock.as_ref().expect("vault unlock state");
    let row = inner_area.y + VAULT_UNLOCK_ACTION_ROW;
    let close_col = inner_area.x + prompt.action.prompt_submit_hint().chars().count() as u16 + action_separator().chars().count() as u16;

    assert_eq!(app.vault_unlock_modal_action_at(inner_area.x, row), Some(VaultUnlockMouseAction::Submit));
    assert_eq!(app.vault_unlock_modal_action_at(close_col, row), Some(VaultUnlockMouseAction::Close));
}

#[test]
fn vault_status_modal_action_hit_detects_unlock_and_close_when_locked() {
    let mut app = SessionManager::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());
    app.vault_status = VaultStatus::locked(true);

    let (_, inner_area) = app.vault_status_modal_layout().expect("vault status modal layout");
    let row = inner_area.y + VAULT_STATUS_ACTION_ROW;
    let close_col = inner_area.x + vault_status_primary_label(false).chars().count() as u16 + action_separator().chars().count() as u16;

    assert_eq!(app.vault_status_modal_action_at(inner_area.x, row), Some(VaultStatusMouseAction::Unlock));
    assert_eq!(app.vault_status_modal_action_at(close_col, row), Some(VaultStatusMouseAction::Close));
}

#[test]
fn vault_status_modal_action_hit_detects_lock_and_close_when_unlocked() {
    let mut app = SessionManager::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());
    app.vault_status = VaultStatus {
        vault_exists: true,
        unlocked: true,
        unlock_expires_in_seconds: Some(300),
        idle_timeout_seconds: Some(900),
        absolute_timeout_seconds: Some(28_800),
        absolute_timeout_at_epoch_seconds: Some(1_700_000_000),
    };

    let (_, inner_area) = app.vault_status_modal_layout().expect("vault status modal layout");
    let row = inner_area.y + VAULT_STATUS_ACTION_ROW;
    let close_col = inner_area.x + vault_status_primary_label(true).chars().count() as u16 + action_separator().chars().count() as u16;

    assert_eq!(app.vault_status_modal_action_at(inner_area.x, row), Some(VaultStatusMouseAction::Lock));
    assert_eq!(app.vault_status_modal_action_at(close_col, row), Some(VaultStatusMouseAction::Close));
}

#[test]
fn handle_vault_unlock_mouse_close_returns_to_vault_status_modal() {
    let mut app = SessionManager::new_for_tests();
    app.vault_unlock = Some(VaultUnlockState::new("shared".to_string(), VaultUnlockAction::UnlockVault).return_to_vault_status());

    let (_, inner_area) = app.vault_unlock_modal_layout().expect("vault unlock modal layout");
    let prompt = app.vault_unlock.as_ref().expect("vault unlock state");
    let close_col = inner_area.x + prompt.action.prompt_submit_hint().chars().count() as u16 + action_separator().chars().count() as u16;
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: close_col,
        row: inner_area.y + VAULT_UNLOCK_ACTION_ROW,
        modifiers: KeyModifiers::NONE,
    };

    app.handle_vault_unlock_mouse(click);

    assert!(app.vault_unlock.is_none());
    assert!(app.vault_status_modal.is_some());
}

#[test]
fn handle_vault_status_modal_mouse_unlock_opens_unlock_prompt() {
    let mut app = SessionManager::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());
    app.vault_status = VaultStatus::locked(true);

    let (_, inner_area) = app.vault_status_modal_layout().expect("vault status modal layout");
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: inner_area.x,
        row: inner_area.y + VAULT_STATUS_ACTION_ROW,
        modifiers: KeyModifiers::NONE,
    };

    app.handle_vault_status_modal_mouse(click);

    assert!(app.vault_status_modal.is_none());
    let prompt = app.vault_unlock.as_ref().expect("vault unlock state");
    assert!(matches!(prompt.action, VaultUnlockAction::UnlockVault));
    assert!(prompt.return_to_vault_status);
}
