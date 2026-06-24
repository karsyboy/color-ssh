use super::*;

#[test]
fn open_host_tab_for_rdp_without_vault_password_opens_credentials_modal() {
    let mut app = AppState::new_for_tests();
    let host = sample_rdp_host();

    app.open_host_tab(host, false);

    let prompt = app.rdp_credentials.as_ref().expect("RDP credentials modal");
    assert_eq!(prompt.target_label, "desktop01 (rdp.internal)");
    assert!(app.tabs.is_empty());
}

#[test]
fn open_host_tab_for_rdp_with_missing_username_and_unlocked_vault_opens_credentials_modal() {
    let mut app = AppState::new_for_tests();
    let mut host = sample_rdp_host();
    host.user = None;

    let auth_resolution = HostPassResolution {
        pass_entry_override: Some("shared".to_string()),
        pass_fallback_notice: None,
        disable_vault_autologin: false,
        manual_rdp_password: None,
    };

    assert!(app.maybe_open_rdp_credentials_modal_for_open_host(&host, false, &auth_resolution));
    let prompt = app.rdp_credentials.as_ref().expect("RDP credentials modal");
    assert_eq!(prompt.user, "");
    assert_eq!(prompt.selected, crate::tui::RdpCredentialsField::User);
}
