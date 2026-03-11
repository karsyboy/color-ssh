use super::{AgentError, map_remote_error};

#[test]
fn map_remote_error_maps_known_codes() {
    assert!(matches!(map_remote_error("locked", "locked".to_string()), AgentError::Locked));
    assert!(matches!(
        map_remote_error("invalid_master_password", "bad".to_string()),
        AgentError::InvalidMasterPassword
    ));
    assert!(matches!(map_remote_error("vault_error", "oops".to_string()), AgentError::Protocol(_)));
}
