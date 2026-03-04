use super::exit::map_exit_code;
use super::launch::{build_plain_ssh_command, resolve_host_by_destination, resolve_pass_entry_from_hosts, synthesize_ssh_args};
use super::stream::{requires_immediate_terminal_flush, should_flush_immediately};
use crate::inventory::InventoryHost;
use std::process::ExitCode;

fn host_with_alias_and_hostname(alias: &str, hostname: &str) -> InventoryHost {
    let mut host = InventoryHost::new(alias.to_string());
    host.host = hostname.to_string();
    host
}

fn assert_pair(args: &[String], flag: &str, value: &str) {
    assert!(
        args.windows(2).any(|window| window[0] == flag && window[1] == value),
        "missing pair [{flag}, {value}] in args: {args:?}"
    );
}

fn assert_contains(args: &[String], expected: &str) {
    assert!(args.iter().any(|arg| arg == expected), "missing arg '{expected}' in args: {args:?}");
}

#[test]
fn map_exit_code_success_failure_and_missing_status_maps_to_expected_code() {
    let cases = [
        ((true, Some(0)), ExitCode::SUCCESS),
        ((false, Some(23)), ExitCode::from(23)),
        ((false, Some(300)), ExitCode::from(255)),
        ((false, None), ExitCode::from(1)),
    ];

    for ((is_success, status), expected) in cases {
        assert_eq!(map_exit_code(is_success, status), expected);
    }
}

#[test]
fn flush_decisions_prompt_signals_and_colored_prompt_changes_return_expected_booleans() {
    let immediate_flush_cases = [("\rprompt", true), ("plain text", false)];
    for (line, expected) in immediate_flush_cases {
        assert_eq!(requires_immediate_terminal_flush(line), expected);
    }

    let raw_prompt = "router# ";
    let colored_prompt = "\x1b[38;2;255;0;0mrouter\x1b[0m# ";
    let flush_cases = [((raw_prompt, colored_prompt, true), true), ((raw_prompt, raw_prompt, false), false)];
    for ((raw, processed, host_changed), expected) in flush_cases {
        assert_eq!(should_flush_immediately(raw, processed, host_changed), expected);
    }
}

#[test]
fn build_plain_ssh_command_args_preserves_program_args_and_stdin_payload() {
    let args = vec!["user@host".to_string(), "-p".to_string(), "22".to_string()];

    let command = build_plain_ssh_command(&args);

    assert_eq!(command.program, "ssh");
    assert_eq!(command.args, args);
    assert!(command.stdin_payload.is_none());
}

#[test]
fn host_and_pass_entry_resolution_explicit_then_inventory_lookup_follows_precedence_rules() {
    let mut prod_host = host_with_alias_and_hostname("prod", "host.example.com");
    prod_host.vault_pass = Some("shared".to_string());

    let explicit_pass_entry = resolve_pass_entry_from_hosts("prod", Some("override"), &[prod_host.clone()]);
    assert_eq!(explicit_pass_entry.as_deref(), Some("override"));

    let derived_pass_entry = resolve_pass_entry_from_hosts("prod", None, &[prod_host]);
    assert_eq!(derived_pass_entry.as_deref(), Some("shared"));

    let hosts = [
        host_with_alias_and_hostname("prod", "host.example.com"),
        host_with_alias_and_hostname("host.example.com", "other.example.com"),
    ];

    let resolved = resolve_host_by_destination("host.example.com", &hosts).expect("resolve by destination");
    assert_eq!(resolved.name, "host.example.com");
}

#[test]
fn synthesize_ssh_args_inventory_defaults_and_cli_overrides_apply_expected_precedence() {
    let mut host = InventoryHost::new("switch".to_string());
    host.host = "10.0.0.10".to_string();
    host.user = Some("admin".to_string());
    host.port = Some(2222);
    host.ssh.proxy_jump = Some("bastion".to_string());

    let default_args = synthesize_ssh_args(&["switch".to_string()], &host);
    assert_pair(&default_args, "-l", "admin");
    assert_pair(&default_args, "-p", "2222");
    assert_contains(&default_args, "ProxyJump=bastion");

    let cli_args = vec![
        "-l".to_string(),
        "override".to_string(),
        "-o".to_string(),
        "ProxyJump=direct".to_string(),
        "user@switch".to_string(),
    ];
    let overridden_args = synthesize_ssh_args(&cli_args, &host);

    assert_pair(&overridden_args, "-l", "override");
    assert_contains(&overridden_args, "ProxyJump=direct");
    assert_contains(&overridden_args, "user@10.0.0.10");
}
