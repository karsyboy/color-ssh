use super::{TerminalEngine, TerminalSelection};

fn trim_row(text: &str) -> &str {
    text.trim_end_matches(' ')
}

#[test]
fn terminal_cell_tab_character_renders_as_space_without_contents() {
    let mut engine = TerminalEngine::new(2, 16, 100);
    engine.process_output(b"A\tB");

    let view = engine.view_model();
    let tab_cell = view.cell(0, 1).expect("tab cell");

    assert!(!tab_cell.has_contents());
    assert_eq!(tab_cell.contents(), " ");

    let mut scratch = String::from("seed");
    assert_eq!(tab_cell.symbol(&mut scratch), " ");
}

#[test]
fn terminal_cell_plain_ascii_renders_normally() {
    let mut engine = TerminalEngine::new(2, 16, 100);
    engine.process_output(b"Z");

    let view = engine.view_model();
    let cell = view.cell(0, 0).expect("cell");

    assert!(cell.has_contents());
    assert_eq!(cell.contents(), "Z");

    let mut scratch = String::new();
    assert_eq!(cell.symbol(&mut scratch), "Z");
}

#[test]
fn terminal_cell_symbol_combining_mark_is_included_in_rendered_symbol() {
    let mut engine = TerminalEngine::new(2, 16, 100);
    engine.process_output("e\u{0301}".as_bytes());

    let view = engine.view_model();
    let cell = view.cell(0, 0).expect("cell");

    let mut scratch = String::new();
    assert_eq!(cell.symbol(&mut scratch), "e\u{0301}");
}

#[test]
fn terminal_viewport_snapshot_preserves_rows_glyphs_and_cursor_state() {
    let mut engine = TerminalEngine::new(3, 8, 100);
    engine.process_output("e\u{0301}".as_bytes());
    engine.process_output(b"\r\nok");

    let viewport = engine.view_model().viewport_snapshot(3, 8);
    let mut scratch = String::new();

    assert_eq!(viewport.size(), (3, 8));
    assert_eq!(viewport.rows()[0].absolute_row(), 0);
    assert_eq!(viewport.rows()[1].absolute_row(), 1);
    assert_eq!(viewport.rows()[0].cells()[0].glyph().as_str(&mut scratch), "e\u{0301}");

    let cursor = viewport.cursor().expect("cursor");
    assert_eq!((cursor.row(), cursor.col()), (1, 2));
}

#[test]
fn terminal_frontend_snapshot_distinguishes_hidden_cursor_from_viewport_cursor() {
    let mut engine = TerminalEngine::new(1, 4, 8);
    engine.process_output(b"\x1b[?25l");

    let snapshot = engine.view_model().frontend_snapshot(1, 4);

    assert!(snapshot.cursor().hidden());
    assert_eq!((snapshot.cursor().position().row(), snapshot.cursor().position().col()), (0, 0));
    assert!(snapshot.visible_cursor().is_none());
}

#[test]
fn terminal_frontend_snapshot_projects_scrollback_without_mutating_live_view() {
    let mut engine = TerminalEngine::new(2, 8, 8);
    engine.process_output(b"line1\r\nline2\r\nline3");

    let scrolled = engine.view_model().frontend_snapshot_at_scrollback(2, 8, 1);
    let scrolled_rows: Vec<String> = scrolled.viewport().rows().iter().map(|row| row.display_text()).collect();

    assert_eq!(scrolled.scrollback().display_offset(), 1);
    assert_eq!(trim_row(&scrolled_rows[0]), "line1");
    assert_eq!(trim_row(&scrolled_rows[1]), "line2");

    let live = engine.view_model().frontend_snapshot(2, 8);
    let live_rows: Vec<String> = live.viewport().rows().iter().map(|row| row.display_text()).collect();

    assert_eq!(live.scrollback().display_offset(), 0);
    assert_eq!(trim_row(&live_rows[0]), "line2");
    assert_eq!(trim_row(&live_rows[1]), "line3");
}

#[test]
fn terminal_search_matches_use_terminal_cell_ranges_for_wide_glyphs() {
    let mut engine = TerminalEngine::new(1, 8, 8);
    engine.process_output("A字B".as_bytes());

    let matches = engine.search_literal_matches("字");

    assert_eq!(matches, vec![(0, 1, 3)]);
}

#[test]
fn terminal_search_matches_keep_combining_clusters_to_single_cell_ranges() {
    let mut engine = TerminalEngine::new(1, 8, 8);
    engine.process_output("e\u{0301}x".as_bytes());

    let matches = engine.search_literal_matches("e\u{0301}");

    assert_eq!(matches, vec![(0, 0, 1)]);
}

#[test]
fn terminal_selection_tracks_terminal_coordinate_ranges() {
    let selection = TerminalSelection::new((2, 5), (1, 3)).ordered();

    assert_eq!((selection.start().absolute_row(), selection.start().column()), (1, 3));
    assert_eq!((selection.end().absolute_row(), selection.end().column()), (2, 5));
    assert!(selection.contains_cell(1, 3));
    assert!(selection.contains_cell(2, 4));
    assert!(!selection.contains_cell(0, 0));
    assert!(!selection.contains_cell(2, 6));
}
