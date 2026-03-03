use super::*;
use crate::auth::ipc::{VaultStatus, VaultStatusEvent, VaultStatusEventKind};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn apply_vault_status_modal_lock_result_marks_modal_locked_on_success() {
    let mut app = AppState::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());
    app.vault_status = VaultStatus {
        vault_exists: true,
        unlocked: true,
        unlock_expires_in_seconds: Some(300),
        idle_timeout_seconds: Some(900),
        absolute_timeout_seconds: Some(28_800),
        absolute_timeout_at_epoch_seconds: Some(1_700_000_000),
    };

    app.apply_vault_status_modal_lock_result(Ok(VaultStatus::locked(true)));

    assert!(!app.vault_status.unlocked);
    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Vault locked."));
    assert!(!modal.message_is_error);
}

#[test]
fn apply_vault_status_modal_lock_result_stores_error_message_on_failure() {
    let mut app = AppState::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());

    app.apply_vault_status_modal_lock_result(Err(agent::AgentError::Protocol("boom".to_string())));

    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Failed to lock vault: boom"));
    assert!(modal.message_is_error);
}

#[test]
fn handle_vault_status_modal_key_l_reports_already_locked() {
    let mut app = AppState::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());
    app.vault_status = VaultStatus::locked(true);

    app.handle_vault_status_modal_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));

    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Vault already locked."));
    assert!(!modal.message_is_error);
}

#[test]
fn handle_vault_status_notification_marks_modal_locked() {
    let mut app = AppState::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());
    let status = VaultStatus {
        vault_exists: true,
        unlocked: true,
        unlock_expires_in_seconds: Some(300),
        idle_timeout_seconds: Some(900),
        absolute_timeout_seconds: Some(28_800),
        absolute_timeout_at_epoch_seconds: Some(1_700_000_000),
    };
    app.vault_status = status.clone();

    app.handle_vault_status_notification(VaultStatusEvent {
        kind: VaultStatusEventKind::Locked,
        status: VaultStatus::locked(true),
        event_id: 1,
    });

    assert_eq!(app.vault_status, VaultStatus::locked(true));
    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Vault locked."));
    assert!(!modal.message_is_error);
}

#[test]
fn handle_vault_status_notification_marks_modal_unlocked() {
    let mut app = AppState::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());
    app.vault_status = VaultStatus::locked(true);

    let unlocked_status = VaultStatus {
        vault_exists: true,
        unlocked: true,
        unlock_expires_in_seconds: Some(300),
        idle_timeout_seconds: Some(900),
        absolute_timeout_seconds: Some(28_800),
        absolute_timeout_at_epoch_seconds: Some(1_700_000_000),
    };
    app.handle_vault_status_notification(VaultStatusEvent {
        kind: VaultStatusEventKind::Unlocked,
        status: unlocked_status.clone(),
        event_id: 2,
    });

    assert_eq!(app.vault_status, unlocked_status);
    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Vault unlocked."));
    assert!(!modal.message_is_error);
}

#[test]
fn restore_vault_status_modal_sets_error_message() {
    let mut app = AppState::new_for_tests();

    app.restore_vault_status_modal(Some(("Vault unlock failed after multiple attempts. Try again.".to_string(), true)));

    assert!(app.vault_unlock.is_none());
    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Vault unlock failed after multiple attempts. Try again."));
    assert!(modal.message_is_error);
}

#[test]
fn close_manual_vault_unlock_after_attempt_limit_closes_direct_unlock_prompt() {
    let mut app = AppState::new_for_tests();
    app.vault_unlock = Some(VaultUnlockState::new("shared".to_string(), VaultUnlockAction::UnlockVault));

    app.vault_unlock = None;
    app.close_manual_vault_unlock_after_attempt_limit(false);

    assert!(app.vault_unlock.is_none());
    assert!(app.vault_status_modal.is_none());
}

#[test]
fn close_manual_vault_unlock_after_attempt_limit_restores_vault_status_modal() {
    let mut app = AppState::new_for_tests();

    app.close_manual_vault_unlock_after_attempt_limit(true);

    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Vault unlock failed after multiple attempts. Try again."));
    assert!(modal.message_is_error);
}

#[test]
fn handle_vault_unlock_escape_returns_to_vault_status_modal_when_requested() {
    let mut app = AppState::new_for_tests();
    app.vault_unlock = Some(VaultUnlockState::new("shared".to_string(), VaultUnlockAction::UnlockVault).return_to_vault_status());

    app.handle_vault_unlock_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    assert!(app.vault_unlock.is_none());
    assert!(app.vault_status_modal.is_some());
}
