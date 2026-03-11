use super::handle_request;
use crate::auth::agent::runtime::AgentRuntime;
use crate::auth::ipc::{self, AgentRequest, AgentRequestPayload, AgentResponse, UnlockPolicy, VaultStatusEventKind};
use crate::auth::secret::{ExposeSecret, sensitive_string};
use crate::test::support::auth::TestVaultEnv;

#[test]
fn handle_request_unlock_authorize_and_get_secret_happy_path() {
    let env = TestVaultEnv::new("unlock_fetch");
    let unlocked = env.init_and_unlock("master-pass");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    let mut runtime = AgentRuntime::new();

    let unlock_response = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::Unlock {
                master_password: sensitive_string("master-pass"),
                policy: UnlockPolicy::new(900, 28_800),
            },
        },
    );
    assert!(matches!(unlock_response, AgentResponse::Success { .. }));

    let token = match handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
        },
    ) {
        AgentResponse::AskpassAuthorized { token, .. } => token,
        other => panic!("unexpected authorize response: {other:?}"),
    };

    let response = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::GetSecret { token },
        },
    );

    assert!(matches!(response, AgentResponse::Secret { secret, .. } if secret.expose_secret() == "top-secret"));
}

#[test]
fn lock_and_unlock_requests_emit_status_events_and_clear_runtime_state() {
    let env = TestVaultEnv::new("lock_unlock_events");
    env.init("master-pass");

    let mut runtime = AgentRuntime::new();
    let unlock = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::Unlock {
                master_password: sensitive_string("master-pass"),
                policy: UnlockPolicy::new(900, 28_800),
            },
        },
    );
    assert!(matches!(unlock, AgentResponse::Success { .. }));
    assert_eq!(
        ipc::read_vault_status_event(env.paths()).expect("read unlocked event").kind,
        VaultStatusEventKind::Unlocked
    );

    let lock = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::Lock,
        },
    );
    assert!(matches!(lock, AgentResponse::Success { .. }));
    assert!(runtime.data_key.is_none());
    assert_eq!(
        ipc::read_vault_status_event(env.paths()).expect("read locked event").kind,
        VaultStatusEventKind::Locked
    );
}

#[test]
fn askpass_token_single_use_and_locked_runtime_errors() {
    let env = TestVaultEnv::new("single_use");
    let unlocked = env.init_and_unlock("master-pass");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    let mut runtime = AgentRuntime::new();
    runtime.unlock(unlocked.data_key_copy(), UnlockPolicy::new(900, 28_800));

    let token = match handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
        },
    ) {
        AgentResponse::AskpassAuthorized { token, .. } => token,
        other => panic!("unexpected authorize response: {other:?}"),
    };

    let first = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::GetSecret { token: token.clone() },
        },
    );
    assert!(matches!(first, AgentResponse::Secret { .. }));

    let second = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::GetSecret { token },
        },
    );
    assert!(matches!(second, AgentResponse::Error { code, .. } if code == "invalid_or_expired_askpass_token"));

    let locked_authorize = handle_request(
        env.paths(),
        &mut AgentRuntime::new(),
        AgentRequest {
            payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
        },
    );
    assert!(matches!(locked_authorize, AgentResponse::Error { code, .. } if code == "locked"));
}
