use super::{build_ssh_command, map_exit_code, requires_immediate_terminal_flush, should_flush_immediately};
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
    let command = build_ssh_command(&args, None);

    assert_eq!(command.program, "ssh");
    assert_eq!(command.args, args);
    assert!(command.env.is_empty());
}

#[test]
fn build_ssh_command_wraps_with_password_injection_tool_when_password_present() {
    let args = vec!["user@host".to_string()];
    let command = build_ssh_command(&args, Some("top-secret"));

    assert_eq!(command.program, "sshpass");
    assert_eq!(command.args, vec!["-e".to_string(), "ssh".to_string(), "user@host".to_string()]);
    assert_eq!(command.env, vec![("SSHPASS".to_string(), "top-secret".to_string())]);
}
