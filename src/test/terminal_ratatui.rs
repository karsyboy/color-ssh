use super::reload_notice_toast_area;
use ratatui::layout::Rect;

#[test]
fn reload_notice_toast_area_places_toast_in_bottom_right_corner() {
    let area = reload_notice_toast_area(Rect::new(0, 0, 100, 30), "[color-ssh] Config reloaded successfully");

    assert_eq!(area.y + area.height, 29);
    assert_eq!(area.x + area.width, 99);
    assert!(area.width >= 3);
    assert!(area.height >= 3);
}

#[test]
fn reload_notice_toast_area_clamps_to_small_terminal_sizes() {
    let area = reload_notice_toast_area(Rect::new(0, 0, 12, 5), "[color-ssh] Config reloaded successfully");

    assert!(area.width <= 10);
    assert!(area.height <= 3);
}
