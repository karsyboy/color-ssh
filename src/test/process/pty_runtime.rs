use super::{
    HostScrollbackMirror, InteractiveSshRuntime, collect_host_scrollback_insertions, encode_mouse_event, infer_scrolled_line_count, paint_terminal_view,
    select_interactive_ssh_runtime, take_latest_reload_notice_toast,
};
use crate::config;
use crate::config::{AuthSettings, CompiledHighlightRule, HighlightOverlayAutoPolicy, HighlightOverlayMode, InteractiveProfileSnapshot};
use crate::runtime::format_reload_notice;
use crate::terminal::highlight_overlay::{HighlightOverlay, HighlightOverlayEngine};
use crate::terminal::{MouseProtocolEncoding, MouseProtocolMode, TerminalEngine};
use crate::test::support::state::TestStateGuard;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
};
use regex::{Regex, RegexSet};

fn buffer_lines(buffer: &Buffer) -> Vec<String> {
    let mut lines = Vec::with_capacity(buffer.area.height as usize);
    for row in 0..buffer.area.height {
        let mut line = String::new();
        for col in 0..buffer.area.width {
            line.push_str(buffer[(col, row)].symbol());
        }
        lines.push(line);
    }
    lines
}

fn trim_line(line: &str) -> &str {
    line.trim_end_matches(' ')
}

#[test]
fn select_interactive_ssh_runtime_prefers_pty_only_for_direct_terminals() {
    assert_eq!(select_interactive_ssh_runtime(true), InteractiveSshRuntime::PtyCentered);
    assert_eq!(select_interactive_ssh_runtime(false), InteractiveSshRuntime::CompatibilityPassthrough);
}

#[test]
fn terminal_view_renders_basic_prompt_and_command_output() {
    let mut engine = TerminalEngine::new(4, 40, 128);
    engine.process_output(b"user@host:~$ ");
    engine.process_output(b"echo hi\r\nhi\r\n");

    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 4));
    let viewport = engine.view_model().viewport_snapshot(4, 40);
    let cursor = paint_terminal_view(&mut buffer, Rect::new(0, 0, 40, 4), &viewport, &HighlightOverlay::default(), true);
    let lines = buffer_lines(&buffer);

    assert_eq!(trim_line(&lines[0]), "user@host:~$ echo hi");
    assert_eq!(trim_line(&lines[1]), "hi");
    assert_eq!(cursor, Some((0, 2).into()));
}

#[test]
fn terminal_view_respects_resize_for_subsequent_output() {
    let mut engine = TerminalEngine::new(2, 5, 128);
    engine.process_output(b"abcde");
    engine.resize_surface(3, 10);
    engine.process_output(b"\r\n1234567890");

    let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 3));
    let viewport = engine.view_model().viewport_snapshot(3, 10);
    paint_terminal_view(&mut buffer, Rect::new(0, 0, 10, 3), &viewport, &HighlightOverlay::default(), true);
    let lines = buffer_lines(&buffer);

    assert_eq!(trim_line(&lines[0]), "abcde");
    assert_eq!(trim_line(&lines[1]), "1234567890");
}

#[test]
fn terminal_view_switches_between_primary_and_alternate_screen() {
    let mut engine = TerminalEngine::new(3, 20, 128);
    engine.process_output(b"primary screen");
    engine.process_output(b"\x1b[?1049h\ralternate");

    let mut alt_buffer = Buffer::empty(Rect::new(0, 0, 20, 3));
    let alt_viewport = engine.view_model().viewport_snapshot(3, 20);
    paint_terminal_view(&mut alt_buffer, Rect::new(0, 0, 20, 3), &alt_viewport, &HighlightOverlay::default(), true);
    let alt_lines = buffer_lines(&alt_buffer);
    assert!(trim_line(&alt_lines[0]).starts_with("alternate"));

    engine.process_output(b"\x1b[?1049l");
    let mut primary_buffer = Buffer::empty(Rect::new(0, 0, 20, 3));
    let primary_viewport = engine.view_model().viewport_snapshot(3, 20);
    paint_terminal_view(
        &mut primary_buffer,
        Rect::new(0, 0, 20, 3),
        &primary_viewport,
        &HighlightOverlay::default(),
        true,
    );
    let primary_lines = buffer_lines(&primary_buffer);
    assert!(trim_line(&primary_lines[0]).starts_with("primary screen"));
}

#[test]
fn terminal_view_preserves_background_and_underline_styles() {
    let mut engine = TerminalEngine::new(2, 10, 128);
    engine.process_output(b"\x1b[41;4mA\x1b[0m");

    let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 2));
    let viewport = engine.view_model().viewport_snapshot(2, 10);
    paint_terminal_view(&mut buffer, Rect::new(0, 0, 10, 2), &viewport, &HighlightOverlay::default(), true);

    let cell = &buffer[(0, 0)];
    assert_eq!(cell.bg, Color::Indexed(1));
    assert!(cell.modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn terminal_view_applies_overlay_row_ranges_from_renderer_contract() {
    let mut engine = TerminalEngine::new(1, 20, 32);
    engine.process_output(b"status: error");

    let snapshot = InteractiveProfileSnapshot {
        auth_settings: AuthSettings::default(),
        history_buffer: 32,
        remote_clipboard_write: false,
        remote_clipboard_max_bytes: 4096,
        ssh_logging_enabled: false,
        secret_patterns: Vec::new(),
        overlay_rules: vec![CompiledHighlightRule::new(
            Regex::new("error").expect("overlay regex"),
            "\x1b[38;2;255;0;0m".to_string(),
        )],
        overlay_rule_set: Some(RegexSet::new(["error"]).expect("overlay rule set")),
        overlay_mode: HighlightOverlayMode::Always,
        overlay_auto_policy: HighlightOverlayAutoPolicy::Safe,
        config_version: 0,
    };
    let mut overlay_engine = HighlightOverlayEngine::from_snapshot(&snapshot);

    let render_snapshot = engine.view_model().frontend_snapshot(1, 20);
    let overlay = render_snapshot.build_highlight_overlay(&mut overlay_engine, 1);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 20, 1));

    paint_terminal_view(&mut buffer, Rect::new(0, 0, 20, 1), render_snapshot.viewport(), &overlay, true);

    assert_eq!(buffer[(8, 0)].fg, Color::Rgb(255, 0, 0));
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
fn collect_host_scrollback_insertions_returns_scrolled_primary_rows() {
    let mut engine = TerminalEngine::new(3, 20, 16);
    let mut highlight_overlay = HighlightOverlayEngine::new();
    let mut host_scrollback = HostScrollbackMirror::new(16);

    engine.process_output(b"line1\r\nline2\r\nline3");
    let initial_insertions = collect_host_scrollback_insertions(&engine, &mut highlight_overlay, 1, &mut host_scrollback);
    assert!(initial_insertions.is_empty());

    engine.process_output(b"\r\nline4");
    let insertions = collect_host_scrollback_insertions(&engine, &mut highlight_overlay, 2, &mut host_scrollback);

    assert_eq!(insertions.len(), 1);
    assert_eq!(insertions[0].viewport.size().0, 1);
    assert_eq!(trim_line(&insertions[0].viewport.rows()[0].display_text()), "line1");
}

#[test]
fn collect_host_scrollback_insertions_handles_saturated_history_with_repeated_rows() {
    let mut engine = TerminalEngine::new(2, 20, 2);
    let mut highlight_overlay = HighlightOverlayEngine::new();
    let mut host_scrollback = HostScrollbackMirror::new(2);

    engine.process_output(b"same\r\nsame\r\nsame\r\nsame");
    let initial_insertions = collect_host_scrollback_insertions(&engine, &mut highlight_overlay, 1, &mut host_scrollback);
    assert!(initial_insertions.is_empty());

    engine.process_output(b"\r\nsame");
    let insertions = collect_host_scrollback_insertions(&engine, &mut highlight_overlay, 2, &mut host_scrollback);

    assert_eq!(insertions.len(), 1);
    assert_eq!(insertions[0].viewport.size().0, 1);
    assert_eq!(trim_line(&insertions[0].viewport.rows()[0].display_text()), "same");
}

#[test]
fn collect_host_scrollback_insertions_preserves_existing_display_scrollback() {
    let mut engine = TerminalEngine::new(2, 20, 8);
    let mut highlight_overlay = HighlightOverlayEngine::new();
    let mut host_scrollback = HostScrollbackMirror::new(8);

    engine.process_output(b"line1\r\nline2\r\nline3");
    let initial_insertions = collect_host_scrollback_insertions(&engine, &mut highlight_overlay, 1, &mut host_scrollback);
    assert!(initial_insertions.is_empty());

    engine.process_output(b"\r\nline4");
    engine.set_display_scrollback(1);

    let _ = collect_host_scrollback_insertions(&engine, &mut highlight_overlay, 2, &mut host_scrollback);

    assert_eq!(engine.view_model().frontend_snapshot(2, 20).scrollback().display_offset(), 1);
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
