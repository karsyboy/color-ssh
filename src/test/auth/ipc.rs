use super::*;
use crate::auth::secret::sensitive_string;
use crate::test::support::auth::TestVaultEnv;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;

fn local_socket_bind_allowed() -> bool {
    let env = TestVaultEnv::new("socket_probe");
    fs::create_dir_all(env.paths().run_dir()).expect("create run dir for socket probe");

    let path = env.paths().run_dir().join("probe.sock");
    let allowed = match UnixListener::bind(&path) {
        Ok(listener) => {
            drop(listener);
            true
        }
        Err(err) if matches!(err.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::Unsupported) => false,
        Err(err) => panic!("unexpected unix socket probe failure: {err}"),
    };

    let _ = fs::remove_file(path);
    allowed
}

#[test]
fn endpoint_derivation_and_json_round_trip_are_stable() {
    let env = TestVaultEnv::new("deterministic");
    let left = agent_endpoint(env.paths());
    let right = agent_endpoint(env.paths());
    assert_eq!(left.identifier, right.identifier);
    assert_eq!(left.socket_path, right.socket_path);

    let mut buffer = Vec::new();
    let request = AgentRequest {
        payload: AgentRequestPayload::GetSecret {
            token: sensitive_string("edge-token"),
        },
    };
    write_json_line(&mut buffer, &request).expect("write json line");
    let mut reader = io::Cursor::new(buffer);
    let decoded: AgentRequest = read_json_line(&mut reader).expect("read json line");
    assert_eq!(decoded, request);
}

#[test]
fn secret_fields_are_redacted_in_debug_output() {
    let payload = AgentRequestPayload::Unlock {
        master_password: sensitive_string("master-pass"),
        policy: UnlockPolicy::new(900, 28_800),
    };
    let debug = format!("{payload:?}");
    assert!(!debug.contains("master-pass"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn listener_stale_socket_reclaim_and_active_listener_detection() {
    if !local_socket_bind_allowed() {
        return;
    }

    let env = TestVaultEnv::new("stale_and_active");
    fs::create_dir_all(env.paths().run_dir()).expect("create run dir");
    set_restrictive_directory_permissions(&env.paths().run_dir()).expect("restrict run dir");

    let endpoint = agent_endpoint(env.paths());
    let stale_listener = UnixListener::bind(&endpoint.socket_path).expect("create stale socket");
    drop(stale_listener);

    let listener = match bind_listener(env.paths()).expect("bind listener after stale socket") {
        ListenerBindResult::Bound(listener) => listener,
        ListenerBindResult::AlreadyRunning => panic!("unexpected existing listener"),
    };
    assert!(endpoint.socket_path.exists());

    let second = bind_listener(env.paths()).expect("second bind");
    assert!(matches!(second, ListenerBindResult::AlreadyRunning));

    drop(listener);
    let _ = fs::remove_file(endpoint.socket_path);
}

#[test]
fn local_socket_round_trip_uses_private_socket_permissions() {
    if !local_socket_bind_allowed() {
        return;
    }

    let env = TestVaultEnv::new("local_socket_round_trip");
    let endpoint = agent_endpoint(env.paths());

    let listener = match bind_listener(env.paths()).expect("bind listener") {
        ListenerBindResult::Bound(listener) => listener,
        ListenerBindResult::AlreadyRunning => panic!("unexpected existing listener"),
    };

    let mode = fs::metadata(&endpoint.socket_path).expect("socket metadata").permissions().mode() & 0o777;
    assert_eq!(mode, UNIX_SOCKET_MODE);

    let _client = connect(env.paths()).expect("connect to listener");

    drop(listener);
    let _ = fs::remove_file(endpoint.socket_path);
}
