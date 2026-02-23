use super::AppState;

#[test]
fn terminal_resize_scales_host_panel_width_proportionally() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (100, 30);
    app.host_panel_width = 25;

    app.handle_terminal_resize(200, 30);
    assert_eq!(app.host_panel_width, 50);

    app.handle_terminal_resize(120, 30);
    assert_eq!(app.host_panel_width, 30);
}

#[test]
fn terminal_resize_clamps_host_panel_width_for_small_windows() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (120, 30);
    app.host_panel_width = 30;

    app.handle_terminal_resize(10, 30);
    assert_eq!(app.host_panel_width, 9);
}

#[test]
fn terminal_resize_growth_caps_host_panel_width_at_default() {
    let mut app = AppState::new_for_tests();
    app.last_terminal_size = (100, 30);
    app.host_panel_default_percent = 25;
    app.host_panel_width = 60;

    app.handle_terminal_resize(200, 30);
    assert_eq!(app.host_panel_width, 50);
}
