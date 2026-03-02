use crate::auth::vault::VaultPaths;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use getrandom::fill as random_fill;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::Path;

const AGENT_STATE_VERSION: u8 = 1;
const AGENT_TOKEN_BYTES: usize = 32;

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
    Unlock { master_password: String, policy: UnlockPolicy },
    GetSecret { name: String },
    Lock,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRequest {
    pub token: String,
    pub payload: AgentRequestPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentResponse {
    Status { status: VaultStatus },
    Secret { status: VaultStatus, name: String, secret: String },
    Success { status: VaultStatus, message: String },
    Error { status: VaultStatus, code: String, message: String },
}

impl AgentResponse {
    pub fn status(&self) -> &VaultStatus {
        match self {
            Self::Status { status } | Self::Secret { status, .. } | Self::Success { status, .. } | Self::Error { status, .. } => status,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConnectionInfo {
    pub version: u8,
    pub host: String,
    pub port: u16,
    pub token: String,
}

pub fn bind_loopback_listener() -> io::Result<(TcpListener, AgentConnectionInfo)> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();

    let mut token_bytes = [0u8; AGENT_TOKEN_BYTES];
    random_fill(&mut token_bytes).map_err(|err| io::Error::other(format!("secure random generation failed: {err}")))?;

    let info = AgentConnectionInfo {
        version: AGENT_STATE_VERSION,
        host: Ipv4Addr::LOCALHOST.to_string(),
        port,
        token: BASE64.encode(token_bytes),
    };

    Ok((listener, info))
}

pub fn send_request(paths: &VaultPaths, payload: AgentRequestPayload) -> io::Result<AgentResponse> {
    let state = read_state(paths)?;
    let mut stream = TcpStream::connect((state.host.as_str(), state.port))?;
    let request = AgentRequest { token: state.token, payload };
    write_json_line(&mut stream, &request)?;
    read_json_line(&mut stream)
}

pub fn read_request(stream: &mut TcpStream) -> io::Result<AgentRequest> {
    read_json_line(stream)
}

pub fn write_response(stream: &mut TcpStream, response: &AgentResponse) -> io::Result<()> {
    write_json_line(stream, response)
}

pub fn write_state(paths: &VaultPaths, state: &AgentConnectionInfo) -> io::Result<()> {
    fs::create_dir_all(paths.run_dir())?;
    set_restrictive_directory_permissions(&paths.run_dir())?;
    let serialized = serde_json::to_vec_pretty(state).map_err(|err| io::Error::other(format!("failed to serialize agent state: {err}")))?;
    let tmp_path = paths.run_dir().join(format!(".agent-state.tmp-{}", std::process::id()));
    fs::write(&tmp_path, serialized)?;
    set_restrictive_file_permissions(&tmp_path)?;
    fs::rename(&tmp_path, paths.agent_state_path())?;
    set_restrictive_file_permissions(&paths.agent_state_path())?;
    Ok(())
}

pub fn read_state(paths: &VaultPaths) -> io::Result<AgentConnectionInfo> {
    let bytes = fs::read(paths.agent_state_path())?;
    let state: AgentConnectionInfo = serde_json::from_slice(&bytes).map_err(|err| io::Error::other(format!("failed to parse agent state: {err}")))?;
    if state.version != AGENT_STATE_VERSION {
        return Err(io::Error::other("unsupported agent state version"));
    }
    Ok(state)
}

pub fn remove_state_file(paths: &VaultPaths) -> io::Result<()> {
    let path = paths.agent_state_path();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn write_json_line<T: Serialize>(stream: &mut TcpStream, value: &T) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(|err| io::Error::other(format!("failed to serialize IPC message: {err}")))?;
    bytes.push(b'\n');
    stream.write_all(&bytes)?;
    stream.flush()
}

fn read_json_line<T: for<'de> Deserialize<'de>>(stream: &mut TcpStream) -> io::Result<T> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    serde_json::from_str(&line).map_err(|err| io::Error::other(format!("failed to parse IPC message: {err}")))
}

#[cfg(unix)]
fn set_restrictive_directory_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_restrictive_directory_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_restrictive_file_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_restrictive_file_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}
