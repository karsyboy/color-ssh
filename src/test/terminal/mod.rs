use super::TerminalSelection;

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
