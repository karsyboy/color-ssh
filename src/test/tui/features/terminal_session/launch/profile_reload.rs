use super::*;

#[test]
fn resolve_session_profile_includes_profile_logging_settings_and_secret_patterns() {
    let workspace = TestWorkspace::new("tui", "profile_logging_snapshot").expect("test workspace");
    let _env = ProfileTestEnvironment::enter(&workspace);
    workspace
        .write_rel(
            "home/.color-ssh/network.cossh-config.yaml",
            "settings:\n  ssh_logging: true\n  remove_secrets:\n    - token=\\w+\nauth_settings:\n  tui_password_autologin: false\ninteractive_settings:\n  history_buffer: 2048\npalette: {}\nrules: []\n",
        )
        .expect("write profile config");

    let mut host = InventoryHost::new("router01".to_string());
    host.host = "10.0.0.10".to_string();
    host.profile = Some("network".to_string());

    let session_profile = AppState::resolve_session_profile(&host).expect("resolve host profile settings");

    assert!(session_profile.ssh_logging_enabled);
    assert_eq!(session_profile.secret_patterns.len(), 1);
}

#[test]
fn no_profile_tabs_use_live_current_config_overlay_rules() {
    let _state = TestStateGuard::lock();
    let mut config = base_config();
    config.metadata.compiled_rules = vec![compiled_rule("error", "\x1b[38;2;255;0;0m")];
    config.metadata.compiled_rule_set = Some(regex::RegexSet::new(["error"]).expect("rule set"));
    config::with_current_config_mut("install test overlay config", |current| *current = config);
    config::set_config_version(1);

    let host = InventoryHost::new("router01".to_string());
    let session_profile = AppState::resolve_session_profile(&host).expect("resolve current profile settings");
    let mut overlay_engine = highlight_overlay_for_host(&host, &session_profile);

    let mut terminal_engine = TerminalEngine::new(2, 20, 128);
    terminal_engine.process_output(b"error");
    let view = terminal_engine.view_model();
    let viewport = view.viewport_snapshot(2, 20);
    let overlay_view = HighlightOverlayViewport::new(&viewport, view.is_alternate_screen(), view.mouse_protocol().0, view.cursor_hidden());
    let first_overlay = overlay_engine.build_visible_overlay(
        &overlay_view,
        HighlightOverlayContext {
            render_epoch: 1,
            display_scrollback: 0,
        },
    );
    assert!(first_overlay.style_for_cell(0, 0).is_some());

    config::with_current_config_mut("update test overlay config", |current| {
        current.metadata.compiled_rules = vec![compiled_rule("warn", "\x1b[38;2;0;255;0m")];
        current.metadata.compiled_rule_set = Some(regex::RegexSet::new(["warn"]).expect("rule set"));
    });
    config::set_config_version(2);

    terminal_engine.process_output(b"\rwarn\x1b[K");
    let view = terminal_engine.view_model();
    let viewport = view.viewport_snapshot(2, 20);
    let overlay_view = HighlightOverlayViewport::new(&viewport, view.is_alternate_screen(), view.mouse_protocol().0, view.cursor_hidden());
    let second_overlay = overlay_engine.build_visible_overlay(
        &overlay_view,
        HighlightOverlayContext {
            render_epoch: 2,
            display_scrollback: 0,
        },
    );

    assert!(second_overlay.style_for_cell(0, 0).is_some());
}

#[test]
fn profile_tabs_refresh_overlay_rules_when_profile_reload_event_arrives() {
    let workspace = TestWorkspace::new("tui", "profile_tab_reload").expect("test workspace");
    let _env = ProfileTestEnvironment::enter(&workspace);
    workspace
        .write_rel(
            "home/.color-ssh/cossh-config.yaml",
            "settings:\n  ssh_logging: false\nauth_settings:\n  tui_password_autologin: true\ninteractive_settings:\n  history_buffer: 1000\npalette: {}\nrules: []\n",
        )
        .expect("write default config");
    workspace
        .write_rel(
            "home/.color-ssh/linux.cossh-config.yaml",
            "interactive_settings:\n  history_buffer: 2048\npalette:\n  alert: '#ffc800'\nrules:\n  - regex: 'warn'\n    color: alert\n",
        )
        .expect("write linux profile config");
    config::init_session_config(None).expect("load default config");

    let mut host = InventoryHost::new("linux01".to_string());
    host.host = "10.0.0.20".to_string();
    host.profile = Some("linux".to_string());

    let session_profile = AppState::resolve_session_profile(&host).expect("resolve initial profile settings");
    let mut overlay_engine = highlight_overlay_for_host(&host, &session_profile);
    let overlay_before = build_overlay_for_text(&mut overlay_engine, "error", 1);
    assert!(overlay_before.style_for_cell(0, 0).is_none());

    let mut app = AppState::new_for_tests();
    app.tabs.push(crate::tui::HostTab {
        host: host.clone(),
        title: host.name.clone(),
        session: None,
        session_error: None,
        highlight_overlay: overlay_engine,
        scroll_offset: 0,
        terminal_search: crate::tui::TerminalSearchState::default(),
        force_ssh_logging: false,
        last_pty_size: None,
    });

    workspace
        .write_rel(
            "home/.color-ssh/linux.cossh-config.yaml",
            "interactive_settings:\n  history_buffer: 2048\nrules:\n  - regex: 'error'\n    color: alert\npalette:\n  alert: '#00ff00'\n",
        )
        .expect("rewrite linux profile config");

    config::queue_profile_reload_event(config::ProfileReloadEvent {
        profile: "linux".to_string(),
        message: "Config profile 'linux' reloaded successfully".to_string(),
        success: true,
    });

    app.apply_config_reload_notifications();

    let overlay_after = build_overlay_for_text(&mut app.tabs[0].highlight_overlay, "error", 2);
    assert!(overlay_after.style_for_cell(0, 0).is_some());
    assert_eq!(
        app.reload_notice_toast.as_ref().map(|toast| toast.message()),
        Some("[color-ssh] Config profile 'linux' reloaded successfully")
    );
}
