use super::{HighlightOverlayContext, HighlightOverlayEngine, HighlightSuppressionReason, viewport_changed_aggressively};
use crate::config::HighlightOverlayMode;
use crate::highlighter::CompiledHighlightRule;
use crate::terminal_core::{AnsiColor, TerminalEngine};
use alacritty_terminal::vte::ansi::Rgb;
use regex::Regex;

fn compiled_rule(pattern: &str, style: &str) -> CompiledHighlightRule {
    CompiledHighlightRule::new(Regex::new(pattern).expect("regex"), style.to_string())
}

#[test]
fn overlay_highlights_visible_rows_with_renderer_side_styles() {
    let mut terminal_engine = TerminalEngine::new(4, 40, 128);
    terminal_engine.process_output(b"user@host:~$ error\r\n");

    let view = terminal_engine.view_model();
    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0;48;2;12;12;12m")], HighlightOverlayMode::Always);

    let overlay = overlay_engine.build_visible_overlay(
        &view,
        HighlightOverlayContext {
            render_epoch: 1,
            display_scrollback: 0,
        },
    );

    assert_eq!(overlay.suppression_reason, None);
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
fn overlay_suppresses_highlighting_for_alternate_screen_apps_in_auto_mode() {
    let mut terminal_engine = TerminalEngine::new(3, 20, 128);
    terminal_engine.process_output(b"primary screen");
    terminal_engine.process_output(b"\x1b[?1049h\ralternate");

    let view = terminal_engine.view_model();
    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("alternate", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Auto);

    let overlay = overlay_engine.build_visible_overlay(
        &view,
        HighlightOverlayContext {
            render_epoch: 2,
            display_scrollback: 0,
        },
    );

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::AlternateScreen));
    assert!(overlay.style_for_cell(0, 0).is_none());
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
