use super::Parser;

#[test]
fn terminal_cell_tab_character_renders_as_space_without_contents() {
    let mut parser = Parser::new(2, 16, 100);
    parser.process_output(b"A\tB");

    let screen = parser.screen();
    let tab_cell = screen.cell(0, 1).expect("tab cell");

    assert!(!tab_cell.has_contents());
    assert_eq!(tab_cell.contents(), " ");

    let mut scratch = String::from("seed");
    assert_eq!(tab_cell.symbol(&mut scratch), " ");
}

#[test]
fn terminal_cell_plain_ascii_renders_normally() {
    let mut parser = Parser::new(2, 16, 100);
    parser.process_output(b"Z");

    let screen = parser.screen();
    let cell = screen.cell(0, 0).expect("cell");

    assert!(cell.has_contents());
    assert_eq!(cell.contents(), "Z");

    let mut scratch = String::new();
    assert_eq!(cell.symbol(&mut scratch), "Z");
}

#[test]
fn terminal_cell_symbol_combining_mark_is_included_in_rendered_symbol() {
    let mut parser = Parser::new(2, 16, 100);
    parser.process_output("e\u{0301}".as_bytes());

    let screen = parser.screen();
    let cell = screen.cell(0, 0).expect("cell");

    let mut scratch = String::new();
    assert_eq!(cell.symbol(&mut scratch), "e\u{0301}");
}
