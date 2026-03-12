use super::*;
use crate::auth::secret::{ExposeSecret, sensitive_string};
use crate::config::AuthSettings;
use crate::inventory::ConnectionProtocol;
use crate::test::support::{fs::TestWorkspace, state::TestStateGuard};
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

    assert_eq!(command.launch_mode, RdpLaunchMode::Pty);
    assert_eq!(command.credential_source, RdpCredentialSource::NativePrompt);
    assert_eq!(command.command.program, "xfreerdp");
    assert_eq!(
        command.command.args,
        vec![
            "/u:alice".to_string(),
            "/d:ACME".to_string(),
            "/v:rdp.internal:3390".to_string(),
            "+force-console-callbacks".to_string(),
            "/from-stdin:force".to_string(),
            "/cert:tofu".to_string(),
        ]
    );
    assert!(command.command.stdin_payload.is_none());
    assert!(command.command.fallback_notice.is_none());
}

#[test]
fn build_rdp_command_with_vault_password_uses_stdin_payload() {
    let host = sample_rdp_host();

    let command = build_prepared_rdp_command(
        &host,
        RdpAuthMode::SuppliedPassword {
            password: sensitive_string("super-secret"),
            source: RdpCredentialSource::VaultEntry,
        },
        None,
    )
    .expect("build vault-backed RDP command");

    assert_eq!(command.launch_mode, RdpLaunchMode::CapturedOutput);
    assert_eq!(command.credential_source, RdpCredentialSource::VaultEntry);
    assert_eq!(command.command.program, "xfreerdp");
    assert_eq!(command.command.args, vec!["/args-from:stdin".to_string()]);
    let stdin_payload = command.command.stdin_payload.expect("stdin payload");
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
    assert!(matches!(
        auth_mode,
        RdpAuthMode::SuppliedPassword {
            source: RdpCredentialSource::VaultEntry,
            ..
        }
    ));
}

#[test]
fn vault_resolution_failures_fall_back_to_prompt_with_notice() {
    let mut host = sample_rdp_host();
    host.vault_pass = Some("shared".to_string());

    let cases = [
        ("missing_entry", "password vault entry 'shared' was not found", "shared"),
        (
            "vault_uninitialized",
            "password vault is not initialized; run `cossh vault init` or `cossh vault add <name>`",
            "password vault is not initialized",
        ),
        (
            "unlock_failure",
            "failed to unlock password vault after 3 attempts",
            "failed to unlock password vault after 3 attempts",
        ),
    ];

    for (case_name, resolver_error, expected_fragment) in cases {
        let (auth_mode, fallback_notice) = resolve_rdp_auth_mode_with(&host, None, |_| Err(resolver_error.to_string()));
        let command = build_prepared_rdp_command(&host, auth_mode, fallback_notice).expect("build fallback RDP command");

        assert_eq!(command.launch_mode, RdpLaunchMode::Pty, "expected PTY mode for case: {case_name}");
        assert!(command.command.stdin_payload.is_none(), "stdin payload should be absent for case: {case_name}");

        let fallback_notice = command.command.fallback_notice.expect("fallback notice should be present");
        assert!(
            fallback_notice.contains("FreeRDP password prompt"),
            "fallback notice should mention FreeRDP prompt for case: {case_name}"
        );
        assert!(
            fallback_notice.contains(expected_fragment),
            "fallback notice should include '{expected_fragment}' for case: {case_name}"
        );
    }
}

#[test]
fn existing_cert_flags_suppress_default_cert_tofu() {
    let mut host = sample_rdp_host();
    host.rdp.args.push("/cert:ignore".to_string());

    let command = build_rdp_command_for_host(&host, None).expect("build RDP command with explicit cert behavior");

    assert!(command.command.args.iter().any(|arg| arg == "/cert:ignore"));
    assert!(!command.command.args.iter().any(|arg| arg == "/cert:tofu"));
}

#[test]
fn direct_rdp_command_prompts_for_missing_username_and_uses_native_prompt_mode() {
    let args = RdpCommandArgs {
        target: "desktop01".to_string(),
        user: None,
        domain: None,
        port: None,
        extra_args: Vec::new(),
    };
    let prompted_hosts = RefCell::new(Vec::new());

    let command = build_rdp_command_with_prompts(&args, None, true, |host| {
        prompted_hosts.borrow_mut().push(format!("user:{}", host.host));
        Ok("alice".to_string())
    })
    .expect("build native-prompt direct RDP command");

    assert_eq!(prompted_hosts.into_inner(), vec!["user:desktop01".to_string()]);
    assert_eq!(command.launch_mode, RdpLaunchMode::Pty);
    assert_eq!(command.credential_source, RdpCredentialSource::NativePrompt);
    assert!(command.command.stdin_payload.is_none());
    assert!(command.command.args.iter().any(|arg| arg == "/from-stdin:force"));
}

#[test]
fn direct_rdp_command_without_terminal_and_missing_username_errors() {
    let args = RdpCommandArgs {
        target: "desktop01".to_string(),
        user: None,
        domain: None,
        port: None,
        extra_args: Vec::new(),
    };

    let err =
        build_rdp_command_with_prompts(&args, None, false, |_| Ok("ignored".to_string())).expect_err("missing username should fail without terminal prompting");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(err.to_string().contains("RDP username is required"));
}

#[test]
fn direct_rdp_command_without_terminal_keeps_native_prompt_mode() {
    let args = RdpCommandArgs {
        target: "desktop01".to_string(),
        user: Some("alice".to_string()),
        domain: None,
        port: None,
        extra_args: Vec::new(),
    };

    let command = build_rdp_command_with_prompts(&args, None, false, |_| Ok("ignored".to_string())).expect("build native-prompt direct RDP command");

    assert_eq!(command.launch_mode, RdpLaunchMode::Pty);
    assert_eq!(command.credential_source, RdpCredentialSource::NativePrompt);
    assert!(command.command.stdin_payload.is_none());
    assert!(command.command.args.iter().any(|arg| arg == "/from-stdin:force"));
}

#[test]
fn direct_vault_autologin_failure_with_explicit_pass_entry_is_hard_error() {
    let err = direct_rdp_vault_autologin_failure("password vault entry 'shared' was not found", true, true)
        .expect_err("explicit pass-entry failures should not silently fall back");

    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(err.to_string().contains("explicit --pass-entry"));
}

#[test]
fn direct_vault_autologin_failure_without_terminal_is_hard_error() {
    let err = direct_rdp_vault_autologin_failure("password vault is locked; run `cossh vault unlock`", false, false).expect_err("non-tty fallback should fail");

    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(err.to_string().contains("no interactive terminal"));
}

#[test]
fn direct_vault_autologin_failure_with_terminal_falls_back_to_native_prompt() {
    let (mode, notice) = direct_rdp_vault_autologin_failure("password vault is locked; run `cossh vault unlock`", false, true)
        .expect("interactive terminals should still support native FreeRDP fallback");

    assert_eq!(mode, RdpAuthMode::NativePrompt);
    assert!(notice.expect("fallback notice").contains("FreeRDP password prompt"));
}

#[test]
fn resolve_rdp_auth_mode_with_invalid_password_falls_back_to_native_prompt() {
    let mut host = sample_rdp_host();
    host.vault_pass = Some("shared".to_string());

    let (mode, notice) = resolve_rdp_auth_mode_with(&host, None, |_| Ok(sensitive_string("bad\nsecret")));

    assert_eq!(mode, RdpAuthMode::NativePrompt);
    let notice = notice.expect("fallback notice");
    assert!(notice.contains("cannot contain carriage returns, newlines, or NUL characters"));
}

#[test]
fn build_rdp_command_rejects_control_characters_in_extra_args() {
    let mut host = sample_rdp_host();
    host.rdp.args.push("/app:calc\n/drive:foo,/tmp".to_string());

    let err = build_rdp_command_for_host(&host, None).expect_err("control characters in startup args should be rejected");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(err.to_string().contains("RDP extra argument"));
}

#[test]
fn build_rdp_command_trims_user_and_domain_values() {
    let mut host = sample_rdp_host();
    host.user = Some(" alice ".to_string());
    host.rdp.domain = Some(" ACME ".to_string());

    let command = build_rdp_command_for_host(&host, None).expect("trimmed user/domain should still build");

    assert!(command.command.args.iter().any(|arg| arg == "/u:alice"));
    assert!(command.command.args.iter().any(|arg| arg == "/d:ACME"));
}

#[test]
fn direct_rdp_auth_resolution_skips_vault_when_direct_autologin_disabled() {
    let mut host = sample_rdp_host();
    host.vault_pass = Some("shared".to_string());
    let auth_settings = AuthSettings {
        direct_password_autologin: false,
        ..AuthSettings::default()
    };

    let (mode, notice) = resolve_direct_rdp_auth_mode_with(&host, None, &auth_settings, true).expect("direct autologin disabled should bypass vault lookup");

    assert_eq!(mode, RdpAuthMode::NativePrompt);
    assert!(notice.is_none());
}

#[test]
fn configured_rdp_host_for_args_ignores_non_rdp_inventory_matches() {
    let guard = TestStateGuard::lock();
    let workspace = TestWorkspace::new("process", "rdp_resolution").expect("temp workspace");
    let home_dir = workspace.join("home");
    let current_dir = workspace.join("cwd");
    workspace
        .write_rel(
            "home/.color-ssh/cossh-inventory.yaml",
            r#"
inventory:
  - name: router01
    protocol: ssh
    host: 10.0.0.10
    user: admin
    vault_pass: shared
"#,
        )
        .expect("write inventory");

    guard
        .with_home_and_cwd(&home_dir, &current_dir, || {
            let args = RdpCommandArgs {
                target: "router01".to_string(),
                user: Some("alice".to_string()),
                domain: None,
                port: None,
                extra_args: Vec::new(),
            };
            let host = configured_rdp_host_for_args(&args);

            assert_eq!(host.protocol, ConnectionProtocol::Rdp);
            assert_eq!(host.host, "router01");
            assert_eq!(host.user.as_deref(), Some("alice"));
            assert!(host.vault_pass.is_none());
        })
        .expect("set test HOME/cwd");
}
