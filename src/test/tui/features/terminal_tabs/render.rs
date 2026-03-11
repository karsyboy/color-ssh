use super::{search_row_ranges_contain, split_terminal_content_and_scrollbar, terminal_scrollbar_geometry};
use ratatui::layout::Rect;

#[test]
fn split_terminal_content_and_scrollbar_reserves_last_column_when_available() {
    let (content_area, scrollbar_area) = split_terminal_content_and_scrollbar(Rect::new(4, 2, 20, 8));

    assert_eq!(content_area, Rect::new(4, 2, 19, 8));
    assert_eq!(scrollbar_area, Rect::new(23, 2, 1, 8));
}

#[test]
fn terminal_scrollbar_geometry_matches_expected_positions_for_common_cases() {
    let cases = [
        ("latest", Rect::new(0, 0, 21, 10), 10, 90, 0, 1, 9),
        ("oldest", Rect::new(0, 0, 21, 10), 10, 90, 90, 1, 0),
        ("no_scrollback", Rect::new(0, 0, 21, 6), 6, 0, 0, 6, 0),
    ];

    for (case_name, area, visible_rows, total_scrollback, display_offset, expected_thumb_height, expected_thumb_top) in cases {
        let (_, scrollbar_area) = split_terminal_content_and_scrollbar(area);
        let geometry = terminal_scrollbar_geometry(scrollbar_area, visible_rows, total_scrollback, display_offset).expect("scrollbar geometry");

        assert_eq!(geometry.thumb_height, expected_thumb_height, "unexpected thumb height for case: {case_name}");
        assert_eq!(geometry.thumb_top, expected_thumb_top, "unexpected thumb top for case: {case_name}");
    }
}

#[test]
fn search_row_ranges_contain_treats_end_column_as_exclusive() {
    let ranges = [(2, 5), (8, 10)];

    assert!(search_row_ranges_contain(Some(&ranges), 2));
    assert!(search_row_ranges_contain(Some(&ranges), 4));
    assert!(!search_row_ranges_contain(Some(&ranges), 5));
    assert!(!search_row_ranges_contain(Some(&ranges), 7));
    assert!(search_row_ranges_contain(Some(&ranges), 8));
    assert!(!search_row_ranges_contain(Some(&ranges), 10));
}
