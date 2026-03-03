use super::*;
use crate::auth::ipc::{AgentRequest, AgentRequestPayload, UnlockPolicy};
use crate::auth::secret::{ExposeSecret, sensitive_string};
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
    assert!(runtime.expire_if_needed());
    assert!(runtime.data_key.is_none());
}

#[test]
fn runtime_does_not_expire_before_timeout() {
    let mut runtime = AgentRuntime::new();
    runtime.unlock([7u8; 32], UnlockPolicy::new(10, 20));
    assert!(!runtime.expire_if_needed());
    assert!(runtime.data_key.is_some());
}

#[test]
fn runtime_status_reports_absolute_timeout_timestamp() {
    let paths = temp_paths("status_absolute_timeout");
    let mut runtime = AgentRuntime::new();
    let before = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_secs();

    runtime.unlock([7u8; 32], UnlockPolicy::new(900, 3_600));

    let status = runtime.status(&paths);
    let absolute_timeout_at = status.absolute_timeout_at_epoch_seconds.expect("absolute timeout timestamp");

    assert_eq!(status.absolute_timeout_seconds, Some(3_600));
    assert!(absolute_timeout_at >= before + 3_595);
    assert!(absolute_timeout_at <= before + 3_605);
}

#[test]
fn runtime_status_keeps_absolute_timeout_timestamp_stable() {
    let paths = temp_paths("stable_absolute_timeout");
    let mut runtime = AgentRuntime::new();
    let fixed_timeout_at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);

    runtime.unlock([7u8; 32], UnlockPolicy::new(900, 3_600));
    runtime.absolute_timeout_at = Some(fixed_timeout_at);
    runtime.unlocked_at = Some(Instant::now() - Duration::from_secs(17));

    let first = runtime.status(&paths);
    let second = runtime.status(&paths);

    assert_eq!(first.absolute_timeout_at_epoch_seconds, Some(1_700_000_000));
    assert_eq!(second.absolute_timeout_at_epoch_seconds, Some(1_700_000_000));
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
            master_password: sensitive_string("master-pass"),
            policy: UnlockPolicy::new(900, 28_800),
        },
    };
    let unlock_response = handle_request(&paths, &mut runtime, unlock);
    assert!(matches!(unlock_response, AgentResponse::Success { .. }));

    let authorize = AgentRequest {
        payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
    };
    let token = match handle_request(&paths, &mut runtime, authorize) {
        AgentResponse::AskpassAuthorized { token, .. } => token,
        other => panic!("unexpected authorize response: {other:?}"),
    };
    let get_secret = AgentRequest {
        payload: AgentRequestPayload::GetSecret { token },
    };
    let response = handle_request(&paths, &mut runtime, get_secret);
    assert!(matches!(response, AgentResponse::Secret { secret, .. } if secret.expose_secret() == "top-secret"));

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn askpass_tokens_are_single_use() {
    let paths = temp_paths("single_use_token");
    initialize_vault_with_paths(&paths, "master-pass").expect("initialize vault");
    let unlocked = vault::unlock_with_password_and_paths(&paths, "master-pass").expect("unlock vault");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    let mut runtime = AgentRuntime::new();
    runtime.unlock(unlocked.data_key_copy(), UnlockPolicy::new(900, 28_800));

    let token = match handle_request(
        &paths,
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
        },
    ) {
        AgentResponse::AskpassAuthorized { token, .. } => token,
        other => panic!("unexpected authorize response: {other:?}"),
    };

    let first = handle_request(
        &paths,
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::GetSecret { token: token.clone() },
        },
    );
    assert!(matches!(first, AgentResponse::Secret { .. }));

    let second = handle_request(
        &paths,
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::GetSecret { token },
        },
    );
    assert!(matches!(second, AgentResponse::Error { code, .. } if code == "invalid_or_expired_askpass_token"));

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn runtime_expiry_clears_outstanding_askpass_tokens() {
    let paths = temp_paths("expired_token");
    initialize_vault_with_paths(&paths, "master-pass").expect("initialize vault");
    let unlocked = vault::unlock_with_password_and_paths(&paths, "master-pass").expect("unlock vault");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    let mut runtime = AgentRuntime::new();
    runtime.unlock([5u8; 32], UnlockPolicy::new(1, 1));
    let token = runtime.issue_askpass_token("shared").expect("issue askpass token");
    runtime.last_activity_at = Some(Instant::now() - Duration::from_secs(2));

    assert!(runtime.expire_if_needed());
    assert!(runtime.askpass_leases.is_empty());

    let response = handle_request(
        &paths,
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::GetSecret { token },
        },
    );
    assert!(matches!(response, AgentResponse::Error { code, .. } if code == "locked"));

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn authorize_askpass_requires_an_unlocked_runtime() {
    let paths = temp_paths("authorize_locked");
    initialize_vault_with_paths(&paths, "master-pass").expect("initialize vault");

    let mut runtime = AgentRuntime::new();
    let response = handle_request(
        &paths,
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
        },
    );

    assert!(matches!(response, AgentResponse::Error { code, .. } if code == "locked"));

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn handle_request_entry_status_reports_existing_entries_while_locked() {
    let paths = temp_paths("entry_status");
    initialize_vault_with_paths(&paths, "master-pass").expect("initialize vault");
    let unlocked = vault::unlock_with_password_and_paths(&paths, "master-pass").expect("unlock vault");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    let mut runtime = AgentRuntime::new();
    let response = handle_request(
        &paths,
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::EntryStatus { name: "shared".to_string() },
        },
    );

    assert!(matches!(response, AgentResponse::EntryStatus { exists, status, .. } if exists && !status.unlocked));

    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn handle_request_lock_clears_runtime_state() {
    let paths = temp_paths("lock_request");
    let mut runtime = AgentRuntime::new();
    runtime.unlock([9u8; 32], UnlockPolicy::new(900, 28_800));

    let response = handle_request(
        &paths,
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::Lock,
        },
    );

    assert!(matches!(response, AgentResponse::Success { .. }));
    assert!(runtime.data_key.is_none());
    assert!(runtime.unlocked_at.is_none());
    assert!(runtime.last_activity_at.is_none());
    assert!(runtime.absolute_timeout_at.is_none());
    assert!(runtime.policy.is_none());

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
        map_remote_error("invalid_or_expired_askpass_token", "expired".to_string()),
        AgentError::InvalidOrExpiredAskpassToken
    ));
    assert!(matches!(
        map_remote_error("vault_not_initialized", "missing".to_string()),
        AgentError::VaultNotInitialized
    ));
    assert!(matches!(map_remote_error("entry_not_found", "missing".to_string()), AgentError::EntryNotFound));
    assert!(matches!(map_remote_error("vault_error", "oops".to_string()), AgentError::Protocol(_)));
}
