use super::{
    HostScrollbackMirror, InteractiveSshRuntime, collect_host_scrollback_insertions, encode_mouse_event, infer_scrolled_line_count, paint_terminal_view,
    select_interactive_ssh_runtime, take_latest_reload_notice_toast,
};
use crate::config;
use crate::reload_notice::format_reload_notice;
use crate::terminal_core::highlight_overlay::{HighlightOverlay, HighlightOverlayEngine};
use crate::terminal_core::{MouseProtocolEncoding, MouseProtocolMode, TerminalEngine};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
};

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
    let initial_insertions = collect_host_scrollback_insertions(&mut engine, &mut highlight_overlay, 1, &mut host_scrollback, 0);
    assert!(initial_insertions.is_empty());

    engine.process_output(b"\r\nline4");
    let insertions = collect_host_scrollback_insertions(&mut engine, &mut highlight_overlay, 2, &mut host_scrollback, 0);

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
    let initial_insertions = collect_host_scrollback_insertions(&mut engine, &mut highlight_overlay, 1, &mut host_scrollback, 0);
    assert!(initial_insertions.is_empty());

    engine.process_output(b"\r\nsame");
    let insertions = collect_host_scrollback_insertions(&mut engine, &mut highlight_overlay, 2, &mut host_scrollback, 0);

    assert_eq!(insertions.len(), 1);
    assert_eq!(insertions[0].viewport.size().0, 1);
    assert_eq!(trim_line(&insertions[0].viewport.rows()[0].display_text()), "same");
}

#[test]
fn format_reload_notice_prefixes_message() {
    assert_eq!(format_reload_notice("Config reloaded successfully"), "[color-ssh] Config reloaded successfully");
}

#[test]
fn take_latest_reload_notice_toast_uses_latest_notice() {
    let _ = config::take_reload_notices();
    config::queue_reload_notice("Config reloaded successfully".to_string());
    config::queue_reload_notice("Config reload failed: parse error at line 77".to_string());

    let toast = take_latest_reload_notice_toast().expect("reload notice toast");

    assert_eq!(toast.message(), "[color-ssh] Config reload failed: parse error at line 77");
    let _ = config::take_reload_notices();
}
