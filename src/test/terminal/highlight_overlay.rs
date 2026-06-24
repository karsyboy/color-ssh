use super::{HighlightCellRange, HighlightOverlay, HighlightOverlayEngine, build_overlay_styles, byte_range_to_cell_columns};
use crate::config::{CompiledHighlightRule, HighlightOverlayAutoPolicy, HighlightOverlayMode};
use crate::terminal::{TerminalEngine, TerminalHostCallbacks};
use regex::Regex;

fn overlay_engine(patterns: &[&str]) -> HighlightOverlayEngine {
    let rules = patterns
        .iter()
        .map(|pattern| CompiledHighlightRule::new(Regex::new(pattern).expect("test regex compiles"), "\x1b[38;2;255;0;0m".to_string()))
        .collect::<Vec<_>>();
    let (styles, rule_style_indexes) = build_overlay_styles(&rules);

    HighlightOverlayEngine {
        rules,
        styles,
        rule_style_indexes,
        mode: HighlightOverlayMode::Always,
        auto_policy: HighlightOverlayAutoPolicy::Safe,
        refresh_from_current_config: false,
        ..HighlightOverlayEngine::default()
    }
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
