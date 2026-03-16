use super::{
    DirectRuntimePtySyncDecision, DirectRuntimeResizeDecision, DirectRuntimeViewportState, InteractiveSshRuntime, direct_runtime_exit_cleanup_sequence,
    direct_runtime_inline_viewport_height, direct_runtime_resize_dimensions, direct_runtime_resize_rows, encode_mouse_event, infer_scrolled_line_count,
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
    assert_eq!(direct_runtime_inline_viewport_height(24), 24);
    assert_eq!(direct_runtime_inline_viewport_height(0), 1);
}

#[test]
fn direct_runtime_resize_rows_tracks_terminal_height_with_minimum_one_row() {
    assert_eq!(direct_runtime_resize_rows(24), 24);
    assert_eq!(direct_runtime_resize_rows(1), 1);
    assert_eq!(direct_runtime_resize_rows(0), 1);
}

#[test]
fn direct_runtime_resize_dimensions_clamp_to_valid_pty_size() {
    assert_eq!(direct_runtime_resize_dimensions(120, 24), (120, 24));
    assert_eq!(direct_runtime_resize_dimensions(0, 24), (1, 24));
    assert_eq!(direct_runtime_resize_dimensions(120, 0), (120, 1));
}

#[test]
fn direct_runtime_viewport_state_noops_when_terminal_size_is_unchanged() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(state.observe_terminal_size(120, 24), DirectRuntimeResizeDecision::Noop);
}

#[test]
fn direct_runtime_viewport_state_shrink_requests_redraw_without_rebuild() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(state.observe_terminal_size(100, 20), DirectRuntimeResizeDecision::Redraw);
    assert_eq!(state.inline_viewport_height(), 24);
}

#[test]
fn direct_runtime_viewport_state_growth_above_inline_cap_triggers_rebuild() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(
        state.observe_terminal_size(120, 40),
        DirectRuntimeResizeDecision::RebuildTerminal { inline_viewport_height: 40 }
    );
    assert_eq!(state.inline_viewport_height(), 40);
}

#[test]
fn direct_runtime_viewport_state_width_only_change_does_not_rebuild() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(state.observe_terminal_size(140, 24), DirectRuntimeResizeDecision::Redraw);
    assert_eq!(state.inline_viewport_height(), 24);
}

#[test]
fn direct_runtime_viewport_state_shrink_sync_uses_drawn_viewport_size() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(
        state.record_drawn_viewport(Rect::new(0, 0, 80, 20)),
        DirectRuntimePtySyncDecision::ResizePty { cols: 80, rows: 20 }
    );
}

#[test]
fn direct_runtime_viewport_state_duplicate_drawn_viewport_does_not_resend_pty_resize() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(state.record_drawn_viewport(Rect::new(0, 0, 120, 24)), DirectRuntimePtySyncDecision::Noop);
    assert_eq!(
        state.record_drawn_viewport(Rect::new(0, 0, 90, 18)),
        DirectRuntimePtySyncDecision::ResizePty { cols: 90, rows: 18 }
    );
    assert_eq!(state.record_drawn_viewport(Rect::new(0, 0, 90, 18)), DirectRuntimePtySyncDecision::Noop);
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
