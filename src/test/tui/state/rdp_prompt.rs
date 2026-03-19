use super::*;
use crate::auth::secret::ExposeSecret;
use crate::inventory::ConnectionProtocol;

fn sample_host() -> InventoryHost {
    let mut host = InventoryHost::new("desktop01".to_string());
    host.protocol = ConnectionProtocol::Rdp;
    host.host = "rdp.internal".to_string();
    host
}

#[test]
fn build_submission_requires_username() {
    let host = sample_host();
    let state = RdpCredentialsState::new(
        &host,
        RdpCredentialsAction::OpenHostTab {
            host: Box::new(host.clone()),
            force_ssh_logging: false,
            launch_context: RdpCredentialLaunchContext {
                pass_entry_override: None,
                pass_fallback_notice: None,
                disable_vault_autologin: false,
            },
        },
        None,
    );

    let err = state.build_submission(&host).expect_err("username should be required");
    assert_eq!(err, RdpCredentialValidationError::UserRequired);
}

#[test]
fn build_submission_applies_domain_port_and_password() {
    let host = sample_host();
    let mut state = RdpCredentialsState::new(
        &host,
        RdpCredentialsAction::OpenHostTab {
            host: Box::new(host.clone()),
            force_ssh_logging: false,
            launch_context: RdpCredentialLaunchContext {
                pass_entry_override: None,
                pass_fallback_notice: None,
                disable_vault_autologin: false,
            },
        },
        None,
    );
    state.user = "alice".to_string();
    state.user_cursor = 5;
    state.domain = "ACME".to_string();
    state.domain_cursor = 4;
    state.port = "3390".to_string();
    state.port_cursor = 4;
    state.password.insert_char(0, 's');
    state.password.insert_char(1, 'e');
    state.password.insert_char(2, 'c');
    state.password.insert_char(3, 'r');
    state.password.insert_char(4, 'e');
    state.password.insert_char(5, 't');
    state.password_cursor = 6;

    let submission = state.build_submission(&host).expect("submission");

    assert_eq!(submission.host.user.as_deref(), Some("alice"));
    assert_eq!(submission.host.rdp.domain.as_deref(), Some("ACME"));
    assert_eq!(submission.host.port, Some(3390));
    assert_eq!(submission.manual_password.expect("password").expose_secret(), "secret");
}

#[test]
fn ctrl_a_style_select_all_replaces_rdp_text_field() {
    let host = sample_host();
    let mut state = RdpCredentialsState::new(
        &host,
        RdpCredentialsAction::OpenHostTab {
            host: Box::new(host.clone()),
            force_ssh_logging: false,
            launch_context: RdpCredentialLaunchContext {
                pass_entry_override: None,
                pass_fallback_notice: None,
                disable_vault_autologin: false,
            },
        },
        None,
    );

    state.user = "alice".to_string();
    state.user_cursor = state.user.chars().count();
    state.select_all(RdpCredentialsField::User);
    state.insert_char(RdpCredentialsField::User, 'z');

    assert_eq!(state.user, "z");
    assert_eq!(state.user_cursor, 1);
    assert!(state.selection_for_field(RdpCredentialsField::User).is_none());
}
