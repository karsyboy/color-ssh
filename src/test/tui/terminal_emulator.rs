use super::Parser;

#[test]
fn tabs_render_as_spaces_not_control_chars() {
    let mut parser = Parser::new(2, 16, 100);
    parser.process(b"A\tB");
    let screen = parser.screen();

    let tab_cell = screen.cell(0, 1).expect("tab cell");
    assert!(!tab_cell.has_contents());
    assert_eq!(tab_cell.contents(), " ");

    let mut scratch = String::from("seed");
    assert_eq!(tab_cell.symbol(&mut scratch), " ");
}

#[test]
fn plain_ascii_cell_is_rendered_normally() {
    let mut parser = Parser::new(2, 16, 100);
    parser.process(b"Z");
    let screen = parser.screen();

    let cell = screen.cell(0, 0).expect("cell");
    assert!(cell.has_contents());
    assert_eq!(cell.contents(), "Z");

    let mut scratch = String::new();
    assert_eq!(cell.symbol(&mut scratch), "Z");
}

#[test]
fn cell_symbol_includes_zero_width_combining_marks() {
    let mut parser = Parser::new(2, 16, 100);
    parser.process("e\u{0301}".as_bytes());
    let screen = parser.screen();

    let cell = screen.cell(0, 0).expect("cell");
    let mut scratch = String::new();
    assert_eq!(cell.symbol(&mut scratch), "e\u{0301}");
}
