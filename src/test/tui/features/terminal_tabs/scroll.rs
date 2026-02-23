use super::SessionManager;
use crate::ssh_config::SshHost;
use crate::tui::{HostTab, TerminalSearchState};

fn app_with_tab_titles(titles: &[&str]) -> SessionManager {
    let mut app = SessionManager::new_for_tests();
    for title in titles {
        app.tabs.push(HostTab {
            host: SshHost::new((*title).to_string()),
            title: (*title).to_string(),
            session: None,
            scroll_offset: 0,
            terminal_search: TerminalSearchState::default(),
            force_ssh_logging: false,
            last_pty_size: None,
        });
    }
    app
}

#[test]
fn normalizes_tab_scroll_offset_by_snapping_and_clamping() {
    let app = app_with_tab_titles(&["aaa", "bbb", "ccc"]);
    assert_eq!(app.normalize_tab_scroll_offset(7, 8), 6);
    assert_eq!(app.normalize_tab_scroll_offset(999, 8), 12);
}

#[test]
fn computes_prev_and_next_tab_scroll_offsets_with_overflow() {
    let app = app_with_tab_titles(&["aaa", "bbb", "ccc"]);
    assert_eq!(app.prev_tab_scroll_offset(6, 8), Some(0));
    assert_eq!(app.next_tab_scroll_offset(6, 8), Some(12));
    assert_eq!(app.next_tab_scroll_offset(12, 8), None);
}

#[test]
fn computes_final_right_offset_with_wide_titles() {
    let app = app_with_tab_titles(&["a界", "bbbb", "cc"]);
    assert_eq!(app.final_right_tab_scroll_offset(10), 13);
}
