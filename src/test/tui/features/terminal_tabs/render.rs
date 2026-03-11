use super::{search_row_ranges_contain, split_terminal_content_and_scrollbar, terminal_scrollbar_geometry};
use ratatui::layout::Rect;

#[test]
fn split_terminal_content_and_scrollbar_reserves_last_column_when_available() {
    let (content_area, scrollbar_area) = split_terminal_content_and_scrollbar(Rect::new(4, 2, 20, 8));

    assert_eq!(content_area, Rect::new(4, 2, 19, 8));
    assert_eq!(scrollbar_area, Rect::new(23, 2, 1, 8));
}

#[test]
fn terminal_scrollbar_geometry_places_latest_content_thumb_at_bottom() {
    let (_, scrollbar_area) = split_terminal_content_and_scrollbar(Rect::new(0, 0, 21, 10));
    let geometry = terminal_scrollbar_geometry(scrollbar_area, 10, 90, 0).expect("scrollbar geometry");

    assert_eq!(geometry.thumb_height, 1);
    assert_eq!(geometry.thumb_top, 9);
}

#[test]
fn terminal_scrollbar_geometry_places_oldest_content_thumb_at_top() {
    let (_, scrollbar_area) = split_terminal_content_and_scrollbar(Rect::new(0, 0, 21, 10));
    let geometry = terminal_scrollbar_geometry(scrollbar_area, 10, 90, 90).expect("scrollbar geometry");

    assert_eq!(geometry.thumb_height, 1);
    assert_eq!(geometry.thumb_top, 0);
}

#[test]
fn terminal_scrollbar_geometry_expands_thumb_when_no_scrollback_exists() {
    let (_, scrollbar_area) = split_terminal_content_and_scrollbar(Rect::new(0, 0, 21, 6));
    let geometry = terminal_scrollbar_geometry(scrollbar_area, 6, 0, 0).expect("scrollbar geometry");

    assert_eq!(geometry.thumb_height, 6);
    assert_eq!(geometry.thumb_top, 0);
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
