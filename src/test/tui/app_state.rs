use super::AppState;
use crate::config;
use crate::inventory::InventoryHost;
use crate::terminal_core::highlight_overlay::HighlightOverlayEngine;
use crate::terminal_core::{TerminalChild, TerminalEngine, TerminalSession};
use crate::tui::{HostTab, TerminalSearchState};
use std::sync::{Mutex, OnceLock};
use std::{
    process::Command,
    sync::{Arc, atomic::AtomicU64},
};

static APP_STATE_CONFIG_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn test_terminal_session(rows: u16, cols: u16) -> (TerminalSession, Arc<Mutex<TerminalEngine>>) {
    let child = Command::new("true").spawn().expect("spawn test child");
    let engine = Arc::new(Mutex::new(TerminalEngine::new(rows, cols, 128)));
    let session = TerminalSession::new(
        None,
        None,
        TerminalChild::Process(Arc::new(Mutex::new(child))),
        engine.clone(),
        Arc::new(Mutex::new(false)),
        Arc::new(AtomicU64::new(0)),
    );

    (session, engine)
}

#[test]
fn handle_terminal_resize_growing_and_shrinking_width_scales_host_panel_proportionally() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (100, 30);
    app.host_panel_width = 25;

    app.handle_terminal_resize(200, 30);
    assert_eq!(app.host_panel_width, 50);

    app.handle_terminal_resize(120, 30);
    assert_eq!(app.host_panel_width, 30);
}

#[test]
fn handle_terminal_resize_very_small_window_clamps_host_panel_width() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (120, 30);
    app.host_panel_width = 30;

    app.handle_terminal_resize(10, 30);

    assert_eq!(app.host_panel_width, 9);
}

#[test]
fn handle_terminal_resize_window_growth_caps_width_at_default_percent() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (100, 30);
    app.host_panel_default_percent = 25;
    app.host_panel_width = 60;

    app.handle_terminal_resize(200, 30);

    assert_eq!(app.host_panel_width, 50);
}

#[test]
fn should_draw_when_config_version_changes() {
    let _lock = APP_STATE_CONFIG_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("app state config test lock");
    let mut app = AppState::new_for_tests();
    app.ui_dirty = false;
    app.mark_drawn();

    let original_version = app.last_seen_config_version;
    config::set_config_version(original_version.wrapping_add(1));

    assert!(app.should_draw(std::time::Duration::from_secs(60)));

    config::set_config_version(original_version);
}

#[test]
fn apply_config_reload_notifications_injects_notice_into_selected_tab_terminal() {
    let _lock = APP_STATE_CONFIG_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("app state config test lock");
    let _ = config::take_reload_notices();

    let mut app = AppState::new_for_tests();
    let (session, engine) = test_terminal_session(4, 80);
    app.tabs.push(HostTab {
        host: InventoryHost::new("router01".to_string()),
        title: "router01".to_string(),
        session: Some(session),
        session_error: None,
        highlight_overlay: HighlightOverlayEngine::new(),
        scroll_offset: 0,
        terminal_search: TerminalSearchState::default(),
        force_ssh_logging: false,
        last_pty_size: None,
    });
    app.selected_tab = 0;

    config::queue_reload_notice("Config reloaded successfully".to_string());
    app.apply_config_reload_notifications();

    let viewport = engine.lock().expect("terminal engine lock").view_model().viewport_snapshot(4, 80);
    assert!(viewport.rows().iter().any(|row| row.display_text().contains("Config reloaded successfully")));
    assert!(app.pending_config_reload_notices.is_empty());

    let _ = config::take_reload_notices();
}
