use super::*;
use crate::auth::secret::ExposeSecret;

#[test]
fn build_submission_for_rdp_includes_domain_port_and_password() {
    let mut state = QuickConnectState::new(false, vec!["default".to_string(), "prod".to_string()]);
    state.protocol = crate::inventory::ConnectionProtocol::Rdp;
    state.user = "alice".to_string();
    state.host = "desktop01".to_string();
    state.port = "3390".to_string();
    state.domain = "ACME".to_string();
    state.profile_index = 1;
    state.password.insert_char(0, 's');
    state.password.insert_char(1, 'e');
    state.password.insert_char(2, 'c');
    state.password.insert_char(3, 'r');
    state.password.insert_char(4, 'e');
    state.password.insert_char(5, 't');
    state.password_cursor = 6;

    let submission = state.build_submission().expect("submission");

    assert!(matches!(submission.host.protocol, crate::inventory::ConnectionProtocol::Rdp));
    assert_eq!(submission.host.user.as_deref(), Some("alice"));
    assert_eq!(submission.host.host, "desktop01");
    assert_eq!(submission.host.port, Some(3390));
    assert_eq!(submission.host.rdp.domain.as_deref(), Some("ACME"));
    assert_eq!(submission.host.profile, None);
    assert_eq!(submission.manual_rdp_password.expect("password").expose_secret(), "secret");
}

#[test]
fn modal_rows_follow_selected_protocol() {
    let ssh_state = QuickConnectState::new(false, vec!["default".to_string()]);

    assert_eq!(
        ssh_state.modal_rows(),
        vec![
            QuickConnectRow::Field(QuickConnectField::Protocol),
            QuickConnectRow::Field(QuickConnectField::User),
            QuickConnectRow::Field(QuickConnectField::Host),
            QuickConnectRow::Field(QuickConnectField::Profile),
            QuickConnectRow::ProfileOptions,
            QuickConnectRow::Field(QuickConnectField::Logging),
            QuickConnectRow::Message,
            QuickConnectRow::Actions,
        ]
    );

    let mut rdp_state = QuickConnectState::new(false, vec!["default".to_string()]);
    rdp_state.protocol = crate::inventory::ConnectionProtocol::Rdp;

    assert_eq!(
        rdp_state.modal_rows(),
        vec![
            QuickConnectRow::Field(QuickConnectField::Protocol),
            QuickConnectRow::Field(QuickConnectField::User),
            QuickConnectRow::Field(QuickConnectField::Host),
            QuickConnectRow::Field(QuickConnectField::Port),
            QuickConnectRow::Field(QuickConnectField::Domain),
            QuickConnectRow::Field(QuickConnectField::Password),
            QuickConnectRow::Message,
            QuickConnectRow::Actions,
        ]
    );
}

#[test]
fn build_submission_for_rdp_requires_username() {
    let mut state = QuickConnectState::new(false, vec!["default".to_string()]);
    state.protocol = crate::inventory::ConnectionProtocol::Rdp;
    state.host = "desktop01".to_string();

    let err = state.build_submission().expect_err("username should be required");

    assert_eq!(err, QuickConnectValidationError::RdpUserRequired);
}

#[test]
fn build_submission_for_ssh_ignores_hidden_rdp_fields() {
    let mut state = QuickConnectState::new(true, vec!["default".to_string()]);
    state.user = "alice".to_string();
    state.host = "ssh.example.com".to_string();
    state.port = "bad-port".to_string();
    state.domain = "ACME".to_string();
    state.password.insert_char(0, 's');
    state.password.insert_char(1, 'e');
    state.password.insert_char(2, 'c');
    state.password.insert_char(3, 'r');
    state.password.insert_char(4, 'e');
    state.password.insert_char(5, 't');
    state.password_cursor = 6;

    let submission = state.build_submission().expect("ssh submission");

    assert!(matches!(submission.host.protocol, crate::inventory::ConnectionProtocol::Ssh));
    assert_eq!(submission.host.port, None);
    assert_eq!(submission.host.rdp.domain, None);
    assert!(submission.manual_rdp_password.is_none());
    assert!(submission.force_ssh_logging);
}
