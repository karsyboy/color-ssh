use crate::auth::ipc::{self, AgentRequestPayload, AgentResponse, UnlockPolicy, VaultStatus};
use crate::auth::vault::{self, UnlockedVault, VaultError, VaultPaths};
use crate::command_path;
use std::fmt;
use std::io;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use zeroize::Zeroize;

const AGENT_STARTUP_TIMEOUT: Duration = Duration::from_secs(2);
const AGENT_STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub use crate::auth::ipc::{AgentConnectionInfo, AgentRequest, UnlockPolicy as AgentUnlockPolicy, VaultStatus as AgentVaultStatus};

#[derive(Debug)]
pub enum AgentError {
    Vault(VaultError),
    Io(io::Error),
    Locked,
    EntryNotFound,
    InvalidMasterPassword,
    VaultNotInitialized,
    Unauthorized,
    Protocol(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vault(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "{err}"),
            Self::Locked => write!(f, "password vault is locked"),
            Self::EntryNotFound => write!(f, "password vault entry was not found"),
            Self::InvalidMasterPassword => write!(f, "invalid master password"),
            Self::VaultNotInitialized => write!(f, "password vault is not initialized"),
            Self::Unauthorized => write!(f, "password vault agent rejected the request"),
            Self::Protocol(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<io::Error> for AgentError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<VaultError> for AgentError {
    fn from(value: VaultError) -> Self {
        Self::Vault(value)
    }
}

#[derive(Debug, Clone)]
pub struct AgentClient {
    paths: VaultPaths,
}

impl AgentClient {
    pub fn new() -> Result<Self, AgentError> {
        Ok(Self {
            paths: VaultPaths::resolve_default()?,
        })
    }
    pub fn status(&self) -> Result<VaultStatus, AgentError> {
        match self.request(AgentRequestPayload::Status, false) {
            Ok(AgentResponse::Status { status }) => Ok(status),
            Ok(response) => Err(AgentError::Protocol(format!("unexpected status response: {response:?}"))),
            Err(AgentError::Io(_)) => Ok(VaultStatus::locked(self.paths.metadata_path().is_file())),
            Err(err) => Err(err),
        }
    }

    pub fn unlock(&self, master_password: &str, policy: UnlockPolicy) -> Result<VaultStatus, AgentError> {
        match self.request(
            AgentRequestPayload::Unlock {
                master_password: master_password.to_string(),
                policy,
            },
            true,
        )? {
            AgentResponse::Success { status, .. } | AgentResponse::Status { status } => Ok(status),
            AgentResponse::Error { code, message, .. } => Err(map_remote_error(&code, message)),
            response => Err(AgentError::Protocol(format!("unexpected unlock response: {response:?}"))),
        }
    }

    pub fn get_secret(&self, name: &str) -> Result<String, AgentError> {
        match self.request(AgentRequestPayload::GetSecret { name: name.to_string() }, true)? {
            AgentResponse::Secret { secret, .. } => Ok(secret),
            AgentResponse::Error { code, message, .. } => Err(map_remote_error(&code, message)),
            response => Err(AgentError::Protocol(format!("unexpected get-secret response: {response:?}"))),
        }
    }

    pub fn lock(&self) -> Result<VaultStatus, AgentError> {
        match self.request(AgentRequestPayload::Lock, false)? {
            AgentResponse::Success { status, .. } | AgentResponse::Status { status } => Ok(status),
            AgentResponse::Error { code, message, .. } => Err(map_remote_error(&code, message)),
            response => Err(AgentError::Protocol(format!("unexpected lock response: {response:?}"))),
        }
    }

    fn request(&self, payload: AgentRequestPayload, auto_start: bool) -> Result<AgentResponse, AgentError> {
        match ipc::send_request(&self.paths, payload.clone()) {
            Ok(response) => Ok(response),
            Err(first_err) if auto_start => {
                let _ = ipc::remove_state_file(&self.paths);
                self.spawn_server()?;
                ipc::send_request(&self.paths, payload)
                    .map_err(|second_err| AgentError::Protocol(format!("failed to contact password vault agent after restart: {first_err}; {second_err}")))
            }
            Err(err) => Err(AgentError::Io(err)),
        }
    }

    fn spawn_server(&self) -> Result<(), AgentError> {
        let cossh_path = command_path::cossh_path()?;
        let mut command = Command::new(cossh_path);
        command
            .arg("agent")
            .arg("--serve")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env_remove(crate::auth::transport::INTERNAL_ASKPASS_MODE_ENV)
            .env_remove(crate::auth::transport::INTERNAL_ASKPASS_ENTRY_ENV)
            .env_remove("SSH_ASKPASS")
            .env_remove("SSH_ASKPASS_REQUIRE");
        command.spawn()?;

        let started_at = Instant::now();
        while started_at.elapsed() < AGENT_STARTUP_TIMEOUT {
            if ipc::send_request(&self.paths, AgentRequestPayload::Status).is_ok() {
                return Ok(());
            }
            thread::sleep(AGENT_STARTUP_POLL_INTERVAL);
        }

        Err(AgentError::Protocol("password vault agent did not become ready in time".to_string()))
    }
}

#[derive(Debug)]
struct AgentRuntime {
    data_key: Option<[u8; 32]>,
    unlocked_at: Option<Instant>,
    last_activity_at: Option<Instant>,
    policy: Option<UnlockPolicy>,
}

impl AgentRuntime {
    fn new() -> Self {
        Self {
            data_key: None,
            unlocked_at: None,
            last_activity_at: None,
            policy: None,
        }
    }

    fn expire_if_needed(&mut self) {
        let Some(policy) = &self.policy else {
            return;
        };
        let Some(unlocked_at) = self.unlocked_at else {
            self.lock();
            return;
        };
        let Some(last_activity_at) = self.last_activity_at else {
            self.lock();
            return;
        };

        let idle_expired = last_activity_at.elapsed() >= Duration::from_secs(policy.unlock_idle_timeout_seconds);
        let absolute_expired = unlocked_at.elapsed() >= Duration::from_secs(policy.unlock_absolute_timeout_seconds);
        if idle_expired || absolute_expired {
            self.lock();
        }
    }

    fn status(&self, paths: &VaultPaths) -> VaultStatus {
        let vault_exists = paths.metadata_path().is_file();
        let Some(policy) = &self.policy else {
            return VaultStatus::locked(vault_exists);
        };
        let Some(unlocked_at) = self.unlocked_at else {
            return VaultStatus::locked(vault_exists);
        };
        let Some(last_activity_at) = self.last_activity_at else {
            return VaultStatus::locked(vault_exists);
        };
        let idle_remaining = Duration::from_secs(policy.unlock_idle_timeout_seconds).saturating_sub(last_activity_at.elapsed());
        let absolute_remaining = Duration::from_secs(policy.unlock_absolute_timeout_seconds).saturating_sub(unlocked_at.elapsed());
        let expires_in_seconds = idle_remaining.min(absolute_remaining).as_secs();

        VaultStatus {
            vault_exists,
            unlocked: self.data_key.is_some(),
            unlock_expires_in_seconds: self.data_key.map(|_| expires_in_seconds),
            idle_timeout_seconds: Some(policy.unlock_idle_timeout_seconds),
            absolute_timeout_seconds: Some(policy.unlock_absolute_timeout_seconds),
        }
    }

    fn unlock(&mut self, data_key: [u8; 32], policy: UnlockPolicy) {
        self.lock();
        self.data_key = Some(data_key);
        self.unlocked_at = Some(Instant::now());
        self.last_activity_at = self.unlocked_at;
        self.policy = Some(policy);
    }

    fn touch(&mut self) {
        self.last_activity_at = Some(Instant::now());
    }

    fn lock(&mut self) {
        if let Some(mut data_key) = self.data_key.take() {
            data_key.zeroize();
        }
        self.unlocked_at = None;
        self.last_activity_at = None;
        self.policy = None;
    }

    fn unlocked_vault(&self, paths: &VaultPaths) -> Option<UnlockedVault> {
        self.data_key.map(|data_key| UnlockedVault::from_data_key(paths.clone(), data_key))
    }
}

struct StateFileGuard {
    paths: VaultPaths,
}

impl Drop for StateFileGuard {
    fn drop(&mut self) {
        let _ = ipc::remove_state_file(&self.paths);
    }
}

pub fn run_server() -> Result<(), AgentError> {
    let paths = VaultPaths::resolve_default()?;
    let (listener, state) = ipc::bind_loopback_listener()?;
    ipc::write_state(&paths, &state)?;
    let _state_guard = StateFileGuard { paths: paths.clone() };
    let mut runtime = AgentRuntime::new();

    loop {
        let (mut stream, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(AgentError::Io(err)),
        };

        let request = match ipc::read_request(&mut stream) {
            Ok(request) => request,
            Err(err) => {
                let response = AgentResponse::Error {
                    status: runtime.status(&paths),
                    code: "invalid_request".to_string(),
                    message: format!("failed to read request: {err}"),
                };
                let _ = ipc::write_response(&mut stream, &response);
                continue;
            }
        };

        runtime.expire_if_needed();
        let response = handle_request(&paths, &state.token, &mut runtime, request);
        let _ = ipc::write_response(&mut stream, &response);
    }
}

fn handle_request(paths: &VaultPaths, expected_token: &str, runtime: &mut AgentRuntime, request: ipc::AgentRequest) -> AgentResponse {
    if request.token != expected_token {
        return AgentResponse::Error {
            status: runtime.status(paths),
            code: "unauthorized".to_string(),
            message: "password vault agent rejected the request".to_string(),
        };
    }

    match request.payload {
        AgentRequestPayload::Status => AgentResponse::Status { status: runtime.status(paths) },
        AgentRequestPayload::Lock => {
            runtime.lock();
            AgentResponse::Success {
                status: runtime.status(paths),
                message: "password vault locked".to_string(),
            }
        }
        AgentRequestPayload::Unlock { master_password, policy } => match vault::unlock_with_password_and_paths(paths, &master_password) {
            Ok(unlocked) => {
                runtime.unlock(unlocked.data_key_copy(), policy);
                AgentResponse::Success {
                    status: runtime.status(paths),
                    message: "password vault unlocked".to_string(),
                }
            }
            Err(err) => agent_error_response(runtime, paths, err),
        },
        AgentRequestPayload::GetSecret { name } => {
            let Some(unlocked) = runtime.unlocked_vault(paths) else {
                return AgentResponse::Error {
                    status: runtime.status(paths),
                    code: "locked".to_string(),
                    message: "password vault is locked".to_string(),
                };
            };
            match unlocked.get_secret(&name) {
                Ok(secret) => {
                    runtime.touch();
                    AgentResponse::Secret {
                        status: runtime.status(paths),
                        name,
                        secret,
                    }
                }
                Err(err) => agent_error_response(runtime, paths, err),
            }
        }
    }
}

fn agent_error_response(runtime: &mut AgentRuntime, paths: &VaultPaths, err: VaultError) -> AgentResponse {
    let (code, message) = match err {
        VaultError::EntryNotFound => ("entry_not_found", err.to_string()),
        VaultError::VaultNotInitialized => ("vault_not_initialized", err.to_string()),
        VaultError::InvalidMasterPassword => ("invalid_master_password", err.to_string()),
        VaultError::InvalidEntryName => ("invalid_entry_name", err.to_string()),
        other => ("vault_error", other.to_string()),
    };
    AgentResponse::Error {
        status: runtime.status(paths),
        code: code.to_string(),
        message,
    }
}

fn map_remote_error(code: &str, message: String) -> AgentError {
    match code {
        "locked" => AgentError::Locked,
        "entry_not_found" => AgentError::EntryNotFound,
        "invalid_master_password" => AgentError::InvalidMasterPassword,
        "vault_not_initialized" => AgentError::VaultNotInitialized,
        "unauthorized" => AgentError::Unauthorized,
        "invalid_entry_name" | "vault_error" => AgentError::Protocol(message),
        _ => AgentError::Protocol(message),
    }
}

#[cfg(test)]
#[path = "../test/auth/agent.rs"]
mod tests;
