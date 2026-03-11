use super::*;

#[test]
fn resolve_host_pass_password_for_rdp_without_vault_pass_is_launchable() {
    let mut app = AppState::new_for_tests();
    let host = sample_rdp_host();
    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host.clone()),
        force_ssh_logging: false,
        auth_settings: AuthSettings::default(),
    };

    let resolution = app.resolve_host_pass_password_with_autologin(&host, action, true);

    assert_eq!(
        resolution,
        Some(HostPassResolution {
            pass_entry_override: None,
            pass_fallback_notice: None,
            disable_vault_autologin: false,
        })
    );
}

#[test]
fn resolve_host_pass_password_for_rdp_with_tui_autologin_disabled_is_launchable() {
    let mut app = AppState::new_for_tests();
    let mut host = sample_rdp_host();
    host.vault_pass = Some("shared".to_string());
    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host.clone()),
        force_ssh_logging: false,
        auth_settings: AuthSettings::default(),
    };

    let resolution = app.resolve_host_pass_password_with_autologin(&host, action, false);

    assert_eq!(
        resolution,
        Some(HostPassResolution {
            pass_entry_override: None,
            pass_fallback_notice: None,
            disable_vault_autologin: true,
        })
    );
}

#[test]
fn resolve_host_pass_password_uses_inventory_profile_auth_settings() {
    let workspace = TestWorkspace::new("tui", "profile_launch").expect("test workspace");
    with_profile_test_environment(&workspace, || {
        workspace
            .write_rel("home/.color-ssh/cossh-config.yaml", &launch_config_yaml(true, 1000))
            .expect("write default config");
        workspace
            .write_rel("home/.color-ssh/network.cossh-config.yaml", &launch_config_yaml(false, 2048))
            .expect("write profile config");
        config::init_session_config(None).expect("load default config");

        let mut app = AppState::new_for_tests();
        let mut host = InventoryHost::new("router01".to_string());
        host.host = "10.0.0.10".to_string();
        host.profile = Some("network".to_string());
        host.vault_pass = Some("shared".to_string());

        let session_profile = AppState::resolve_session_profile(&host).expect("resolve host profile settings");
        assert_eq!(session_profile.history_buffer, 2048);
        assert!(!session_profile.auth_settings.tui_password_autologin);

        let action = VaultUnlockAction::OpenHostTab {
            host: Box::new(host.clone()),
            force_ssh_logging: false,
            auth_settings: session_profile.auth_settings.clone(),
        };
        let resolution = app.resolve_host_pass_password(&host, action, &session_profile.auth_settings);

        assert_eq!(
            resolution,
            Some(HostPassResolution {
                pass_entry_override: None,
                pass_fallback_notice: None,
                disable_vault_autologin: true,
            })
        );
    });
}
