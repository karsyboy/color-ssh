use super::*;

#[test]
fn overlay_reuses_cached_overlay_when_render_epoch_changes_but_viewport_text_is_stable() {
    let mut terminal_engine = TerminalEngine::new(4, 40, 128);
    terminal_engine.process_output(b"user@host:~$ error\r\n");

    let mut overlay_engine = HighlightOverlayEngine::with_rules(vec![compiled_rule("error", "\x1b[38;2;255;0;0m")], HighlightOverlayMode::Always);

    let _first_overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 1, 0);
    assert_eq!(overlay_engine.profiler.last_build.kind, HighlightOverlayBuildKind::IncrementalAnalysis);
    assert_eq!(overlay_engine.profiler.last_build.analyzed_rows, 1);

    let second_overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 2, 0);

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

    let _first_overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 1, 0);
    assert_eq!(overlay_engine.profiler.last_build.analyzed_rows, 3);

    terminal_engine.process_output(b"\r\ndelta");
    let _second_overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 2, 0);

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

    let first_overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 1, 0);
    assert!(first_overlay.style_for_cell(0, 8).is_some());

    terminal_engine.process_output(b"\rstatus: warn \x1b[K");
    let second_overlay = build_overlay_for_engine(&mut terminal_engine, &mut overlay_engine, 2, 0);

    assert_eq!(overlay_engine.profiler.last_build.kind, HighlightOverlayBuildKind::IncrementalAnalysis);
    assert_eq!(overlay_engine.profiler.last_build.analyzed_rows, 1);
    assert!(second_overlay.style_for_cell(0, 8).is_some());
    assert!(second_overlay.style_for_cell(0, 12).is_none());
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
