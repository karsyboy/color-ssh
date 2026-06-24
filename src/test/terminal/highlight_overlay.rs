use super::{HighlightCellRange, HighlightOverlay, HighlightOverlayEngine, HighlightSuppressionReason, build_overlay_styles, byte_range_to_cell_columns};
use crate::config::{CompiledHighlightRule, HighlightOverlayAutoPolicy, HighlightOverlayMode};
use crate::terminal::{MouseProtocolEncoding, MouseProtocolMode, TerminalEngine, TerminalHostCallbacks};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use regex::Regex;
use std::io::{self, Read};

fn overlay_engine(patterns: &[&str]) -> HighlightOverlayEngine {
    overlay_engine_with_mode(patterns, HighlightOverlayMode::Always)
}

fn overlay_engine_with_mode(patterns: &[&str], mode: HighlightOverlayMode) -> HighlightOverlayEngine {
    let rules = patterns
        .iter()
        .map(|pattern| CompiledHighlightRule::new(Regex::new(pattern).expect("test regex compiles"), "\x1b[38;2;255;0;0m".to_string()))
        .collect::<Vec<_>>();
    let (styles, rule_style_indexes) = build_overlay_styles(&rules);

    HighlightOverlayEngine {
        rules,
        styles,
        rule_style_indexes,
        mode,
        auto_policy: HighlightOverlayAutoPolicy::Safe,
        refresh_from_current_config: false,
        ..HighlightOverlayEngine::default()
    }
}

fn pty_output_from_shell(script: &str) -> Vec<u8> {
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows: 6,
            cols: 40,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("open test pty");

    let mut command = CommandBuilder::new("/bin/sh");
    command.arg("-c");
    command.arg(script);

    let mut child = pty_pair.slave.spawn_command(command).expect("spawn test shell in pty");
    drop(pty_pair.slave);

    let mut reader = pty_pair.master.try_clone_reader().expect("clone pty reader");
    let output = read_pty_to_end(&mut reader);
    let status = child.wait().expect("wait for test shell");
    assert!(status.success(), "test shell exited unsuccessfully: {:?}", status);

    output
}

fn read_pty_to_end(reader: &mut Box<dyn Read + Send>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buf = [0u8; 1024];

    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(bytes_read) => output.extend_from_slice(&buf[..bytes_read]),
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) if err.raw_os_error() == Some(nix::libc::EIO) => break,
            Err(err) => panic!("read test pty output: {err}"),
        }
    }

    output
}

#[test]
fn overlay_collects_multiple_rules_on_same_line() {
    let engine = overlay_engine(&["error", "ok"]);

    let ranges = engine.analyze_row_ranges("ok error ok");

    assert_eq!(
        ranges.as_ref(),
        &[
            HighlightCellRange {
                start_col: 0,
                end_col: 2,
                style_index: 1,
            },
            HighlightCellRange {
                start_col: 3,
                end_col: 8,
                style_index: 0,
            },
            HighlightCellRange {
                start_col: 9,
                end_col: 11,
                style_index: 1,
            },
        ]
    );
}

#[test]
fn overlay_prefers_earlier_rule_for_same_start_overlap() {
    let engine = overlay_engine(&["error", "err"]);

    let ranges = engine.analyze_row_ranges("error");

    assert_eq!(
        ranges.as_ref(),
        &[HighlightCellRange {
            start_col: 0,
            end_col: 5,
            style_index: 0,
        }]
    );
}

#[test]
fn byte_ranges_map_to_terminal_columns_for_wide_cells() {
    let text = "a界b";
    let start = text.find('界').expect("wide char present");
    let end = start + '界'.len_utf8();

    assert_eq!(byte_range_to_cell_columns(text, start, end), (1, 3));
}

#[test]
fn overlay_matches_emulated_text_not_ansi_escape_bytes() {
    let mut terminal = TerminalEngine::new_with_host_and_remote_clipboard_policy(1, 20, 10, TerminalHostCallbacks::default(), false, 4096);
    terminal.process_output(b"\x1b[31merror\x1b[0m ok");
    let snapshot = terminal.view_model().frontend_snapshot_at_scrollback(1, 20, 0);
    let mut engine = overlay_engine(&["31", "error"]);

    let overlay: HighlightOverlay = snapshot.build_highlight_overlay(&mut engine, 1);

    assert_eq!(
        overlay.ranges_for_row(0).expect("error range"),
        &[HighlightCellRange {
            start_col: 0,
            end_col: 5,
            style_index: 1,
        }]
    );
}

#[test]
fn auto_overlay_suppresses_real_pty_alternate_screen_tui_with_mouse_reporting() {
    let output = pty_output_from_shell("printf '\\033[?1049h\\033[?1002h\\033[?1006herror'");
    let mut terminal = TerminalEngine::new_with_host_and_remote_clipboard_policy(6, 40, 10, TerminalHostCallbacks::default(), false, 4096);
    terminal.process_output(&output);

    let view = terminal.view_model();
    assert!(view.is_alternate_screen());
    assert_eq!(view.mouse_protocol(), (MouseProtocolMode::ButtonMotion, MouseProtocolEncoding::Sgr));
    let snapshot = view.frontend_snapshot_at_scrollback(6, 40, 0);
    let mut engine = overlay_engine_with_mode(&["error"], HighlightOverlayMode::Auto);

    let overlay = snapshot.build_highlight_overlay(&mut engine, 1);

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::AlternateScreen));
    assert!(overlay.ranges_for_row(0).is_none());
}

#[test]
fn auto_overlay_suppresses_primary_screen_mouse_reporting() {
    let mut terminal = TerminalEngine::new_with_host_and_remote_clipboard_policy(1, 20, 10, TerminalHostCallbacks::default(), false, 4096);
    terminal.process_output(b"\x1b[?1002h\x1b[?1006herror");
    let view = terminal.view_model();
    assert!(!view.is_alternate_screen());
    assert_eq!(view.mouse_protocol(), (MouseProtocolMode::ButtonMotion, MouseProtocolEncoding::Sgr));
    let snapshot = view.frontend_snapshot_at_scrollback(1, 20, 0);
    let mut engine = overlay_engine_with_mode(&["error"], HighlightOverlayMode::Auto);

    let overlay = snapshot.build_highlight_overlay(&mut engine, 1);

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::MouseReporting));
    assert!(overlay.ranges_for_row(0).is_none());
}
