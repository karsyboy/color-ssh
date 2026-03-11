use super::*;

#[test]
fn overlay_highlights_visible_rows_with_renderer_side_styles() {
    let mut terminal_engine = TerminalEngine::new(4, 40, 128);
    terminal_engine.process_output(b"user@host:~$ error\r\n");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0;48;2;12;12;12m")], HighlightOverlayMode::Always);

    let overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 1, 0);

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
fn overlay_exposes_row_ranges_and_style_table_for_gui_renderers() {
    let mut terminal_engine = TerminalEngine::new(3, 32, 128);
    terminal_engine.process_output(b"status: error\r\n");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Always);
    let overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 1, 0);

    let view = terminal_engine.view_model();
    let (highlight_row, visible_text) = view
        .visible_row_texts()
        .into_iter()
        .find(|(_, text)| text.contains("error"))
        .expect("visible row with highlight target");
    let highlight_col = visible_text.find("error").expect("highlight column") as u16;
    let ranges = overlay.ranges_for_row(highlight_row).expect("row highlight ranges");

    assert_eq!(overlay.suppression_reason(), None);
    assert!(!overlay.styles().is_empty());
    assert!(ranges.iter().any(|range| highlight_col >= range.start_col() && highlight_col < range.end_col()));
    let style = &overlay.styles()[ranges[0].style_index()];
    assert!(matches!(style.fg_color(), Some(AnsiColor::Spec(Rgb { r: 255, g: 0, b: 0 }))));
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

    let overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 1, 0);

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

    let overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 2, 0);

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
