use super::*;
use crate::ssh_config::SshHost;
use crate::tui::{HostTab, PassPromptAction, PassPromptState, QuickConnectField, QuickConnectState, TerminalSearchState};

fn host_tab(title: &str) -> HostTab {
    HostTab {
        host: SshHost::new(title.to_string()),
        title: title.to_string(),
        session: None,
        scroll_offset: 0,
        terminal_search: TerminalSearchState::default(),
        force_ssh_logging: false,
        last_pty_size: None,
    }
}

#[test]
fn handle_key_ctrl_q_sets_should_exit() {
    let mut app = SessionManager::new_for_tests();
    let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
    app.handle_key(key).expect("handle_key should succeed");
    assert!(app.should_exit);
}

#[test]
fn handle_key_ctrl_q_does_not_exit_in_terminal_view() {
    let mut app = SessionManager::new_for_tests();
    app.tabs.push(host_tab("test-host"));
    app.selected_tab = 0;
    app.focus_on_manager = false;

    let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
    app.handle_key(key).expect("handle_key should succeed");
    assert!(!app.should_exit);
}

#[test]
fn handle_key_ctrl_left_reorders_selected_tab_left() {
    let mut app = SessionManager::new_for_tests();
    app.tabs.push(host_tab("one"));
    app.tabs.push(host_tab("two"));
    app.tabs.push(host_tab("three"));
    app.selected_tab = 1;
    app.focus_on_manager = false;

    let key = KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL);
    app.handle_key(key).expect("handle_key should succeed");

    let titles: Vec<&str> = app.tabs.iter().map(|tab| tab.title.as_str()).collect();
    assert_eq!(titles, vec!["two", "one", "three"]);
    assert_eq!(app.selected_tab, 0);
}

#[test]
fn handle_key_ctrl_right_reorders_selected_tab_right() {
    let mut app = SessionManager::new_for_tests();
    app.tabs.push(host_tab("one"));
    app.tabs.push(host_tab("two"));
    app.tabs.push(host_tab("three"));
    app.selected_tab = 1;
    app.focus_on_manager = false;

    let key = KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL);
    app.handle_key(key).expect("handle_key should succeed");

    let titles: Vec<&str> = app.tabs.iter().map(|tab| tab.title.as_str()).collect();
    assert_eq!(titles, vec!["one", "three", "two"]);
    assert_eq!(app.selected_tab, 2);
}

#[test]
fn handle_paste_routes_to_quick_connect_form_when_modal_open() {
    let mut app = SessionManager::new_for_tests();
    let mut form = QuickConnectState::new(false, vec!["default".to_string()]);
    form.selected = QuickConnectField::User;
    app.quick_connect = Some(form);

    app.handle_paste("ops\n".to_string()).expect("paste should succeed");

    let form = app.quick_connect.as_ref().expect("quick connect state");
    assert_eq!(form.user, "ops");
}

#[test]
fn handle_paste_routes_to_pass_prompt_when_modal_open() {
    let mut app = SessionManager::new_for_tests();
    app.pass_prompt = Some(PassPromptState::new("shared".to_string(), PassPromptAction::ReconnectTab { tab_index: 0 }));

    app.handle_paste("secret\n".to_string()).expect("paste should succeed");

    let prompt = app.pass_prompt.as_ref().expect("pass prompt state");
    assert_eq!(prompt.passphrase, "secret");
}

#[test]
fn encode_paste_bytes_wraps_bracketed_payload_when_enabled() {
    let pasted = "hello\nworld";
    assert_eq!(encode_paste_bytes(pasted, true), b"\x1b[200~hello\nworld\x1b[201~".to_vec());
    assert_eq!(encode_paste_bytes(pasted, false), pasted.as_bytes().to_vec());
}

#[test]
fn alt_c_copies_and_clears_selection() {
    let mut app = SessionManager::new_for_tests();
    app.tabs.push(host_tab("copy-target"));
    app.selected_tab = 0;
    app.focus_on_manager = false;
    app.selection_start = Some((0, 1));
    app.selection_end = Some((0, 4));
    app.selection_dragged = true;

    let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT);
    app.handle_key(key).expect("handle_key should succeed");

    assert!(app.selection_start.is_none());
    assert!(app.selection_end.is_none());
    assert!(!app.selection_dragged);
}
