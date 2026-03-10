use super::error::{AgentError, map_remote_error};
use crate::auth::ipc::{self, AgentRequestPayload, AgentResponse, UnlockPolicy, VaultStatus};
use crate::auth::secret::{SensitiveString, sensitive_string};
use crate::auth::vault::VaultPaths;
use crate::log_debug;
use crate::platform;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const AGENT_STARTUP_TIMEOUT: Duration = Duration::from_secs(2);
const AGENT_STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Result of checking whether a vault entry is available for use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntryStatus {
    /// Current vault lock/unlock status.
    pub status: VaultStatus,
    /// Whether the queried entry name exists in the vault.
    pub exists: bool,
}

/// Client used by runtime command paths to communicate with the unlock agent.
#[derive(Debug, Clone)]
pub struct AgentClient {
    paths: VaultPaths,
}

impl AgentClient {
    /// Create a client bound to the default `~/.color-ssh` runtime paths.
    pub fn new() -> Result<Self, AgentError> {
        let paths = VaultPaths::resolve_default()?;
        log_debug!("Initialized password vault client for '{}'", paths.base_dir().display());
        Ok(Self { paths })
    }

    /// Query current vault status.
    pub fn status(&self) -> Result<VaultStatus, AgentError> {
        log_debug!("Requesting password vault status");
        match self.request(AgentRequestPayload::Status, false) {
            Ok(AgentResponse::Status { status }) => Ok(status),
            Ok(response) => Err(AgentError::Protocol(format!("unexpected status response: {response:?}"))),
            Err(AgentError::Io(_)) => Ok(VaultStatus::locked(self.paths.metadata_path().is_file())),
            Err(err) => Err(err),
        }
    }

    /// Unlock the vault using a master password and timeout policy.
    pub fn unlock(&self, master_password: &str, policy: UnlockPolicy) -> Result<VaultStatus, AgentError> {
        log_debug!(
            "Requesting password vault unlock with idle={}s absolute={}s",
            policy.idle_timeout_seconds,
            policy.session_timeout_seconds
        );
        match self.request(
            AgentRequestPayload::Unlock {
                master_password: sensitive_string(master_password),
                policy,
            },
            true,
        )? {
            AgentResponse::Success { status, .. } | AgentResponse::Status { status } => Ok(status),
            AgentResponse::Error { code, message, .. } => Err(map_remote_error(&code, message)),
            response => Err(AgentError::Protocol(format!("unexpected unlock response: {response:?}"))),
        }
    }

    /// Query whether a vault entry exists and whether the vault is unlocked.
    pub fn entry_status(&self, name: &str) -> Result<AgentEntryStatus, AgentError> {
        log_debug!("Requesting password vault entry status '{}'", name);
        match self.request(AgentRequestPayload::EntryStatus { name: name.to_string() }, true)? {
            AgentResponse::EntryStatus { status, exists, .. } => Ok(AgentEntryStatus { status, exists }),
            AgentResponse::Error { code, message, .. } => Err(map_remote_error(&code, message)),
            response => Err(AgentError::Protocol(format!("unexpected entry-status response: {response:?}"))),
        }
    }

    /// Authorize one short-lived askpass token for the named vault entry.
    pub fn authorize_askpass(&self, name: &str) -> Result<SensitiveString, AgentError> {
        log_debug!("Requesting internal askpass authorization for '{}'", name);
        match self.request(AgentRequestPayload::AuthorizeAskpass { name: name.to_string() }, true)? {
            AgentResponse::AskpassAuthorized { token, .. } => Ok(token),
            AgentResponse::Error { code, message, .. } => Err(map_remote_error(&code, message)),
            response => Err(AgentError::Protocol(format!("unexpected askpass authorization response: {response:?}"))),
        }
    }

    /// Resolve a secret by askpass token.
    pub fn get_secret(&self, token: &str) -> Result<SensitiveString, AgentError> {
        log_debug!("Requesting password vault secret using askpass token");
        match self.request(
            AgentRequestPayload::GetSecret {
                token: sensitive_string(token),
            },
            true,
        )? {
            AgentResponse::Secret { secret, .. } => Ok(secret),
            AgentResponse::Error { code, message, .. } => Err(map_remote_error(&code, message)),
            response => Err(AgentError::Protocol(format!("unexpected get-secret response: {response:?}"))),
        }
    }

    /// Request an explicit vault lock and agent shutdown.
    pub fn lock(&self) -> Result<VaultStatus, AgentError> {
        log_debug!("Requesting password vault lock");
        match self.request(AgentRequestPayload::Lock, false)? {
            AgentResponse::Success { status, .. } | AgentResponse::Status { status } => Ok(status),
            AgentResponse::Error { code, message, .. } => Err(map_remote_error(&code, message)),
            response => Err(AgentError::Protocol(format!("unexpected lock response: {response:?}"))),
        }
    }

    fn request(&self, payload: AgentRequestPayload, auto_start: bool) -> Result<AgentResponse, AgentError> {
        let payload_name = payload.debug_name();
        log_debug!("Sending password vault agent request '{}' (auto_start={})", payload_name, auto_start);
        match ipc::send_request(&self.paths, &payload) {
            Ok(response) => Ok(response),
            Err(first_err) if auto_start => {
                log_debug!(
                    "Password vault agent request '{}' failed initially ({}); attempting auto-start",
                    payload_name,
                    first_err
                );
                self.spawn_server()?;
                ipc::send_request(&self.paths, &payload)
                    .map_err(|second_err| AgentError::Protocol(format!("failed to contact password vault agent after restart: {first_err}; {second_err}")))
            }
            Err(err) => Err(AgentError::Io(err)),
        }
    }

    fn spawn_server(&self) -> Result<(), AgentError> {
        log_debug!("Starting password vault agent process");
        let cossh_path = platform::cossh_path()?;
        let mut command = Command::new(cossh_path);
        command
            .arg("agent")
            .arg("--serve")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env_remove(crate::auth::transport::INTERNAL_ASKPASS_MODE_ENV)
            .env_remove(crate::auth::transport::INTERNAL_ASKPASS_TOKEN_ENV)
            .env_remove("SSH_ASKPASS")
            .env_remove("SSH_ASKPASS_REQUIRE");
        command.spawn()?;

        // Poll readiness briefly so first caller does not race the agent boot.
        let started_at = Instant::now();
        while started_at.elapsed() < AGENT_STARTUP_TIMEOUT {
            if ipc::send_request(&self.paths, &AgentRequestPayload::Status).is_ok() {
                log_debug!("Password vault agent became ready in {:?}", started_at.elapsed());
                return Ok(());
            }
            thread::sleep(AGENT_STARTUP_POLL_INTERVAL);
        }

        Err(AgentError::Protocol("password vault agent did not become ready in time".to_string()))
    }
}
