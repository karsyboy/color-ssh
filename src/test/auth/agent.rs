use super::*;
use crate::auth::ipc::{AgentRequest, AgentRequestPayload, UnlockPolicy};
use crate::auth::vault::{VaultPaths, initialize_vault_with_paths};
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_paths(prefix: &str) -> VaultPaths {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_nanos();
    let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    VaultPaths::new(std::env::temp_dir().join(format!("cossh_agent_{prefix}_{nanos}_{serial}")))
}

#[test]
fn runtime_expires_when_idle_timeout_elapses() {
    let mut runtime = AgentRuntime::new();
    runtime.unlock([7u8; 32], UnlockPolicy::new(1, 10));
    runtime.last_activity_at = Some(Instant::now() - Duration::from_secs(2));
    runtime.expire_if_needed();
    assert!(runtime.data_key.is_none());
}

#[test]
fn handle_request_unlocks_and_fetches_secret() {
    let paths = temp_paths("unlock_fetch");
    initialize_vault_with_paths(&paths, "master-pass").expect("initialize vault");
    let unlocked = vault::unlock_with_password_and_paths(&paths, "master-pass").expect("unlock vault");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    let mut runtime = AgentRuntime::new();
    let unlock = AgentRequest {
        payload: AgentRequestPayload::Unlock {
            master_password: "master-pass".to_string(),
            policy: UnlockPolicy::new(900, 28_800),
        },
    };
    let unlock_response = handle_request(&paths, &mut runtime, unlock);
    assert!(matches!(unlock_response, AgentResponse::Success { .. }));

    let get_secret = AgentRequest {
        payload: AgentRequestPayload::GetSecret { name: "shared".to_string() },
    };
    let response = handle_request(&paths, &mut runtime, get_secret);
    assert!(matches!(response, AgentResponse::Secret { secret, .. } if secret == "top-secret"));

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn map_remote_error_preserves_expected_codes() {
    assert!(matches!(map_remote_error("locked", "locked".to_string()), AgentError::Locked));
    assert!(matches!(
        map_remote_error("invalid_master_password", "bad".to_string()),
        AgentError::InvalidMasterPassword
    ));
    assert!(matches!(
        map_remote_error("vault_not_initialized", "missing".to_string()),
        AgentError::VaultNotInitialized
    ));
    assert!(matches!(map_remote_error("entry_not_found", "missing".to_string()), AgentError::EntryNotFound));
    assert!(matches!(map_remote_error("vault_error", "oops".to_string()), AgentError::Protocol(_)));
}
