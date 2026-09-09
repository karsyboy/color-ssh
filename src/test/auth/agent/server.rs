use super::{handle_request, run_server_with_paths};
use crate::auth::agent::runtime::AgentRuntime;
use crate::auth::ipc::{self, AgentPeerTrust, AgentRequest, AgentRequestPayload, AgentResponse, UnlockPolicy, VaultStatusEventKind};
use crate::auth::secret::{ExposeSecret, sensitive_string};
use crate::test::support::auth::TestVaultEnv;
use std::io::Write;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn trickling_client_does_not_block_other_requests_or_expiry() {
    let env = TestVaultEnv::new("trickling_client");
    env.init("master-pass");

    let server_paths = env.paths().clone();
    let (server_tx, server_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let _ = server_tx.send(run_server_with_paths(server_paths));
    });

    let startup_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match ipc::connect(env.paths()) {
            Ok(stream) => {
                drop(stream);
                break;
            }
            Err(_) if Instant::now() < startup_deadline => thread::sleep(Duration::from_millis(10)),
            Err(err) => panic!("agent did not start: {err}"),
        }
    }

    let unlocked = ipc::send_request(
        env.paths(),
        &AgentRequestPayload::Unlock {
            master_password: sensitive_string("master-pass"),
            policy: UnlockPolicy::new(10, 2),
        },
    )
    .expect("unlock agent");
    assert!(unlocked.status().unlocked);
    let unlocked_at = Instant::now();

    let mut partial = ipc::connect(env.paths()).expect("connect partial client");
    partial.write_all(b"{").expect("write partial request");
    let stop_trickling = Arc::new(AtomicBool::new(false));
    let trickle_stop = Arc::clone(&stop_trickling);
    let mut trickle = Some(thread::spawn(move || {
        while !trickle_stop.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(200));
            if partial.write_all(b" ").is_err() {
                break;
            }
        }
    }));
    thread::sleep(Duration::from_millis(100));

    let request_paths = env.paths().clone();
    let (request_tx, request_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = request_tx.send(ipc::send_request(&request_paths, &AgentRequestPayload::Status));
    });

    let competing = request_rx.recv_timeout(Duration::from_secs(1));
    if competing.is_err() {
        stop_trickling.store(true, Ordering::Relaxed);
        trickle.take().expect("trickling client handle").join().expect("join trickling client");
    }

    let server_result = server_rx.recv_timeout(Duration::from_secs(3));
    stop_trickling.store(true, Ordering::Relaxed);
    if let Some(trickle) = trickle {
        trickle.join().expect("join trickling client");
    }
    if server_result.is_err() {
        let _ = server_rx.recv_timeout(Duration::from_secs(2));
    }
    server.join().expect("join agent server");

    let response = competing
        .expect("a partial client must not block a subsequent request")
        .expect("status request should succeed");
    assert!(matches!(response, AgentResponse::Status { .. }));
    assert!(unlocked_at.elapsed() < Duration::from_secs(3));
    server_result
        .expect("agent should expire while partial client remains connected")
        .expect("agent server should exit cleanly");

    let event = ipc::read_vault_status_event(env.paths()).expect("read expiry event");
    assert_eq!(event.kind, VaultStatusEventKind::Locked);
    assert!(!event.status.unlocked);
}

#[test]
fn handle_request_unlock_authorize_and_get_secret_happy_path() {
    let env = TestVaultEnv::new("unlock_fetch");
    let unlocked = env.init_and_unlock("master-pass");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

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
        AgentPeerTrust::TrustedCossh,
    );
    assert!(matches!(unlock, AgentResponse::Success { .. }));

    let token = match handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
        },
        AgentPeerTrust::TrustedCossh,
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
        AgentPeerTrust::TrustedCossh,
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
        AgentPeerTrust::TrustedCossh,
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
        AgentPeerTrust::TrustedCossh,
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
        AgentPeerTrust::TrustedCossh,
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
        AgentPeerTrust::TrustedCossh,
    );
    assert!(matches!(first, AgentResponse::Secret { .. }));

    let second = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::GetSecret { token },
        },
        AgentPeerTrust::TrustedCossh,
    );
    assert!(matches!(second, AgentResponse::Error { code, .. } if code == "invalid_or_expired_askpass_token"));

    let locked_authorize = handle_request(
        env.paths(),
        &mut AgentRuntime::new(),
        AgentRequest {
            payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
        },
        AgentPeerTrust::TrustedCossh,
    );
    assert!(matches!(locked_authorize, AgentResponse::Error { code, .. } if code == "locked"));
}

#[test]
fn secret_bearing_requests_require_trusted_peer() {
    let env = TestVaultEnv::new("peer_trust_required");
    let unlocked = env.init_and_unlock("master-pass");
    unlocked.store_secret("shared", "top-secret").expect("store secret");

    let mut runtime = AgentRuntime::new();
    runtime.unlock(unlocked.data_key_copy(), UnlockPolicy::new(900, 28_800));

    let untrusted_authorize = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
        },
        AgentPeerTrust::Untrusted,
    );
    assert!(matches!(untrusted_authorize, AgentResponse::Error { code, .. } if code == "unauthorized_client"));

    let token = match handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::AuthorizeAskpass { name: "shared".to_string() },
        },
        AgentPeerTrust::TrustedCossh,
    ) {
        AgentResponse::AskpassAuthorized { token, .. } => token,
        other => panic!("unexpected authorize response: {other:?}"),
    };

    let untrusted_get = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::GetSecret { token: token.clone() },
        },
        AgentPeerTrust::Untrusted,
    );
    assert!(matches!(untrusted_get, AgentResponse::Error { code, .. } if code == "unauthorized_client"));

    let trusted_get = handle_request(
        env.paths(),
        &mut runtime,
        AgentRequest {
            payload: AgentRequestPayload::GetSecret { token },
        },
        AgentPeerTrust::TrustedCossh,
    );
    assert!(matches!(trusted_get, AgentResponse::Secret { secret, .. } if secret.expose_secret() == "top-secret"));
}
