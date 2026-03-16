use super::{
    InteractiveSshRuntime, direct_runtime_exit_cleanup_sequence, direct_runtime_inline_viewport_height, encode_mouse_event, infer_scrolled_line_count,
    select_interactive_ssh_runtime, take_latest_reload_notice_toast,
};
use crate::config;
use crate::runtime::format_reload_notice;
use crate::terminal::{MouseProtocolEncoding, MouseProtocolMode};
use crate::test::support::state::TestStateGuard;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

#[test]
fn select_interactive_ssh_runtime_prefers_pty_only_for_direct_terminals() {
    assert_eq!(select_interactive_ssh_runtime(true), InteractiveSshRuntime::PtyCentered);
    assert_eq!(select_interactive_ssh_runtime(false), InteractiveSshRuntime::CompatibilityPassthrough);
}

#[test]
fn direct_runtime_inline_viewport_uses_full_height_with_minimum_one_row() {
    assert_eq!(direct_runtime_inline_viewport_height(24, Some(0)), 24);
    assert_eq!(direct_runtime_inline_viewport_height(24, Some(12)), 12);
    assert_eq!(direct_runtime_inline_viewport_height(24, Some(23)), 1);
    assert_eq!(direct_runtime_inline_viewport_height(24, Some(50)), 1);
    assert_eq!(direct_runtime_inline_viewport_height(24, None), 24);
    assert_eq!(direct_runtime_inline_viewport_height(0, None), 1);
}

#[test]
fn direct_runtime_exit_cleanup_sequence_resets_and_appends_newline() {
    assert_eq!(direct_runtime_exit_cleanup_sequence(), "\x1b[0m\r\n");
}

#[test]
fn mouse_event_encoding_uses_sgr_release_suffix_for_release_events() {
    let mouse = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 1,
        row: 1,
        modifiers: KeyModifiers::empty(),
    };

    let bytes = encode_mouse_event(mouse, Rect::new(0, 0, 20, 5), MouseProtocolMode::ButtonMotion, MouseProtocolEncoding::Sgr).expect("encoded mouse release");

    assert_eq!(String::from_utf8(bytes).expect("utf8 mouse release"), "\x1b[<0;2;2m");
}

#[test]
fn mouse_event_encoding_ignores_events_outside_viewport() {
    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 30,
        row: 10,
        modifiers: KeyModifiers::empty(),
    };

    assert!(encode_mouse_event(mouse, Rect::new(0, 0, 20, 5), MouseProtocolMode::Press, MouseProtocolEncoding::Default).is_none());
}

#[test]
fn infer_scrolled_line_count_detects_simple_terminal_scroll() {
    let previous = vec!["line1".to_string(), "line2".to_string(), "line3".to_string()];
    let current = vec!["line2".to_string(), "line3".to_string(), "line4".to_string()];

    assert_eq!(infer_scrolled_line_count(&previous, &current), 1);
}

#[test]
fn format_reload_notice_prefixes_message() {
    assert_eq!(format_reload_notice("Config reloaded successfully"), "[color-ssh] Config reloaded successfully");
}

#[test]
fn take_latest_reload_notice_toast_uses_latest_notice() {
    let _state = TestStateGuard::lock();
    config::queue_reload_notice("Config reloaded successfully".to_string());
    config::queue_reload_notice("Config reload failed: parse error at line 77".to_string());

    let toast = take_latest_reload_notice_toast().expect("reload notice toast");

    assert_eq!(toast.message(), "[color-ssh] Config reload failed: parse error at line 77");
}
