use super::*;

#[test]
fn resolve_session_profile_includes_profile_logging_settings_and_secret_patterns() {
    let workspace = TestWorkspace::new("tui", "profile_logging_snapshot").expect("test workspace");
    with_profile_test_environment(&workspace, || {
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
    });
}
