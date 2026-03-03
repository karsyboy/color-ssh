use super::SessionManager;
use crate::auth::ipc::VaultStatus;
use crate::tui::VaultUnlockAction;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn paste_inserts_host_search_text_at_cursor() {
    let mut app = SessionManager::new_for_tests();
    app.search_mode = true;
    app.search_query = "core-router".to_string();
    app.search_query_cursor = 4;
    app.search_query_selection = None;

    app.handle_search_paste("-edge");

    assert_eq!(app.search_query, "core-edge-router");
    assert_eq!(app.search_query_cursor, 9);
    assert_eq!(app.search_query_selection, None);
}

#[test]
fn host_search_supports_middle_insert_and_delete() {
    let mut app = SessionManager::new_for_tests();
    app.search_mode = true;
    app.search_query = "admn".to_string();
    app.search_query_cursor = 3;
    app.search_query_selection = None;

    app.handle_search_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)).expect("insert");
    assert_eq!(app.search_query, "admin");

    app.handle_search_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)).expect("left");
    app.handle_search_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)).expect("delete");
    assert_eq!(app.search_query, "admn");
}

#[test]
fn manager_v_shortcut_opens_manual_vault_unlock_when_locked() {
    let mut app = SessionManager::new_for_tests();
    app.vault_status = VaultStatus::locked(true);

    app.handle_manager_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE))
        .expect("vault unlock shortcut");

    let prompt = app.vault_unlock.as_ref().expect("vault unlock prompt");
    assert!(matches!(prompt.action, VaultUnlockAction::UnlockVault));
}

#[test]
fn manager_v_shortcut_opens_status_modal_when_vault_is_already_unlocked() {
    let mut app = SessionManager::new_for_tests();
    app.vault_status = VaultStatus {
        vault_exists: true,
        unlocked: true,
        unlock_expires_in_seconds: Some(300),
        idle_timeout_seconds: Some(900),
        absolute_timeout_seconds: Some(28_800),
        absolute_timeout_at_epoch_seconds: Some(1_700_000_000),
    };

    app.handle_manager_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE))
        .expect("vault unlock shortcut");

    assert!(app.vault_unlock.is_none());
    assert!(app.vault_status_modal.is_some());
}
