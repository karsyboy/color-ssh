use super::*;
use crate::auth::vault::VaultPaths;
use std::fs;
use std::io;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::UnixListener;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_paths(prefix: &str) -> VaultPaths {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_nanos();
    let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    VaultPaths::new(std::env::temp_dir().join(format!("cossh_ipc_{prefix}_{nanos}_{serial}")))
}

#[test]
fn endpoint_derivation_is_deterministic() {
    let paths = temp_paths("deterministic");
    let left = agent_endpoint(&paths);
    let right = agent_endpoint(&paths);

    assert_eq!(left.identifier, right.identifier);
    #[cfg(unix)]
    assert_eq!(left.socket_path, right.socket_path);
}

#[test]
fn different_base_dirs_produce_different_endpoints() {
    let left = temp_paths("left");
    let right = temp_paths("right");

    assert_ne!(agent_endpoint(&left).identifier, agent_endpoint(&right).identifier);
}

#[test]
fn bind_listener_ignores_and_removes_legacy_state_file() {
    #[cfg(unix)]
    if !unix_socket_bind_allowed() {
        return;
    }

    let paths = temp_paths("legacy_state");
    fs::create_dir_all(paths.run_dir()).expect("create run dir");
    fs::write(legacy_state_file_path(&paths), b"legacy").expect("write legacy state");

    let listener = match bind_listener(&paths).expect("bind listener") {
        ListenerBindResult::Bound(listener) => listener,
        ListenerBindResult::AlreadyRunning => panic!("unexpected existing listener"),
    };

    assert!(!legacy_state_file_path(&paths).exists());
    drop(listener);
    #[cfg(unix)]
    let _ = fs::remove_file(agent_endpoint(&paths).socket_path);
    let _ = fs::remove_dir_all(paths.base_dir());
}

#[test]
fn read_write_json_line_round_trip() {
    let mut buffer = Vec::new();
    let request = AgentRequest {
        payload: AgentRequestPayload::GetSecret { name: "edge".to_string() },
    };

    write_json_line(&mut buffer, &request).expect("write json line");
    let mut reader = io::Cursor::new(buffer);
    let decoded: AgentRequest = read_json_line(&mut reader).expect("read json line");

    assert_eq!(decoded, request);
}

#[cfg(unix)]
fn unix_socket_bind_allowed() -> bool {
    let paths = temp_paths("socket_probe");
    fs::create_dir_all(paths.run_dir()).expect("create run dir for socket probe");
    let path = paths.run_dir().join("probe.sock");
    let allowed = match UnixListener::bind(&path) {
        Ok(listener) => {
            drop(listener);
            true
        }
        Err(err) if matches!(err.kind(), io::ErrorKind::PermissionDenied | io::ErrorKind::Unsupported) => false,
        Err(err) => panic!("unexpected unix socket probe failure: {err}"),
    };
    let _ = fs::remove_file(path);
    let _ = fs::remove_dir_all(paths.base_dir());
    allowed
}

#[cfg(unix)]
#[test]
fn unix_socket_round_trip_uses_run_dir_and_private_mode() {
    if !unix_socket_bind_allowed() {
        return;
    }

    let paths = temp_paths("unix_round_trip");
    let endpoint = agent_endpoint(&paths);
    let listener = match bind_listener(&paths).expect("bind listener") {
        ListenerBindResult::Bound(listener) => listener,
        ListenerBindResult::AlreadyRunning => panic!("unexpected existing listener"),
    };

    assert!(endpoint.socket_path.starts_with(paths.run_dir()));
    let mode = fs::metadata(&endpoint.socket_path).expect("socket metadata").permissions().mode() & 0o777;
    assert_eq!(mode, UNIX_SOCKET_MODE);

    let _client = connect(&paths).expect("connect to listener");

    drop(listener);
    let _ = fs::remove_file(endpoint.socket_path);
    let _ = fs::remove_dir_all(paths.base_dir());
}

#[cfg(unix)]
#[test]
fn unix_stale_socket_file_is_reclaimed() {
    if !unix_socket_bind_allowed() {
        return;
    }

    let paths = temp_paths("stale_socket");
    fs::create_dir_all(paths.run_dir()).expect("create run dir");
    set_restrictive_directory_permissions(&paths.run_dir()).expect("restrict run dir");
    let endpoint = agent_endpoint(&paths);

    let stale_listener = UnixListener::bind(&endpoint.socket_path).expect("create stale socket");
    drop(stale_listener);

    let listener = match bind_listener(&paths).expect("bind listener after stale socket") {
        ListenerBindResult::Bound(listener) => listener,
        ListenerBindResult::AlreadyRunning => panic!("unexpected existing listener"),
    };

    assert!(endpoint.socket_path.exists());

    drop(listener);
    let _ = fs::remove_file(endpoint.socket_path);
    let _ = fs::remove_dir_all(paths.base_dir());
}

#[cfg(unix)]
#[test]
fn unix_active_listener_is_not_replaced() {
    if !unix_socket_bind_allowed() {
        return;
    }

    let paths = temp_paths("active_listener");
    let listener = match bind_listener(&paths).expect("bind listener") {
        ListenerBindResult::Bound(listener) => listener,
        ListenerBindResult::AlreadyRunning => panic!("unexpected existing listener"),
    };

    let second = bind_listener(&paths).expect("second bind");
    assert!(matches!(second, ListenerBindResult::AlreadyRunning));

    drop(listener);
    let _ = fs::remove_file(agent_endpoint(&paths).socket_path);
    let _ = fs::remove_dir_all(paths.base_dir());
}

#[cfg(windows)]
#[test]
fn windows_endpoint_name_is_deterministic() {
    let paths = temp_paths("windows_name");
    let left = agent_endpoint(&paths);
    let right = agent_endpoint(&paths);
    assert_eq!(left.identifier, right.identifier);
}

#[cfg(windows)]
#[test]
fn windows_listener_accepts_same_user_connections() {
    let paths = temp_paths("windows_round_trip");
    let listener = match bind_listener(&paths).expect("bind listener") {
        ListenerBindResult::Bound(listener) => listener,
        ListenerBindResult::AlreadyRunning => panic!("unexpected existing listener"),
    };
    let _client = connect(&paths).expect("connect to listener");
    drop(listener);
}
