use super::*;
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
    };

    let notice = app.vault_unlock_fallback_notice(&action, "Password vault unlock canceled");

    assert!(notice.contains("standard SSH password prompt"));
    assert!(!notice.contains("FreeRDP password prompt"));
}
