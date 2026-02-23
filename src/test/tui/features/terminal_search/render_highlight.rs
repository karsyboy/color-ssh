use super::build_search_row_ranges;
use crate::tui::TerminalSearchState;

#[test]
fn build_search_row_ranges_groups_matches_by_row() {
    let search = TerminalSearchState {
        active: true,
        query: "abc".to_string(),
        query_cursor: 3,
        query_selection: None,
        matches: vec![(2, 4, 3), (2, 10, 2), (3, 1, 1)],
        current: 1,
    };

    let (ranges, current) = build_search_row_ranges(Some(&search));
    assert_eq!(ranges.get(&2).map(Vec::len), Some(2));
    assert_eq!(ranges.get(&3).map(Vec::len), Some(1));
    assert_eq!(current, Some((2, 10, 12)));
}
