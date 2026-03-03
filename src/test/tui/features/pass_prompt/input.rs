use super::*;
use crate::auth::ipc::VaultStatus;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn apply_vault_status_modal_lock_result_marks_modal_locked_on_success() {
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

    app.apply_vault_status_modal_lock_result(Ok(VaultStatus::locked(true)));

    assert!(!app.vault_status.unlocked);
    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Vault locked."));
    assert!(!modal.message_is_error);
}

#[test]
fn apply_vault_status_modal_lock_result_stores_error_message_on_failure() {
    let mut app = SessionManager::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());

    app.apply_vault_status_modal_lock_result(Err(agent::AgentError::Protocol("boom".to_string())));

    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Failed to lock vault: boom"));
    assert!(modal.message_is_error);
}

#[test]
fn handle_vault_status_modal_key_l_reports_already_locked() {
    let mut app = SessionManager::new_for_tests();
    app.vault_status_modal = Some(VaultStatusModalState::new());
    app.vault_status = VaultStatus::locked(true);

    app.handle_vault_status_modal_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));

    let modal = app.vault_status_modal.as_ref().expect("vault status modal");
    assert_eq!(modal.message.as_deref(), Some("Vault already locked."));
    assert!(!modal.message_is_error);
}
