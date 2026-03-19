use super::{AppState, TabBarHit};
use crate::inventory::InventoryHost;
use crate::terminal::highlight_overlay::HighlightOverlayEngine;
use crate::tui::{EditorTabId, EditorTabState, HostEditorState, HostTab, TerminalSearchState, TerminalTabState};
use ratatui::layout::Rect;
use std::path::PathBuf;

fn terminal_tab(name: &str) -> HostTab {
    HostTab::new_terminal(TerminalTabState {
        host: InventoryHost::new(name.to_string()),
        title: name.to_string(),
        session: None,
        session_error: None,
        highlight_overlay: HighlightOverlayEngine::new(),
        scroll_offset: 0,
        terminal_search: TerminalSearchState::default(),
        force_ssh_logging: false,
        last_pty_size: None,
    })
}

fn editor_tab(source_file: &str) -> HostTab {
    let source_file = PathBuf::from(source_file);
    HostTab::new_editor(EditorTabState {
        id: EditorTabId::for_new_entry(source_file.clone()),
        editor_state: HostEditorState::new_create(source_file, vec![], vec![]),
    })
}

#[test]
fn ensure_tab_visible_accounts_for_overflow_markers() {
    let mut app = AppState::new_for_tests();
    app.tabs = vec![terminal_tab("a"), terminal_tab("b"), terminal_tab("c")];
    app.selected_tab = 1;
    app.tab_bar_area = Rect::new(0, 0, 8, 1);
    app.tab_scroll_offset = 0;

    app.ensure_tab_visible();

    assert_eq!(app.tab_scroll_offset, app.tab_start_offset(1));

    let metrics = app.tab_bar_viewport_metrics(app.tab_scroll_offset, app.tab_bar_area.width as usize);
    let selected_start = app.tab_start_offset(app.selected_tab);
    let selected_end = selected_start + app.tab_display_width(app.selected_tab);
    assert!(selected_end <= metrics.scroll_offset + metrics.visible_tab_width);
}

#[test]
fn tab_bar_hit_test_distinguishes_close_targets_for_mixed_tabs() {
    let mut app = AppState::new_for_tests();
    app.tabs = vec![terminal_tab("alpha"), editor_tab("/tmp/inventory.yaml"), terminal_tab("beta")];
    app.tab_bar_area = Rect::new(5, 1, 60, 1);

    let editor_close_col = app.tab_bar_area.x + app.tab_start_offset(1) as u16 + app.tab_title_display_width(1) as u16 + 1;
    let terminal_close_col = app.tab_bar_area.x + app.tab_start_offset(2) as u16 + app.tab_title_display_width(2) as u16 + 1;
    let terminal_title_col = app.tab_bar_area.x + app.tab_start_offset(0) as u16;

    assert_eq!(app.tab_bar_hit_test(editor_close_col), Some(TabBarHit::TabClose(1)));
    assert_eq!(app.tab_bar_hit_test(terminal_close_col), Some(TabBarHit::TabClose(2)));
    assert_eq!(app.tab_bar_hit_test(terminal_title_col), Some(TabBarHit::TabTitle(0)));
}

#[test]
fn move_tab_preserves_selected_mixed_tab_identity() {
    let mut app = AppState::new_for_tests();
    app.tabs = vec![terminal_tab("alpha"), editor_tab("/tmp/inventory.yaml"), terminal_tab("beta")];
    app.selected_tab = 1;
    app.tab_bar_area = Rect::new(0, 0, 24, 1);

    assert!(app.move_tab(1, 2));

    assert_eq!(app.selected_tab, 2);
    assert!(app.tabs[2].editor().is_some(), "selected editor tab should remain selected after reorder");
    assert_eq!(app.tabs[0].terminal().map(|terminal| terminal.title.as_str()), Some("alpha"));
    assert_eq!(app.tabs[1].terminal().map(|terminal| terminal.title.as_str()), Some("beta"));
}
