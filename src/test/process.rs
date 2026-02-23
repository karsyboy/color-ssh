use super::{map_exit_code, requires_immediate_terminal_flush, should_flush_immediately};
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
