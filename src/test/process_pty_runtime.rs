use super::{InteractiveSshRuntime, paint_terminal_view, select_interactive_ssh_runtime};
use crate::terminal_core::TerminalEngine;
use ratatui::{buffer::Buffer, layout::Rect};

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
    assert_eq!(select_interactive_ssh_runtime(false, false, true), InteractiveSshRuntime::PtyCentered);
    assert_eq!(select_interactive_ssh_runtime(true, false, true), InteractiveSshRuntime::LegacyStream);
    assert_eq!(select_interactive_ssh_runtime(false, true, true), InteractiveSshRuntime::LegacyStream);
    assert_eq!(select_interactive_ssh_runtime(false, false, false), InteractiveSshRuntime::LegacyStream);
}

#[test]
fn terminal_view_renders_basic_prompt_and_command_output() {
    let mut engine = TerminalEngine::new(4, 40, 128);
    engine.process_output(b"user@host:~$ ");
    engine.process_output(b"echo hi\r\nhi\r\n");

    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 4));
    let viewport = engine.view_model().viewport_snapshot(4, 40);
    let cursor = paint_terminal_view(&mut buffer, Rect::new(0, 0, 40, 4), &viewport, true);
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
    paint_terminal_view(&mut buffer, Rect::new(0, 0, 10, 3), &viewport, true);
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
    paint_terminal_view(&mut alt_buffer, Rect::new(0, 0, 20, 3), &alt_viewport, true);
    let alt_lines = buffer_lines(&alt_buffer);
    assert_eq!(trim_line(&alt_lines[0]), "alternate");

    engine.process_output(b"\x1b[?1049l");
    let mut primary_buffer = Buffer::empty(Rect::new(0, 0, 20, 3));
    let primary_viewport = engine.view_model().viewport_snapshot(3, 20);
    paint_terminal_view(&mut primary_buffer, Rect::new(0, 0, 20, 3), &primary_viewport, true);
    let primary_lines = buffer_lines(&primary_buffer);
    assert_eq!(trim_line(&primary_lines[0]), "primary screen");
}
