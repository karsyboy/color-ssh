//! Local IPC protocol used by the password-vault unlock agent.

use crate::auth::secret::{SensitiveString, serde_sensitive_string};
use crate::auth::vault::VaultPaths;
use crate::log_debug;
use interprocess::local_socket::{GenericFilePath, ToFsName};
use interprocess::local_socket::{Listener as LocalSocketListener, ListenerNonblockingMode, ListenerOptions, Stream as LocalSocketStream, prelude::*};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

const AGENT_ENDPOINT_PREFIX: &str = "cossh-agent-v2-";
const LEGACY_AGENT_STATE_FILENAME: &str = "agent-state.json";
const VAULT_STATUS_EVENT_FILENAME: &str = "vault-events";
const UNIX_SOCKET_MODE: u32 = 0o600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentEndpoint {
    identifier: String,
    socket_path: PathBuf,
}

impl AgentEndpoint {
    fn debug_label(&self) -> &str {
        &self.identifier
    }
}

#[derive(Debug)]
/// Result of trying to bind the agent socket listener.
pub enum ListenerBindResult {
    /// Listener successfully bound on this process.
    Bound(LocalSocketListener),
    /// Another live agent already owns the endpoint.
    AlreadyRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Unlock timeout policy sent to the agent.
pub struct UnlockPolicy {
    /// Idle timeout after which the vault is re-locked.
    pub idle_timeout_seconds: u64,
    /// Absolute unlock lifetime cap.
    pub session_timeout_seconds: u64,
}

impl UnlockPolicy {
    /// Build a new unlock policy.
    pub fn new(idle_timeout_seconds: u64, session_timeout_seconds: u64) -> Self {
        Self {
            idle_timeout_seconds,
            session_timeout_seconds,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Current vault state reported by the agent.
pub struct VaultStatus {
    /// Whether the vault metadata exists on disk.
    pub vault_exists: bool,
    /// Whether the vault is currently unlocked in the agent.
    pub unlocked: bool,
    /// Remaining unlock time in seconds, if unlocked.
    pub unlock_expires_in_seconds: Option<u64>,
    /// Effective idle timeout for the current session.
    pub idle_timeout_seconds: Option<u64>,
    /// Effective absolute timeout for the current session.
    pub absolute_timeout_seconds: Option<u64>,
    /// Absolute timeout wall-clock epoch, if available.
    pub absolute_timeout_at_epoch_seconds: Option<u64>,
}

impl VaultStatus {
    /// Build a locked-status snapshot.
    pub fn locked(vault_exists: bool) -> Self {
        Self {
            vault_exists,
            unlocked: false,
            unlock_expires_in_seconds: None,
            idle_timeout_seconds: None,
            absolute_timeout_seconds: None,
            absolute_timeout_at_epoch_seconds: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Emitted vault status transition kind.
pub enum VaultStatusEventKind {
    /// Vault transitioned to locked.
    Locked,
    /// Vault transitioned to unlocked.
    Unlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Broadcast event stored in the status event file.
pub struct VaultStatusEvent {
    /// Transition kind.
    pub kind: VaultStatusEventKind,
    /// Vault status snapshot at event time.
    pub status: VaultStatus,
    /// Monotonic-ish event id derived from timestamp nanos.
    pub event_id: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
/// Request payload sent from clients to the unlock agent.
pub enum AgentRequestPayload {
    Status,
    Unlock {
        #[serde(with = "serde_sensitive_string")]
        master_password: SensitiveString,
        policy: UnlockPolicy,
    },
    AuthorizeAskpass {
        name: String,
    },
    EntryStatus {
        name: String,
    },
    GetSecret {
        #[serde(with = "serde_sensitive_string")]
        token: SensitiveString,
    },
    Lock,
}

impl AgentRequestPayload {
    /// Stable debug label for logging request flow.
    pub fn debug_name(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Unlock { .. } => "unlock",
            Self::AuthorizeAskpass { .. } => "authorize_askpass",
            Self::EntryStatus { .. } => "entry_status",
            Self::GetSecret { .. } => "get_secret",
            Self::Lock => "lock",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Top-level client request wrapper.
pub struct AgentRequest {
    /// Request payload.
    pub payload: AgentRequestPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
/// Response envelope returned by the unlock agent.
pub enum AgentResponse {
    Status {
        status: VaultStatus,
    },
    EntryStatus {
        status: VaultStatus,
        name: String,
        exists: bool,
    },
    AskpassAuthorized {
        status: VaultStatus,
        #[serde(with = "serde_sensitive_string")]
        token: SensitiveString,
    },
    Secret {
        status: VaultStatus,
        name: String,
        #[serde(with = "serde_sensitive_string")]
        secret: SensitiveString,
    },
    Success {
        status: VaultStatus,
        message: String,
    },
    Error {
        status: VaultStatus,
        code: String,
        message: String,
    },
}

impl AgentResponse {
    /// Borrow the status snapshot included in any response variant.
    pub fn status(&self) -> &VaultStatus {
        match self {
            Self::Status { status }
            | Self::EntryStatus { status, .. }
            | Self::AskpassAuthorized { status, .. }
            | Self::Secret { status, .. }
            | Self::Success { status, .. }
            | Self::Error { status, .. } => status,
        }
    }
}

#[derive(Serialize)]
struct AgentRequestRef<'a> {
    payload: &'a AgentRequestPayload,
}

/// Bind the unlock-agent local socket listener.
pub fn bind_listener(paths: &VaultPaths) -> io::Result<ListenerBindResult> {
    remove_legacy_state_file(paths);
    log_debug!("Binding password vault agent endpoint");
    match create_listener(paths) {
        Ok(listener) => Ok(ListenerBindResult::Bound(listener)),
        Err(err) if is_address_in_use(&err) => handle_bind_conflict(paths, err),
        Err(err) => Err(err),
    }
}

/// Send one request and wait for one response.
pub fn send_request(paths: &VaultPaths, payload: &AgentRequestPayload) -> io::Result<AgentResponse> {
    log_debug!("Opening IPC request '{}' to password vault agent", payload.debug_name());
    let mut stream = connect(paths)?;
    let request = AgentRequestRef { payload };
    write_json_line(&mut stream, &request)?;
    read_json_line(&mut stream)
}

/// Connect directly to the current agent endpoint.
pub fn connect(paths: &VaultPaths) -> io::Result<LocalSocketStream> {
    let endpoint = agent_endpoint(paths);
    log_debug!("Connecting to password vault agent endpoint '{}'", endpoint.debug_label());
    let stream = connect_to_endpoint(&endpoint)?;
    remove_legacy_state_file(paths);
    Ok(stream)
}

/// Remove endpoint resources used by the unlock agent.
pub fn cleanup_endpoint(paths: &VaultPaths) -> io::Result<()> {
    log_debug!("Cleaning password vault agent endpoint resources");
    remove_legacy_state_file(paths);
    cleanup_local_endpoint(paths)
}

/// Persist a vault status event for local consumers.
pub fn broadcast_vault_status_event(paths: &VaultPaths, kind: VaultStatusEventKind, status: VaultStatus) -> io::Result<()> {
    let run_dir = paths.run_dir();
    fs::create_dir_all(&run_dir)?;
    set_restrictive_directory_permissions(&run_dir)?;

    let event_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| io::Error::other(format!("failed to derive vault status event timestamp: {err}")))?
        .as_nanos();
    let path = vault_status_event_file_path(paths);
    let event = VaultStatusEvent { kind, status, event_id };
    let bytes = serde_json::to_vec(&event).map_err(|err| io::Error::other(format!("failed to serialize vault status event: {err}")))?;
    fs::write(&path, bytes)?;
    set_restrictive_file_permissions(&path)?;
    Ok(())
}

/// Read the latest persisted vault status event.
pub fn read_vault_status_event(paths: &VaultPaths) -> io::Result<VaultStatusEvent> {
    let path = vault_status_event_file_path(paths);
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|err| io::Error::other(format!("failed to parse vault status event: {err}")))
}

/// Read one IPC request from a connected stream.
pub fn read_request(stream: &mut LocalSocketStream) -> io::Result<AgentRequest> {
    read_json_line(stream)
}

/// Write one IPC response to a connected stream.
pub fn write_response(stream: &mut LocalSocketStream, response: &AgentResponse) -> io::Result<()> {
    write_json_line(stream, response)
}

fn is_address_in_use(err: &io::Error) -> bool {
    matches!(err.kind(), io::ErrorKind::AddrInUse | io::ErrorKind::AlreadyExists)
}

fn handle_bind_conflict(paths: &VaultPaths, original_err: io::Error) -> io::Result<ListenerBindResult> {
    if connect(paths).is_ok() {
        log_debug!("Password vault agent endpoint already has a live server");
        return Ok(ListenerBindResult::AlreadyRunning);
    }

    if remove_stale_socket_file(paths)? {
        log_debug!("Removed stale password vault agent socket file; retrying bind");
        return match create_listener(paths) {
            Ok(listener) => Ok(ListenerBindResult::Bound(listener)),
            Err(err) if is_address_in_use(&err) && connect(paths).is_ok() => Ok(ListenerBindResult::AlreadyRunning),
            Err(err) => Err(err),
        };
    }

    Err(original_err)
}

fn create_listener(paths: &VaultPaths) -> io::Result<LocalSocketListener> {
    let endpoint = agent_endpoint(paths);
    create_listener_for_endpoint(paths, &endpoint)
}

fn write_json_line<T: Serialize, W: Write>(stream: &mut W, value: &T) -> io::Result<()> {
    let mut bytes = Zeroizing::new(serde_json::to_vec(value).map_err(|err| io::Error::other(format!("failed to serialize IPC message: {err}")))?);
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    stream.flush()
}

fn read_json_line<T: for<'de> Deserialize<'de>, R: Read>(stream: &mut R) -> io::Result<T> {
    let mut reader = BufReader::new(stream);
    let mut line = Zeroizing::new(Vec::new());
    reader.read_until(b'\n', &mut line)?;
    serde_json::from_slice(&line).map_err(|err| io::Error::other(format!("failed to parse IPC message: {err}")))
}

fn agent_endpoint(paths: &VaultPaths) -> AgentEndpoint {
    let identifier = format!("{AGENT_ENDPOINT_PREFIX}{:016x}", fnv1a_64(endpoint_seed(paths).as_bytes()));
    AgentEndpoint {
        socket_path: paths.run_dir().join(format!("{identifier}.sock")),
        identifier,
    }
}

pub(crate) fn vault_status_event_file_path(paths: &VaultPaths) -> PathBuf {
    paths.run_dir().join(VAULT_STATUS_EVENT_FILENAME)
}

fn endpoint_seed(paths: &VaultPaths) -> String {
    canonical_base_dir(paths)
        .unwrap_or_else(|| absolute_base_dir(paths.base_dir()))
        .to_string_lossy()
        .into_owned()
}

fn canonical_base_dir(paths: &VaultPaths) -> Option<PathBuf> {
    fs::canonicalize(paths.base_dir()).ok()
}

fn absolute_base_dir(base_dir: &Path) -> PathBuf {
    if base_dir.is_absolute() {
        return base_dir.to_path_buf();
    }

    match std::env::current_dir() {
        Ok(current_dir) => current_dir.join(base_dir),
        Err(_) => base_dir.to_path_buf(),
    }
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn remove_legacy_state_file(paths: &VaultPaths) {
    let path = legacy_state_file_path(paths);
    if path.exists() {
        log_debug!("Removing obsolete password vault agent state file '{}'", path.display());
        let _ = fs::remove_file(path);
    }
}

fn legacy_state_file_path(paths: &VaultPaths) -> PathBuf {
    paths.run_dir().join(LEGACY_AGENT_STATE_FILENAME)
}

fn create_listener_for_endpoint(paths: &VaultPaths, endpoint: &AgentEndpoint) -> io::Result<LocalSocketListener> {
    fs::create_dir_all(paths.run_dir())?;
    set_restrictive_directory_permissions(&paths.run_dir())?;
    let name = endpoint.socket_path.as_os_str().to_fs_name::<GenericFilePath>()?;
    let listener = ListenerOptions::new().name(name).nonblocking(ListenerNonblockingMode::Accept).create_sync()?;
    set_restrictive_file_permissions(&endpoint.socket_path)?;
    Ok(listener)
}

fn connect_to_endpoint(endpoint: &AgentEndpoint) -> io::Result<LocalSocketStream> {
    let name = endpoint.socket_path.as_os_str().to_fs_name::<GenericFilePath>()?;
    LocalSocketStream::connect(name)
}

fn remove_stale_socket_file(paths: &VaultPaths) -> io::Result<bool> {
    let endpoint = agent_endpoint(paths);
    let socket_path = endpoint.socket_path;
    if !socket_path.exists() {
        return Ok(false);
    }

    let metadata = fs::symlink_metadata(&socket_path)?;
    if metadata.file_type().is_socket() {
        fs::remove_file(socket_path)?;
        return Ok(true);
    }

    Ok(false)
}

fn cleanup_local_endpoint(paths: &VaultPaths) -> io::Result<()> {
    let endpoint = agent_endpoint(paths);
    if !endpoint.socket_path.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(&endpoint.socket_path)?;
    if metadata.file_type().is_socket() {
        fs::remove_file(endpoint.socket_path)?;
    }

    Ok(())
}

fn set_restrictive_directory_permissions(path: &Path) -> io::Result<()> {
    crate::platform::set_private_directory_permissions(path, 0o700)
}

fn set_restrictive_file_permissions(path: &Path) -> io::Result<()> {
    crate::platform::set_private_file_permissions(path, UNIX_SOCKET_MODE)
}

#[cfg(test)]
#[path = "../test/auth/ipc.rs"]
mod tests;
