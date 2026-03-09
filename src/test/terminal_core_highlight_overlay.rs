use super::{
    HighlightCompatibilityAction, HighlightOverlayBuildKind, HighlightOverlayContext, HighlightOverlayEngine, HighlightOverlayViewport,
    HighlightSuppressionReason, viewport_changed_aggressively,
};
use crate::config::{HighlightOverlayAutoPolicy, HighlightOverlayMode, InteractiveProfileSnapshot};
use crate::highlight_rules::CompiledHighlightRule;
use crate::terminal_core::{AnsiColor, TerminalEngine};
use alacritty_terminal::vte::ansi::Rgb;
use regex::Regex;

fn compiled_rule(pattern: &str, style: &str) -> CompiledHighlightRule {
    CompiledHighlightRule::new(Regex::new(pattern).expect("regex"), style.to_string())
}

fn build_overlay_for_engine(
    terminal_engine: &TerminalEngine,
    overlay_engine: &mut HighlightOverlayEngine,
    render_epoch: u64,
    display_scrollback: usize,
) -> super::HighlightOverlay {
    let view = terminal_engine.view_model();
    let (rows, cols) = view.size();
    let viewport = view.viewport_snapshot(rows, cols);
    let overlay_view = HighlightOverlayViewport::new(&viewport, view.is_alternate_screen(), view.mouse_protocol().0, view.cursor_hidden());
    overlay_engine.build_visible_overlay(
        &overlay_view,
        HighlightOverlayContext {
            render_epoch,
            display_scrollback,
        },
    )
}

fn render_dense_lines(terminal_engine: &mut TerminalEngine, line_count: usize, prefix: &str) {
    let mut bytes = String::new();
    for row_idx in 0..line_count {
        if row_idx > 0 {
            bytes.push_str("\r\n");
        }
        bytes.push_str(&format!("{prefix}{row_idx:02} error error"));
    }
    terminal_engine.process_output(bytes.as_bytes());
}

#[test]
fn overlay_highlights_visible_rows_with_renderer_side_styles() {
    let mut terminal_engine = TerminalEngine::new(4, 40, 128);
    terminal_engine.process_output(b"user@host:~$ error\r\n");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0;48;2;12;12;12m")], HighlightOverlayMode::Always);

    let overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 1, 0);

    assert_eq!(overlay.suppression_reason, None);
    let view = terminal_engine.view_model();
    let visible_rows = view.visible_row_texts();
    let (highlight_row, visible_text) = visible_rows
        .iter()
        .find(|(_, text)| text.contains("error"))
        .expect("visible row with highlight target");
    let highlight_col = visible_text.find("error").expect("highlight column") as u16;

    assert!(overlay.style_for_cell(*highlight_row, 0).is_none());

    let style = overlay.style_for_cell(*highlight_row, highlight_col).expect("highlight style");
    assert!(matches!(style.fg_color(), Some(AnsiColor::Spec(Rgb { r: 255, g: 0, b: 0 }))));
    assert!(matches!(style.bg_color(), Some(AnsiColor::Spec(Rgb { r: 12, g: 12, b: 12 }))));

    assert!(visible_text.starts_with("user@host:~$ error"));
}

#[test]
fn overlay_from_profile_snapshot_uses_snapshot_rules() {
    let mut terminal_engine = TerminalEngine::new(3, 32, 128);
    terminal_engine.process_output(b"status: warn\r\n");

    let snapshot = InteractiveProfileSnapshot {
        auth_settings: crate::config::AuthSettings::default(),
        history_buffer: 128,
        remote_clipboard_write: false,
        remote_clipboard_max_bytes: 4096,
        ssh_logging_enabled: false,
        secret_patterns: Vec::new(),
        overlay_rules: vec![compiled_rule("warn", "\x1b[38;2;255;200;0m")],
        overlay_rule_set: Some(regex::RegexSet::new(["warn"]).expect("rule set")),
        overlay_mode: HighlightOverlayMode::Always,
        overlay_auto_policy: HighlightOverlayAutoPolicy::Safe,
        config_version: 0,
    };
    let mut overlay_engine = HighlightOverlayEngine::from_snapshot(&snapshot);

    let overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 1, 0);

    assert_eq!(overlay.suppression_reason, None);
    let view = terminal_engine.view_model();
    let visible_rows = view.visible_row_texts();
    let (highlight_row, visible_text) = visible_rows
        .iter()
        .find(|(_, text)| text.contains("warn"))
        .expect("visible row with snapshot rule target");
    let highlight_col = visible_text.find("warn").expect("highlight column") as u16;
    assert!(overlay.style_for_cell(*highlight_row, highlight_col).is_some());
}

#[test]
fn overlay_auto_mode_preserves_log_stream_highlighting() {
    let mut terminal_engine = TerminalEngine::new(6, 24, 128);
    render_dense_lines(&mut terminal_engine, 6, "log");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Auto);

    let overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 2, 0);

    assert_eq!(overlay.suppression_reason, None);
    let view = terminal_engine.view_model();
    let visible_rows = view.visible_row_texts();
    let (highlight_row, visible_text) = visible_rows
        .iter()
        .find(|(_, text)| text.contains("error"))
        .expect("visible log row with highlight target");
    let highlight_col = visible_text.find("error").expect("highlight column") as u16;
    assert!(overlay.style_for_cell(*highlight_row, highlight_col).is_some());
}

#[test]
fn overlay_suppresses_less_like_alternate_screen_sessions_in_auto_mode() {
    let mut terminal_engine = TerminalEngine::new(3, 20, 128);
    terminal_engine.process_output(b"primary screen");
    terminal_engine.process_output(b"\x1b[?1049h\rLESS error");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Auto);

    let overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 2, 0);

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::AlternateScreen));
    assert!(overlay.style_for_cell(0, 0).is_none());
}

#[test]
fn overlay_suppresses_vim_like_alternate_screen_sessions_in_auto_mode() {
    let mut terminal_engine = TerminalEngine::new(4, 24, 128);
    terminal_engine.process_output(b"\x1b[?1049h\x1b[?25lfile.rs\r\nerror line\r\n~");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Auto);

    let overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 3, 0);

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::AlternateScreen));
}

#[test]
fn overlay_suppresses_htop_like_mouse_reporting_sessions_in_auto_mode() {
    let mut terminal_engine = TerminalEngine::new(4, 24, 128);
    terminal_engine.process_output(b"\x1b[?1000h\x1b[?25lCPU error\r\nMEM error");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Auto);

    let overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 4, 0);

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::MouseReporting));
}

#[test]
fn overlay_reuses_cached_overlay_when_render_epoch_changes_but_viewport_text_is_stable() {
    let mut terminal_engine = TerminalEngine::new(4, 40, 128);
    terminal_engine.process_output(b"user@host:~$ error\r\n");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Always);

    let _first_overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 1, 0);
    assert_eq!(overlay_engine.profiler.last_build.kind, HighlightOverlayBuildKind::IncrementalAnalysis);
    assert_eq!(overlay_engine.profiler.last_build.analyzed_rows, 1);

    let second_overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 2, 0);

    assert_eq!(overlay_engine.profiler.last_build.kind, HighlightOverlayBuildKind::SnapshotReuse);
    assert_eq!(overlay_engine.profiler.last_build.compatibility_action, HighlightCompatibilityAction::Full);
    assert_eq!(overlay_engine.profiler.last_build.analyzed_rows, 0);
    assert_eq!(overlay_engine.profiler.last_build.row_cache_hits, 0);
    assert_eq!(overlay_engine.profiler.snapshot_reuse_count, 1);
    assert!(second_overlay.style_for_cell(0, 0).is_none());
}

#[test]
fn overlay_only_reanalyzes_newly_visible_rows_after_scroll() {
    let mut terminal_engine = TerminalEngine::new(3, 16, 128);
    terminal_engine.process_output(b"alpha\r\nbravo\r\ncharlie");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(
        vec![compiled_rule("alpha|bravo|charlie|delta", "\x1b[38;2;255;0;0m")],
        HighlightOverlayMode::Always,
    );

    let _first_overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 1, 0);
    assert_eq!(overlay_engine.profiler.last_build.analyzed_rows, 3);

    terminal_engine.process_output(b"\r\ndelta");
    let _second_overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 2, 0);

    assert_eq!(overlay_engine.profiler.last_build.kind, HighlightOverlayBuildKind::IncrementalAnalysis);
    assert_eq!(overlay_engine.profiler.last_build.analyzed_rows, 1);
    assert_eq!(overlay_engine.profiler.last_build.row_cache_hits, 2);
    assert_eq!(overlay_engine.profiler.last_build.row_cache_misses, 1);
}

#[test]
fn overlay_reanalyzes_mutated_prompt_line_without_reprocessing_unchanged_rows() {
    let mut terminal_engine = TerminalEngine::new(2, 24, 128);
    terminal_engine.process_output(b"status: error");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error|warn", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Always);

    let first_overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 1, 0);
    assert!(first_overlay.style_for_cell(0, 8).is_some());

    terminal_engine.process_output(b"\rstatus: warn \x1b[K");
    let second_overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 2, 0);

    assert_eq!(overlay_engine.profiler.last_build.kind, HighlightOverlayBuildKind::IncrementalAnalysis);
    assert_eq!(overlay_engine.profiler.last_build.analyzed_rows, 1);
    assert!(second_overlay.style_for_cell(0, 8).is_some());
    assert!(second_overlay.style_for_cell(0, 12).is_none());
}

#[test]
fn overlay_safe_policy_suppresses_primary_screen_fullscreen_viewports() {
    let mut terminal_engine = TerminalEngine::new(6, 24, 128);
    terminal_engine.process_output(b"\x1b[?25l");
    render_dense_lines(&mut terminal_engine, 6, "pane");

    let mut overlay_engine = HighlightOverlayEngine::with_rules_and_policy(
        vec![compiled_rule("error", "\x1b[38;2;255;0;0m")],
        HighlightOverlayMode::Auto,
        HighlightOverlayAutoPolicy::Safe,
    );

    let overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 5, 0);

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::PrimaryScreenFullscreen));
    assert_eq!(
        overlay_engine.profiler.last_build.compatibility_action,
        HighlightCompatibilityAction::Disable(HighlightSuppressionReason::PrimaryScreenFullscreen)
    );
}

#[test]
fn overlay_reduced_policy_limits_primary_screen_fullscreen_viewports_to_trailing_rows() {
    let mut terminal_engine = TerminalEngine::new(6, 24, 128);
    terminal_engine.process_output(b"\x1b[?25l");
    render_dense_lines(&mut terminal_engine, 6, "pane");

    let mut overlay_engine = HighlightOverlayEngine::with_rules_and_policy(
        vec![compiled_rule("error", "\x1b[38;2;255;0;0m")],
        HighlightOverlayMode::Auto,
        HighlightOverlayAutoPolicy::Reduced,
    );

    let overlay = build_overlay_for_engine(&terminal_engine, &mut overlay_engine, 6, 0);
    let view = terminal_engine.view_model();
    let visible_rows = view.visible_row_texts();
    let first_highlight_col = visible_rows[0].1.find("error").expect("first highlight") as u16;
    let last_highlight_col = visible_rows.last().and_then(|(_, text)| text.find("error")).expect("last highlight") as u16;

    assert_eq!(overlay.suppression_reason, None);
    assert_eq!(
        overlay_engine.profiler.last_build.compatibility_action,
        HighlightCompatibilityAction::ReduceToTrailingRows(4)
    );
    assert!(overlay.style_for_cell(visible_rows[0].0, first_highlight_col).is_none());
    assert!(overlay.style_for_cell(visible_rows[5].0, last_highlight_col).is_some());
}

#[test]
fn overlay_repaint_heuristic_detects_large_viewport_churn() {
    let previous_rows = vec![
        (0, "alpha".to_string()),
        (1, "bravo".to_string()),
        (2, "charlie".to_string()),
        (3, "delta".to_string()),
        (4, "echo".to_string()),
        (5, "foxtrot".to_string()),
    ];
    let visible_rows = vec![
        (0, "one".to_string()),
        (1, "two".to_string()),
        (2, "three".to_string()),
        (3, "four".to_string()),
        (4, "five".to_string()),
        (5, "foxtrot".to_string()),
    ];

    assert!(viewport_changed_aggressively(&previous_rows, &visible_rows));
}
