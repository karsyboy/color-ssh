use super::exit::map_exit_code;
use super::launch::{build_plain_ssh_command, resolve_host_by_destination, resolve_pass_entry_from_hosts, synthesize_ssh_args};
use super::stream::{requires_immediate_terminal_flush, should_flush_immediately};
use crate::inventory::InventoryHost;
use std::process::ExitCode;

#[test]
fn returns_success_exit_code_for_success_status() {
    assert_eq!(map_exit_code(true, Some(0)), ExitCode::SUCCESS);
}

#[test]
fn preserves_non_zero_exit_status_in_u8_range() {
    assert_eq!(map_exit_code(false, Some(23)), ExitCode::from(23));
}

#[test]
fn clamps_out_of_range_status_and_defaults_missing_to_one() {
    assert_eq!(map_exit_code(false, Some(300)), ExitCode::from(255));
    assert_eq!(map_exit_code(false, Some(-1)), ExitCode::from(255));
    assert_eq!(map_exit_code(false, None), ExitCode::from(1));
}

#[test]
fn immediate_flush_detects_cursor_control_sequences() {
    assert!(requires_immediate_terminal_flush("\rprompt"));
    assert!(requires_immediate_terminal_flush("\x1b[2J"));
    assert!(requires_immediate_terminal_flush("abc\x08"));
    assert!(!requires_immediate_terminal_flush("plain text\nnext line"));
}

#[test]
fn immediate_flush_for_short_highlighted_prompt_chunks() {
    let raw = "router# ";
    let processed = "\x1b[38;2;255;0;0mrouter\x1b[0m# ";
    assert!(should_flush_immediately(raw, processed));
}

#[test]
fn does_not_force_immediate_flush_for_large_highlighted_chunks() {
    let raw = "x".repeat(1024);
    let processed = format!("\x1b[31m{}\x1b[0m", raw);
    assert!(!should_flush_immediately(&raw, &processed));
}

#[test]
fn build_ssh_command_uses_plain_ssh_without_pass_password() {
    let args = vec!["user@host".to_string(), "-p".to_string(), "22".to_string()];
    let command = build_plain_ssh_command(&args);

    assert_eq!(command.program, "ssh");
    assert_eq!(command.args, args);
    assert!(command.env.is_empty());
    assert!(command.stdin_payload.is_none());
    assert!(command.fallback_notice.is_none());
}

#[test]
fn build_ssh_command_leaves_direct_launches_as_plain_ssh() {
    let args = vec!["user@host".to_string()];
    let command = build_plain_ssh_command(&args);

    assert_eq!(command.program, "ssh");
    assert_eq!(command.args, args);
    assert!(command.env.is_empty());
    assert!(command.stdin_payload.is_none());
    assert!(command.fallback_notice.is_none());
}

#[test]
fn resolve_pass_entry_prefers_explicit_override() {
    let mut host = InventoryHost::new("prod".to_string());
    host.vault_pass = Some("shared".to_string());

    let resolved = resolve_pass_entry_from_hosts("prod", Some("override"), &[host]);
    assert_eq!(resolved.as_deref(), Some("override"));
}

#[test]
fn resolve_pass_entry_matches_unique_hostname_when_alias_not_found() {
    let mut host = InventoryHost::new("prod".to_string());
    host.host = "host.example.com".to_string();
    host.vault_pass = Some("shared".to_string());

    let resolved = resolve_pass_entry_from_hosts("host.example.com", None, &[host]);
    assert_eq!(resolved.as_deref(), Some("shared"));
}

#[test]
fn resolve_host_by_destination_prefers_alias_before_hostname() {
    let mut alias_host = InventoryHost::new("prod".to_string());
    alias_host.host = "host.example.com".to_string();

    let mut hostname_host = InventoryHost::new("host.example.com".to_string());
    hostname_host.host = "other.example.com".to_string();

    let hosts = vec![alias_host, hostname_host];
    let resolved = resolve_host_by_destination("host.example.com", &hosts).expect("host");
    assert_eq!(resolved.name, "host.example.com");
}

#[test]
fn synthesize_ssh_args_injects_inventory_defaults() {
    let mut host = InventoryHost::new("switch".to_string());
    host.host = "10.0.0.10".to_string();
    host.user = Some("admin".to_string());
    host.port = Some(2222);
    host.ssh.identity_files.push("~/.ssh/id_rsa".to_string());
    host.ssh.proxy_jump = Some("bastion".to_string());
    host.ssh.identities_only = Some(true);
    host.ssh.local_forward.push("8080 localhost:80".to_string());

    let args = vec!["switch".to_string()];
    let synthesized = synthesize_ssh_args(&args, &host);

    assert_eq!(
        synthesized,
        vec![
            "-l".to_string(),
            "admin".to_string(),
            "-p".to_string(),
            "2222".to_string(),
            "-i".to_string(),
            "~/.ssh/id_rsa".to_string(),
            "-o".to_string(),
            "ProxyJump=bastion".to_string(),
            "-o".to_string(),
            "IdentitiesOnly=yes".to_string(),
            "-L".to_string(),
            "8080:localhost:80".to_string(),
            "10.0.0.10".to_string(),
        ]
    );
}

#[test]
fn synthesize_ssh_args_preserves_repeated_generic_ssh_options() {
    let mut host = InventoryHost::new("jump".to_string());
    host.host = "jump.example".to_string();
    host.ssh.identity_files.push("~/.ssh/id_jump".to_string());
    host.ssh.identity_files.push("~/.ssh/id_ops".to_string());
    host.ssh.forward_agent = Some("$SSH_AUTH_SOCK".to_string());
    host.ssh.extra_options.insert(
        "CertificateFile".to_string(),
        vec!["~/.ssh/id_jump-cert.pub".to_string(), "~/.ssh/id_ops-cert.pub".to_string()],
    );

    let args = vec!["jump".to_string()];
    let synthesized = synthesize_ssh_args(&args, &host);

    assert_eq!(
        synthesized,
        vec![
            "-i".to_string(),
            "~/.ssh/id_jump".to_string(),
            "-i".to_string(),
            "~/.ssh/id_ops".to_string(),
            "-o".to_string(),
            "ForwardAgent=$SSH_AUTH_SOCK".to_string(),
            "-o".to_string(),
            "CertificateFile=~/.ssh/id_jump-cert.pub".to_string(),
            "-o".to_string(),
            "CertificateFile=~/.ssh/id_ops-cert.pub".to_string(),
            "jump.example".to_string(),
        ]
    );
}

#[test]
fn synthesize_ssh_args_preserves_cli_overrides() {
    let mut host = InventoryHost::new("switch".to_string());
    host.host = "10.0.0.10".to_string();
    host.user = Some("admin".to_string());
    host.port = Some(2222);
    host.ssh.proxy_jump = Some("bastion".to_string());

    let args = vec![
        "-l".to_string(),
        "override".to_string(),
        "-p".to_string(),
        "2200".to_string(),
        "-o".to_string(),
        "ProxyJump=direct".to_string(),
        "user@switch".to_string(),
    ];
    let synthesized = synthesize_ssh_args(&args, &host);

    assert_eq!(
        synthesized,
        vec![
            "-l".to_string(),
            "override".to_string(),
            "-p".to_string(),
            "2200".to_string(),
            "-o".to_string(),
            "ProxyJump=direct".to_string(),
            "user@10.0.0.10".to_string(),
        ]
    );
}
