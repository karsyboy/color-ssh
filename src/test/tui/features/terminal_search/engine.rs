use crate::tui::terminal_emulator::Parser;

#[test]
fn search_literal_matches_finds_multiple_matches_on_same_row() {
    let mut parser = Parser::new(2, 20, 50);
    parser.process(b"alpha alpha\\r\\n");
    let matches = parser.search_literal_matches("alpha");
    assert_eq!(matches, vec![(0, 0, 5), (0, 6, 5)]);
}

#[test]
fn search_literal_matches_is_case_insensitive() {
    let mut parser = Parser::new(2, 20, 50);
    parser.process(b"Status STATUS status\\r\\n");
    let matches = parser.search_literal_matches("status");
    assert_eq!(matches.len(), 3);
}
