use super::extract_selection_text;
use crate::tui::terminal_emulator::Parser;

#[test]
fn extract_selection_text_restores_scrollback_view() {
    let mut parser = Parser::new(2, 5, 50);
    parser.process(b"11111\r\n22222\r\n33333\r\n");
    parser.set_scrollback(1);
    let before_cell = parser.screen().cell(0, 0).map(|cell| cell.contents()).unwrap_or_default();

    let _ = extract_selection_text(&parser, (0, 0), (0, 2));
    let after_cell = parser.screen().cell(0, 0).map(|cell| cell.contents()).unwrap_or_default();

    assert_eq!(before_cell, after_cell);
}
