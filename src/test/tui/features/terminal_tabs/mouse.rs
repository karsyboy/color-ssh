use super::{SessionManager, force_local_selection};
use crate::ssh_config::SshHost;
use crate::tui::terminal_emulator::MouseProtocolEncoding;
use crate::tui::{HostTab, TerminalSearchState};
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

fn app_with_tabs(titles: &[&str]) -> SessionManager {
    let mut app = SessionManager::new_for_tests();
    app.host_panel_visible = false;
    app.host_panel_area = Rect::default();
    app.host_list_area = Rect::default();
    app.tab_bar_area = Rect::new(0, 0, 40, 1);
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
fn encode_mouse_event_bytes_sgr_press_and_release() {
    let press = SessionManager::encode_mouse_event_bytes(MouseProtocolEncoding::Sgr, 0, 10, 5, false);
    let release = SessionManager::encode_mouse_event_bytes(MouseProtocolEncoding::Sgr, 0, 10, 5, true);

    assert_eq!(press, b"\x1b[<0;10;5M".to_vec());
    assert_eq!(release, b"\x1b[<0;10;5m".to_vec());
}

#[test]
fn encode_mouse_event_bytes_default_clamps_large_coords() {
    let bytes = SessionManager::encode_mouse_event_bytes(MouseProtocolEncoding::Default, 0, 500, 900, false);
    assert_eq!(bytes, vec![0x1b, b'[', b'M', 32, 255, 255]);
}

#[test]
fn force_local_selection_accepts_shift_and_alt() {
    assert!(force_local_selection(KeyModifiers::SHIFT));
    assert!(force_local_selection(KeyModifiers::ALT));
    assert!(force_local_selection(KeyModifiers::SHIFT | KeyModifiers::ALT));
    assert!(!force_local_selection(KeyModifiers::NONE));
}

#[test]
fn closes_wide_title_tab_when_clicking_close_glyph() {
    let mut app = app_with_tabs(&["a界", "next"]);
    let close_click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 4,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };

    app.handle_mouse(close_click).expect("mouse handling");
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.tabs[0].title, "next");
}

#[test]
fn selects_tab_when_clicking_title_region_not_close_glyph() {
    let mut app = app_with_tabs(&["one", "a界"]);
    let select_second_tab = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 7,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };

    app.handle_mouse(select_second_tab).expect("mouse handling");
    assert_eq!(app.selected_tab, 1);
    assert_eq!(app.tabs.len(), 2);
}

#[test]
fn drags_tab_title_to_reorder_tabs() {
    let mut app = app_with_tabs(&["one", "two", "three"]);

    let down_first_tab = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(down_first_tab).expect("mouse down");

    let drag_to_second_tab = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 7,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(drag_to_second_tab).expect("mouse drag");

    let up_on_second_tab = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 7,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(up_on_second_tab).expect("mouse up");

    let titles: Vec<&str> = app.tabs.iter().map(|tab| tab.title.as_str()).collect();
    assert_eq!(titles, vec!["two", "one", "three"]);
    assert_eq!(app.selected_tab, 1);
}

#[test]
fn scroll_markers_move_tab_strip_left_and_right() {
    let mut app = app_with_tabs(&["one", "two", "three", "four"]);
    app.tab_bar_area = Rect::new(0, 0, 10, 1);
    app.tab_scroll_offset = 0;

    let right_marker_click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 9,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(right_marker_click).expect("right marker click");
    assert!(app.tab_scroll_offset > 0);

    let left_marker_click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(left_marker_click).expect("left marker click");
    assert_eq!(app.tab_scroll_offset, 0);
}

#[test]
fn left_drag_selection_is_kept_on_release() {
    let mut app = app_with_tabs(&["one"]);
    app.tab_content_area = Rect::new(0, 1, 40, 10);
    app.focus_on_manager = false;

    let down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1,
        row: 2,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(down).expect("mouse down");

    let drag = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: 4,
        row: 2,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(drag).expect("mouse drag");

    let up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 4,
        row: 2,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(up).expect("mouse up");

    assert!(!app.is_selecting);
    assert!(app.selection_start.is_some());
    assert!(app.selection_end.is_some());
}

#[test]
fn right_click_copies_and_clears_existing_selection() {
    let mut app = app_with_tabs(&["one"]);
    app.tab_content_area = Rect::new(0, 1, 40, 10);
    app.focus_on_manager = false;
    app.selection_start = Some((0, 1));
    app.selection_end = Some((0, 4));
    app.selection_dragged = true;

    let right_down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: 2,
        row: 2,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(right_down).expect("right down");

    let right_up = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Right),
        column: 2,
        row: 2,
        modifiers: KeyModifiers::NONE,
    };
    app.handle_mouse(right_up).expect("right up");

    assert!(app.selection_start.is_none());
    assert!(app.selection_end.is_none());
    assert!(!app.selection_dragged);
}
