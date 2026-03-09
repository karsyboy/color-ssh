use super::{paint_terminal_viewport, reload_notice_toast_area};
use crate::terminal_core::TerminalEngine;
use ratatui::buffer::Buffer;
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

#[test]
fn paint_terminal_viewport_draws_blank_cursor_cells_as_underscore() {
    let engine = TerminalEngine::new(1, 4, 8);
    let viewport = engine.view_model().viewport_snapshot(1, 4);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 1));

    let cursor = paint_terminal_viewport(
        &mut buffer,
        Rect::new(0, 0, 4, 1),
        &viewport,
        true,
        |_row, _col, _cell, _is_cursor, base_style| base_style,
    );

    assert_eq!(cursor, Some((0, 0).into()));
    assert_eq!(buffer[(0, 0)].symbol(), "_");
}
