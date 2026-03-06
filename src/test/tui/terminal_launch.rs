use super::*;
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::tui::VaultUnlockAction;

fn sample_rdp_host() -> InventoryHost {
    let mut host = InventoryHost::new("desktop01".to_string());
    host.protocol = ConnectionProtocol::Rdp;
    host.host = "rdp.internal".to_string();
    host.user = Some("alice".to_string());
    host
}

#[test]
fn auto_login_notice_for_rdp_mentions_freerdp_prompt() {
    let host = sample_rdp_host();

    let notice = auto_login_notice(&host, "Password vault unlock canceled");

    assert!(notice.contains("FreeRDP password prompt"));
    assert!(!notice.contains("standard SSH password prompt"));
}

#[test]
fn resolve_host_pass_password_for_rdp_without_vault_pass_is_launchable() {
    let mut app = AppState::new_for_tests();
    let host = sample_rdp_host();
    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host.clone()),
        force_ssh_logging: false,
    };

    let resolution = app.resolve_host_pass_password_with_autologin(&host, action, true);

    assert_eq!(
        resolution,
        Some(HostPassResolution {
            pass_entry_override: None,
            pass_fallback_notice: None,
            disable_vault_autologin: false,
        })
    );
}

#[test]
fn resolve_host_pass_password_for_rdp_with_tui_autologin_disabled_is_launchable() {
    let mut app = AppState::new_for_tests();
    let mut host = sample_rdp_host();
    host.vault_pass = Some("shared".to_string());
    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host.clone()),
        force_ssh_logging: false,
    };

    let resolution = app.resolve_host_pass_password_with_autologin(&host, action, false);

    assert_eq!(
        resolution,
        Some(HostPassResolution {
            pass_entry_override: None,
            pass_fallback_notice: None,
            disable_vault_autologin: true,
        })
    );
}
