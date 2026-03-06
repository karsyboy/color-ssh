use super::*;
use crate::auth::secret::{ExposeSecret, sensitive_string};
use crate::inventory::ConnectionProtocol;
use std::cell::RefCell;

fn sample_rdp_host() -> InventoryHost {
    let mut host = InventoryHost::new("desktop01".to_string());
    host.protocol = ConnectionProtocol::Rdp;
    host.host = "rdp.internal".to_string();
    host.user = Some("alice".to_string());
    host.port = Some(3390);
    host.rdp.domain = Some("ACME".to_string());
    host
}

#[test]
fn build_rdp_command_without_vault_pass_uses_native_prompt_mode() {
    let host = sample_rdp_host();

    let command = build_rdp_command_for_host(&host, None).expect("build prompt-mode RDP command");

    assert_eq!(command.program, "xfreerdp");
    assert_eq!(
        command.args,
        vec![
            "/u:alice".to_string(),
            "/d:ACME".to_string(),
            "/v:rdp.internal:3390".to_string(),
            "+force-console-callbacks".to_string(),
            "/from-stdin:force".to_string(),
            "/cert:tofu".to_string(),
        ]
    );
    assert!(command.stdin_payload.is_none());
    assert!(command.fallback_notice.is_none());
}

#[test]
fn build_rdp_command_with_vault_password_uses_stdin_payload() {
    let host = sample_rdp_host();

    let command =
        build_prepared_rdp_command(&host, RdpAuthMode::VaultInjectedPassword(sensitive_string("super-secret")), None).expect("build vault-backed RDP command");

    assert_eq!(command.program, "xfreerdp");
    assert_eq!(command.args, vec!["/args-from:stdin".to_string()]);
    let stdin_payload = command.stdin_payload.expect("stdin payload");
    let payload = stdin_payload.expose_secret();
    assert!(payload.contains("/u:alice"));
    assert!(payload.contains("/d:ACME"));
    assert!(payload.contains("/v:rdp.internal:3390"));
    assert!(payload.contains("/p:super-secret"));
}

#[test]
fn explicit_pass_entry_overrides_inventory_pass_for_resolution() {
    let mut host = sample_rdp_host();
    host.vault_pass = Some("inventory-pass".to_string());
    let resolved_entry = RefCell::new(String::new());

    let (auth_mode, fallback_notice) = resolve_rdp_auth_mode_with(&host, Some("override-pass"), |entry| {
        resolved_entry.replace(entry.to_string());
        Ok(sensitive_string("resolved-password"))
    });

    assert_eq!(resolved_entry.into_inner(), "override-pass");
    assert!(fallback_notice.is_none());
    assert!(matches!(auth_mode, RdpAuthMode::VaultInjectedPassword(_)));
}

#[test]
fn missing_vault_entry_falls_back_to_prompt_with_notice() {
    let mut host = sample_rdp_host();
    host.vault_pass = Some("shared".to_string());

    let (auth_mode, fallback_notice) = resolve_rdp_auth_mode_with(&host, None, |_| Err("password vault entry 'shared' was not found".to_string()));
    let command = build_prepared_rdp_command(&host, auth_mode, fallback_notice.clone()).expect("build fallback RDP command");

    assert!(command.stdin_payload.is_none());
    assert!(matches!(command.fallback_notice.as_deref(), Some(notice) if notice.contains("FreeRDP password prompt")));
    assert!(matches!(fallback_notice.as_deref(), Some(notice) if notice.contains("shared")));
}

#[test]
fn vault_uninitialized_falls_back_to_prompt_with_notice() {
    let mut host = sample_rdp_host();
    host.vault_pass = Some("shared".to_string());

    let (auth_mode, fallback_notice) = resolve_rdp_auth_mode_with(&host, None, |_| {
        Err("password vault is not initialized; run `cossh vault init` or `cossh vault add <name>`".to_string())
    });
    let command = build_prepared_rdp_command(&host, auth_mode, fallback_notice.clone()).expect("build uninitialized-vault fallback RDP command");

    assert!(command.stdin_payload.is_none());
    assert!(matches!(
        fallback_notice.as_deref(),
        Some(notice) if notice.contains("password vault is not initialized") && notice.contains("FreeRDP password prompt")
    ));
}

#[test]
fn vault_unlock_failure_falls_back_to_prompt_with_notice() {
    let mut host = sample_rdp_host();
    host.vault_pass = Some("shared".to_string());

    let (auth_mode, fallback_notice) = resolve_rdp_auth_mode_with(&host, None, |_| Err("failed to unlock password vault after 3 attempts".to_string()));
    let command = build_prepared_rdp_command(&host, auth_mode, fallback_notice.clone()).expect("build unlock-failure fallback RDP command");

    assert!(command.stdin_payload.is_none());
    assert!(matches!(
        command.fallback_notice.as_deref(),
        Some(notice) if notice.contains("failed to unlock password vault after 3 attempts") && notice.contains("FreeRDP password prompt")
    ));
}

#[test]
fn existing_cert_flags_suppress_default_cert_tofu() {
    let mut host = sample_rdp_host();
    host.rdp.args.push("/cert:ignore".to_string());

    let command = build_rdp_command_for_host(&host, None).expect("build RDP command with explicit cert behavior");

    assert!(command.args.iter().any(|arg| arg == "/cert:ignore"));
    assert!(!command.args.iter().any(|arg| arg == "/cert:tofu"));
}
