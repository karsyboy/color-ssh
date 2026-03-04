use super::AppState;
use crate::inventory::InventoryHost;
use crate::tui::{HostTab, TerminalSearchState};
use crate::tui::terminal_emulator::Parser;

fn app_with_active_search() -> AppState {
    let mut app = AppState::new_for_tests();
    app.tabs.push(HostTab {
        host: InventoryHost::new("search-host".to_string()),
        title: "search-host".to_string(),
        session: None,
        session_error: None,
        scroll_offset: 0,
        terminal_search: TerminalSearchState {
            active: true,
            query: "alpha".to_string(),
            query_cursor: 5,
            query_selection: None,
            matches: vec![(0, 0, 5)],
            current: 0,
            ..TerminalSearchState::default()
        },
        force_ssh_logging: false,
        last_pty_size: None,
    });
    app.selected_tab = 0;
    app.focus_on_manager = false;
    app
}

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

#[test]
fn update_terminal_search_skips_when_query_and_epoch_are_unchanged() {
    let mut app = app_with_active_search();
    if let Some(search) = app.current_tab_search_mut() {
        search.last_search_query = "alpha".to_string();
        search.last_scanned_render_epoch = 0;
    }

    app.update_terminal_search();

    let search = app.current_tab_search().expect("search state");
    assert_eq!(search.matches, vec![(0, 0, 5)]);
    assert_eq!(search.last_search_query, "alpha");
}

#[test]
fn update_terminal_search_recomputes_when_query_changes() {
    let mut app = app_with_active_search();
    if let Some(search) = app.current_tab_search_mut() {
        search.query = "beta".to_string();
        search.last_search_query = "alpha".to_string();
        search.matches = vec![(9, 9, 9)];
    }

    app.update_terminal_search();

    let search = app.current_tab_search().expect("search state");
    assert!(search.matches.is_empty());
    assert_eq!(search.current, 0);
    assert_eq!(search.last_search_query, "beta");
}
