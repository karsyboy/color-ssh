use super::AppState;
use crate::config;
use crate::terminal_core::TerminalGridPoint;
use std::sync::{Mutex, OnceLock};

static APP_STATE_CONFIG_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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
fn apply_config_reload_notifications_sets_reload_notice_toast() {
    let _lock = APP_STATE_CONFIG_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("app state config test lock");
    let _ = config::take_reload_notices();

    let mut app = AppState::new_for_tests();

    config::queue_reload_notice("Config reloaded successfully".to_string());
    app.apply_config_reload_notifications();

    assert_eq!(
        app.reload_notice_toast.as_ref().map(|toast| toast.message()),
        Some("[color-ssh] Config reloaded successfully")
    );

    let _ = config::take_reload_notices();
}

#[test]
fn current_selection_orders_typed_terminal_points() {
    let mut app = AppState::new_for_tests();
    app.selection_start = Some(TerminalGridPoint::new(4, 10));
    app.selection_end = Some(TerminalGridPoint::new(2, 3));

    let selection = app.current_selection().expect("current selection");

    assert_eq!(selection.start(), TerminalGridPoint::new(2, 3));
    assert_eq!(selection.end(), TerminalGridPoint::new(4, 10));
}
