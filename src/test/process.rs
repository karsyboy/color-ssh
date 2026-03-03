use super::command::{build_plain_ssh_command, resolve_pass_entry_from_hosts};
use super::map_exit_code;
use super::stream::{requires_immediate_terminal_flush, should_flush_immediately};
use crate::ssh_config::SshHost;
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
    assert!(command.fallback_notice.is_none());
}

#[test]
fn build_ssh_command_leaves_direct_launches_as_plain_ssh() {
    let args = vec!["user@host".to_string()];
    let command = build_plain_ssh_command(&args);

    assert_eq!(command.program, "ssh");
    assert_eq!(command.args, args);
    assert!(command.env.is_empty());
    assert!(command.fallback_notice.is_none());
}

#[test]
fn resolve_pass_entry_prefers_explicit_override() {
    let mut host = SshHost::new("prod".to_string());
    host.pass_key = Some("shared".to_string());

    let resolved = resolve_pass_entry_from_hosts("prod", Some("override"), &[host]);
    assert_eq!(resolved.as_deref(), Some("override"));
}

#[test]
fn resolve_pass_entry_matches_unique_hostname_when_alias_not_found() {
    let mut host = SshHost::new("prod".to_string());
    host.hostname = Some("host.example.com".to_string());
    host.pass_key = Some("shared".to_string());

    let resolved = resolve_pass_entry_from_hosts("host.example.com", None, &[host]);
    assert_eq!(resolved.as_deref(), Some("shared"));
}
