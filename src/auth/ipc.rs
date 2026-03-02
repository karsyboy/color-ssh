use crate::auth::secret::{SensitiveString, serde_sensitive_string};
use crate::auth::vault::VaultPaths;
use crate::log_debug;
#[cfg(unix)]
use interprocess::local_socket::{GenericFilePath, ToFsName};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName};
use interprocess::local_socket::{Listener as LocalSocketListener, ListenerNonblockingMode, ListenerOptions, Stream as LocalSocketStream, prelude::*};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[cfg(windows)]
use interprocess::os::windows::local_socket::ListenerOptionsExt as _;
#[cfg(windows)]
use interprocess::os::windows::security_descriptor::SecurityDescriptor;
#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, PermissionsExt};

#[cfg(windows)]
use widestring::U16CString;
#[cfg(windows)]
use windows_sys::Win32::Foundation::{GetLastError, PSID};
#[cfg(windows)]
use windows_sys::Win32::Security::{ConvertSidToStringSidW, GetTokenInformation, OpenProcessToken, TOKEN_QUERY, TOKEN_USER, TokenUser};
#[cfg(windows)]
use windows_sys::Win32::System::Memory::LocalFree;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::GetCurrentProcess;

const AGENT_ENDPOINT_PREFIX: &str = "cossh-agent-v1-";
const LEGACY_AGENT_STATE_FILENAME: &str = "agent-state.json";

#[cfg(unix)]
const UNIX_SOCKET_MODE: u32 = 0o600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentEndpoint {
    identifier: String,
    #[cfg(unix)]
    socket_path: PathBuf,
}

impl AgentEndpoint {
    fn debug_label(&self) -> &str {
        &self.identifier
    }
}

#[derive(Debug)]
pub enum ListenerBindResult {
    Bound(LocalSocketListener),
    AlreadyRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnlockPolicy {
    pub unlock_idle_timeout_seconds: u64,
    pub unlock_absolute_timeout_seconds: u64,
}

impl UnlockPolicy {
    pub fn new(unlock_idle_timeout_seconds: u64, unlock_absolute_timeout_seconds: u64) -> Self {
        Self {
            unlock_idle_timeout_seconds,
            unlock_absolute_timeout_seconds,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultStatus {
    pub vault_exists: bool,
    pub unlocked: bool,
    pub unlock_expires_in_seconds: Option<u64>,
    pub idle_timeout_seconds: Option<u64>,
    pub absolute_timeout_seconds: Option<u64>,
}

impl VaultStatus {
    pub fn locked(vault_exists: bool) -> Self {
        Self {
            vault_exists,
            unlocked: false,
            unlock_expires_in_seconds: None,
            idle_timeout_seconds: None,
            absolute_timeout_seconds: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentRequestPayload {
    Status,
    Unlock {
        #[serde(with = "serde_sensitive_string")]
        master_password: SensitiveString,
        policy: UnlockPolicy,
    },
    EntryStatus {
        name: String,
    },
    GetSecret {
        name: String,
    },
    Lock,
}

impl AgentRequestPayload {
    pub fn debug_name(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Unlock { .. } => "unlock",
            Self::EntryStatus { .. } => "entry_status",
            Self::GetSecret { .. } => "get_secret",
            Self::Lock => "lock",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRequest {
    pub payload: AgentRequestPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentResponse {
    Status {
        status: VaultStatus,
    },
    EntryStatus {
        status: VaultStatus,
        name: String,
        exists: bool,
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
    pub fn status(&self) -> &VaultStatus {
        match self {
            Self::Status { status }
            | Self::EntryStatus { status, .. }
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

pub fn bind_listener(paths: &VaultPaths) -> io::Result<ListenerBindResult> {
    remove_legacy_state_file(paths);
    log_debug!("Binding password vault agent endpoint");
    match create_listener(paths) {
        Ok(listener) => Ok(ListenerBindResult::Bound(listener)),
        Err(err) if is_address_in_use(&err) => handle_bind_conflict(paths, err),
        Err(err) => Err(err),
    }
}

pub fn send_request(paths: &VaultPaths, payload: &AgentRequestPayload) -> io::Result<AgentResponse> {
    log_debug!("Opening IPC request '{}' to password vault agent", payload.debug_name());
    let mut stream = connect(paths)?;
    let request = AgentRequestRef { payload };
    write_json_line(&mut stream, &request)?;
    read_json_line(&mut stream)
}

pub fn connect(paths: &VaultPaths) -> io::Result<LocalSocketStream> {
    let endpoint = agent_endpoint(paths);
    log_debug!("Connecting to password vault agent endpoint '{}'", endpoint.debug_label());
    let stream = connect_to_endpoint(&endpoint)?;
    remove_legacy_state_file(paths);
    Ok(stream)
}

pub fn cleanup_endpoint(paths: &VaultPaths) -> io::Result<()> {
    log_debug!("Cleaning password vault agent endpoint resources");
    remove_legacy_state_file(paths);
    cleanup_local_endpoint(paths)
}

pub fn read_request(stream: &mut LocalSocketStream) -> io::Result<AgentRequest> {
    read_json_line(stream)
}

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

    #[cfg(unix)]
    {
        if remove_stale_socket_file(paths)? {
            log_debug!("Removed stale password vault agent Unix socket; retrying bind");
            return match create_listener(paths) {
                Ok(listener) => Ok(ListenerBindResult::Bound(listener)),
                Err(err) if is_address_in_use(&err) && connect(paths).is_ok() => Ok(ListenerBindResult::AlreadyRunning),
                Err(err) => Err(err),
            };
        }
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
    let mut line = Zeroizing::new(String::new());
    reader.read_line(&mut line)?;
    serde_json::from_str(&line).map_err(|err| io::Error::other(format!("failed to parse IPC message: {err}")))
}

fn agent_endpoint(paths: &VaultPaths) -> AgentEndpoint {
    let identifier = format!("{AGENT_ENDPOINT_PREFIX}{:016x}", fnv1a_64(endpoint_seed(paths).as_bytes()));
    AgentEndpoint {
        #[cfg(unix)]
        socket_path: paths.run_dir().join(format!("{identifier}.sock")),
        identifier,
    }
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

#[cfg(unix)]
fn create_listener_for_endpoint(paths: &VaultPaths, endpoint: &AgentEndpoint) -> io::Result<LocalSocketListener> {
    fs::create_dir_all(paths.run_dir())?;
    set_restrictive_directory_permissions(&paths.run_dir())?;
    let name = endpoint.socket_path.as_os_str().to_fs_name::<GenericFilePath>()?;
    let listener = ListenerOptions::new().name(name).nonblocking(ListenerNonblockingMode::Accept).create_sync()?;
    set_restrictive_file_permissions(&endpoint.socket_path)?;
    Ok(listener)
}

#[cfg(unix)]
fn connect_to_endpoint(endpoint: &AgentEndpoint) -> io::Result<LocalSocketStream> {
    let name = endpoint.socket_path.as_os_str().to_fs_name::<GenericFilePath>()?;
    LocalSocketStream::connect(name)
}

#[cfg(unix)]
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

#[cfg(unix)]
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

#[cfg(unix)]
fn set_restrictive_directory_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(unix)]
fn set_restrictive_file_permissions(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(UNIX_SOCKET_MODE))?;
    Ok(())
}

#[cfg(not(unix))]
fn remove_stale_socket_file(_paths: &VaultPaths) -> io::Result<bool> {
    Ok(false)
}

#[cfg(not(unix))]
fn cleanup_local_endpoint(_paths: &VaultPaths) -> io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn set_restrictive_directory_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn set_restrictive_file_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn create_listener_for_endpoint(_paths: &VaultPaths, endpoint: &AgentEndpoint) -> io::Result<LocalSocketListener> {
    let name = endpoint.identifier.as_str().to_ns_name::<GenericNamespaced>()?;
    let security_descriptor = current_user_security_descriptor()?;
    ListenerOptions::new()
        .name(name)
        .nonblocking(ListenerNonblockingMode::Accept)
        .security_descriptor(security_descriptor)
        .create_sync()
}

#[cfg(windows)]
fn connect_to_endpoint(endpoint: &AgentEndpoint) -> io::Result<LocalSocketStream> {
    let name = endpoint.identifier.as_str().to_ns_name::<GenericNamespaced>()?;
    LocalSocketStream::connect(name)
}

#[cfg(windows)]
fn current_user_security_descriptor() -> io::Result<SecurityDescriptor> {
    let sid = current_user_sid_string()?;
    let descriptor = format!("D:P(A;;GA;;;{sid})");
    let descriptor = U16CString::from_str(&descriptor).map_err(|err| io::Error::other(format!("failed to encode security descriptor: {err}")))?;
    SecurityDescriptor::deserialize(descriptor.as_ucstr())
}

#[cfg(windows)]
fn current_user_sid_string() -> io::Result<String> {
    let mut token_handle = 0isize;
    let open_ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle) };
    if open_ok == 0 {
        return Err(io::Error::other(format!("failed to open process token: {}", unsafe { GetLastError() })));
    }

    struct HandleGuard(isize);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.0);
            }
        }
    }
    let _guard = HandleGuard(token_handle);

    let mut required_size = 0u32;
    unsafe {
        GetTokenInformation(token_handle, TokenUser, std::ptr::null_mut(), 0, &mut required_size);
    }
    if required_size == 0 {
        return Err(io::Error::other("failed to determine token information size"));
    }

    let mut buffer = vec![0u8; required_size as usize];
    let token_user_ptr = buffer.as_mut_ptr().cast();
    let token_ok = unsafe { GetTokenInformation(token_handle, TokenUser, token_user_ptr, required_size, &mut required_size) };
    if token_ok == 0 {
        return Err(io::Error::other(format!("failed to read token information: {}", unsafe { GetLastError() })));
    }

    let token_user = unsafe { &*(token_user_ptr as *const TOKEN_USER) };
    sid_to_string(token_user.User.Sid)
}

#[cfg(windows)]
fn sid_to_string(sid: PSID) -> io::Result<String> {
    let mut sid_ptr = std::ptr::null_mut();
    let convert_ok = unsafe { ConvertSidToStringSidW(sid, &mut sid_ptr) };
    if convert_ok == 0 {
        return Err(io::Error::other(format!("failed to convert SID to string: {}", unsafe { GetLastError() })));
    }

    struct LocalAllocGuard(*mut core::ffi::c_void);
    impl Drop for LocalAllocGuard {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0 as isize);
            }
        }
    }
    let _guard = LocalAllocGuard(sid_ptr.cast());

    let mut len = 0usize;
    unsafe {
        while *sid_ptr.add(len) != 0 {
            len += 1;
        }
        String::from_utf16(&std::slice::from_raw_parts(sid_ptr, len)).map_err(|err| io::Error::other(format!("failed to decode SID string: {err}")))
    }
}

#[cfg(test)]
#[path = "../test/auth/ipc.rs"]
mod tests;
