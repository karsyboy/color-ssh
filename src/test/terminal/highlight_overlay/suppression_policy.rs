use super::*;

#[test]
fn overlay_suppresses_less_like_alternate_screen_sessions_in_auto_mode() {
    let mut terminal_engine = TerminalEngine::new(3, 20, 128);
    terminal_engine.process_output(b"primary screen");
    terminal_engine.process_output(b"\x1b[?1049h\rLESS error");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Auto);

    let overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 2, 0);

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::AlternateScreen));
    assert!(overlay.style_for_cell(0, 0).is_none());
}

#[test]
fn overlay_suppresses_vim_like_alternate_screen_sessions_in_auto_mode() {
    let mut terminal_engine = TerminalEngine::new(4, 24, 128);
    terminal_engine.process_output(b"\x1b[?1049h\x1b[?25lfile.rs\r\nerror line\r\n~");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Auto);

    let overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 3, 0);

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::AlternateScreen));
}

#[test]
fn overlay_suppresses_htop_like_mouse_reporting_sessions_in_auto_mode() {
    let mut terminal_engine = TerminalEngine::new(4, 24, 128);
    terminal_engine.process_output(b"\x1b[?1000h\x1b[?25lCPU error\r\nMEM error");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Auto);

    let overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 4, 0);

    assert_eq!(overlay.suppression_reason, Some(HighlightSuppressionReason::MouseReporting));
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

    let overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 5, 0);

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

    let overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 6, 0);
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
