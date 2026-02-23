use super::SessionManager;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn paste_inserts_host_search_text_at_cursor() {
    let mut app = SessionManager::new_for_tests();
    app.search_mode = true;
    app.search_query = "core-router".to_string();
    app.search_query_cursor = 4;
    app.search_query_selection = None;

    app.handle_search_paste("-edge");

    assert_eq!(app.search_query, "core-edge-router");
    assert_eq!(app.search_query_cursor, 9);
    assert_eq!(app.search_query_selection, None);
}

#[test]
fn host_search_supports_middle_insert_and_delete() {
    let mut app = SessionManager::new_for_tests();
    app.search_mode = true;
    app.search_query = "admn".to_string();
    app.search_query_cursor = 3;
    app.search_query_selection = None;

    app.handle_search_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE)).expect("insert");
    assert_eq!(app.search_query, "admin");

    app.handle_search_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)).expect("left");
    app.handle_search_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)).expect("delete");
    assert_eq!(app.search_query, "admn");
}
