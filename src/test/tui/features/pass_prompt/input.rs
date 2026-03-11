use super::*;
use crate::config::AuthSettings;
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::tui::VaultUnlockAction;

fn sample_host(protocol: ConnectionProtocol) -> InventoryHost {
    let mut host = InventoryHost::new("session".to_string());
    host.protocol = protocol;
    host.host = "remote.internal".to_string();
    host.user = Some("alice".to_string());
    host
}

#[test]
fn vault_unlock_fallback_notice_for_rdp_mentions_freerdp_prompt() {
    let app = AppState::new_for_tests();
    let host = sample_host(ConnectionProtocol::Rdp);
    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host),
        force_ssh_logging: false,
        auth_settings: AuthSettings::default(),
    };

    let notice = app.vault_unlock_fallback_notice(&action, "Password vault unlock canceled");

    assert!(notice.contains("FreeRDP password prompt"));
    assert!(!notice.contains("standard SSH password prompt"));
}

#[test]
fn vault_unlock_fallback_notice_for_ssh_keeps_standard_prompt_text() {
    let app = AppState::new_for_tests();
    let host = sample_host(ConnectionProtocol::Ssh);
    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host),
        force_ssh_logging: false,
        auth_settings: AuthSettings::default(),
    };

    let notice = app.vault_unlock_fallback_notice(&action, "Password vault unlock canceled");

    assert!(notice.contains("standard SSH password prompt"));
    assert!(!notice.contains("FreeRDP password prompt"));
}

#[test]
fn unlock_policy_for_host_action_uses_carried_auth_settings() {
    let app = AppState::new_for_tests();
    let host = sample_host(ConnectionProtocol::Ssh);
    let auth_settings = AuthSettings {
        idle_timeout_seconds: 12,
        session_timeout_seconds: 34,
        ..AuthSettings::default()
    };
    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host),
        force_ssh_logging: false,
        auth_settings,
    };

    let policy = app.unlock_policy_for_action(&action);

    assert_eq!(policy, crate::auth::ipc::UnlockPolicy::new(12, 34));
}
