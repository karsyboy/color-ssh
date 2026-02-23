use super::SessionManager;
use crate::ssh_config::SshHost;
use crate::tui::{HostTab, TerminalSearchState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn app_with_active_search() -> SessionManager {
    let mut app = SessionManager::new_for_tests();
    app.tabs.push(HostTab {
        host: SshHost::new("test-host".to_string()),
        title: "test-host".to_string(),
        session: None,
        scroll_offset: 0,
        terminal_search: TerminalSearchState {
            active: true,
            query: String::new(),
            query_cursor: 0,
            query_selection: None,
            matches: Vec::new(),
            current: 0,
        },
        force_ssh_logging: false,
        last_pty_size: None,
    });
    app.selected_tab = 0;
    app.focus_on_manager = false;
    app
}

#[test]
fn clears_terminal_search_state() {
    let mut app = app_with_active_search();
    if let Some(search) = app.current_tab_search_mut() {
        search.query = "err".to_string();
        search.matches = vec![(0, 1, 3)];
        search.current = 2;
    }

    app.clear_terminal_search();

    let search = app.current_tab_search().expect("search state");
    assert!(!search.active);
    assert!(search.query.is_empty());
    assert!(search.matches.is_empty());
    assert_eq!(search.current, 0);
}

#[test]
fn wraps_terminal_search_navigation_next_and_prev() {
    let mut app = app_with_active_search();
    if let Some(search) = app.current_tab_search_mut() {
        search.matches = vec![(0, 0, 1), (1, 0, 1)];
        search.current = 1;
    }

    app.handle_terminal_search_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
        .expect("down key");
    assert_eq!(app.current_tab_search().map(|search| search.current), Some(0));

    app.handle_terminal_search_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)).expect("up key");
    assert_eq!(app.current_tab_search().map(|search| search.current), Some(1));
}

#[test]
fn edits_terminal_search_query_with_char_and_backspace() {
    let mut app = app_with_active_search();

    app.handle_terminal_search_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE))
        .expect("insert char");
    assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some("a"));

    app.handle_terminal_search_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
        .expect("backspace");
    assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some(""));
}

#[test]
fn edits_terminal_search_query_in_the_middle() {
    let mut app = app_with_active_search();
    if let Some(search) = app.current_tab_search_mut() {
        search.query = "admn".to_string();
        search.query_cursor = 3;
    }

    app.handle_terminal_search_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE))
        .expect("insert char");
    assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some("admin"));

    app.handle_terminal_search_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)).expect("left");
    app.handle_terminal_search_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE))
        .expect("delete");
    assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some("admn"));
}

#[test]
fn paste_appends_terminal_search_query() {
    let mut app = app_with_active_search();
    app.handle_terminal_search_paste("err\nwarn");

    // Control characters are filtered from pasted text.
    assert_eq!(app.current_tab_search().map(|search| search.query.as_str()), Some("errwarn"));
}
