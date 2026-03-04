use super::error::AgentError;
use super::runtime::{AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MIN, AgentRuntime, next_idle_shutdown_poll_interval};
use crate::auth::ipc::{self, AgentRequestPayload, AgentResponse, VaultStatus, VaultStatusEventKind};
use crate::auth::secret::ExposeSecret;
use crate::auth::vault::{self, VaultError, VaultPaths};
use crate::log_debug;
use interprocess::local_socket::traits::Listener as _;
use std::io;
use std::thread;

struct EndpointGuard {
    paths: VaultPaths,
}

impl Drop for EndpointGuard {
    fn drop(&mut self) {
        log_debug!("Cleaning up password vault agent endpoint");
        let _ = ipc::cleanup_endpoint(&self.paths);
    }
}

pub fn run_server() -> Result<(), AgentError> {
    let paths = VaultPaths::resolve_default()?;
    let listener = match ipc::bind_listener(&paths)? {
        ipc::ListenerBindResult::Bound(listener) => listener,
        ipc::ListenerBindResult::AlreadyRunning => {
            log_debug!("Password vault agent already running; new server instance will exit");
            return Ok(());
        }
    };
    log_debug!("Password vault agent server started");
    let _endpoint_guard = EndpointGuard { paths: paths.clone() };
    let mut runtime = AgentRuntime::new();
    let mut idle_poll_interval = AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MIN;

    loop {
        if runtime.expire_if_needed() {
            broadcast_vault_status_event(&paths, VaultStatusEventKind::Locked, runtime.status(&paths));
            log_debug!("Password vault agent exiting after session expiry");
            return Ok(());
        }

        let mut stream = match listener.accept() {
            Ok(connection) => {
                idle_poll_interval = AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MIN;
                connection
            }
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(idle_poll_interval);
                idle_poll_interval = next_idle_shutdown_poll_interval(idle_poll_interval);
                continue;
            }
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

        if runtime.expire_if_needed() {
            broadcast_vault_status_event(&paths, VaultStatusEventKind::Locked, runtime.status(&paths));
            let response = AgentResponse::Error {
                status: runtime.status(&paths),
                code: "locked".to_string(),
                message: "password vault session expired".to_string(),
            };
            let _ = ipc::write_response(&mut stream, &response);
            log_debug!("Password vault agent exiting after session expiry during request handling");
            return Ok(());
        }

        let should_shutdown = matches!(&request.payload, AgentRequestPayload::Lock);
        log_debug!("Handling password vault agent request '{}'", request.payload.debug_name());
        let response = handle_request(&paths, &mut runtime, request);
        let _ = ipc::write_response(&mut stream, &response);
        if should_shutdown {
            log_debug!("Password vault agent exiting after explicit lock request");
            return Ok(());
        }
    }
}

pub(crate) fn handle_request(paths: &VaultPaths, runtime: &mut AgentRuntime, request: ipc::AgentRequest) -> AgentResponse {
    match request.payload {
        AgentRequestPayload::Status => AgentResponse::Status { status: runtime.status(paths) },
        AgentRequestPayload::Lock => {
            if runtime.lock() {
                broadcast_vault_status_event(paths, VaultStatusEventKind::Locked, runtime.status(paths));
            }
            AgentResponse::Success {
                status: runtime.status(paths),
                message: "password vault locked".to_string(),
            }
        }
        AgentRequestPayload::Unlock { master_password, policy } => match vault::unlock_with_password_and_paths(paths, master_password.expose_secret()) {
            Ok(unlocked) => {
                runtime.unlock(unlocked.data_key_copy(), policy);
                log_debug!("Password vault agent accepted unlock request");
                let status = runtime.status(paths);
                broadcast_vault_status_event(paths, VaultStatusEventKind::Unlocked, status.clone());
                AgentResponse::Success {
                    status,
                    message: "password vault unlocked".to_string(),
                }
            }
            Err(err) => agent_error_response(runtime, paths, err),
        },
        AgentRequestPayload::AuthorizeAskpass { name } => {
            let Some(_unlocked) = runtime.unlocked_vault(paths) else {
                return AgentResponse::Error {
                    status: runtime.status(paths),
                    code: "locked".to_string(),
                    message: "password vault is locked".to_string(),
                };
            };
            match vault::entry_exists_with_paths(paths, &name) {
                Ok(true) => match runtime.issue_askpass_token(&name) {
                    Ok(token) => {
                        runtime.touch();
                        AgentResponse::AskpassAuthorized {
                            status: runtime.status(paths),
                            token,
                        }
                    }
                    Err(err) => AgentResponse::Error {
                        status: runtime.status(paths),
                        code: "askpass_token_error".to_string(),
                        message: err.to_string(),
                    },
                },
                Ok(false) => AgentResponse::Error {
                    status: runtime.status(paths),
                    code: "entry_not_found".to_string(),
                    message: "password vault entry was not found".to_string(),
                },
                Err(err) => agent_error_response(runtime, paths, err),
            }
        }
        AgentRequestPayload::EntryStatus { name } => match vault::entry_exists_with_paths(paths, &name) {
            Ok(exists) => AgentResponse::EntryStatus {
                status: runtime.status(paths),
                name,
                exists,
            },
            Err(err) => agent_error_response(runtime, paths, err),
        },
        AgentRequestPayload::GetSecret { token } => {
            let Some(unlocked) = runtime.unlocked_vault(paths) else {
                return AgentResponse::Error {
                    status: runtime.status(paths),
                    code: "locked".to_string(),
                    message: "password vault is locked".to_string(),
                };
            };
            let Some(name) = runtime.take_askpass_entry(token.expose_secret()) else {
                return AgentResponse::Error {
                    status: runtime.status(paths),
                    code: "invalid_or_expired_askpass_token".to_string(),
                    message: "invalid or expired askpass token".to_string(),
                };
            };
            match unlocked.get_secret(&name) {
                Ok(secret) => {
                    runtime.touch();
                    log_debug!("Password vault agent served secret for entry '{}'", name);
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

fn broadcast_vault_status_event(paths: &VaultPaths, kind: VaultStatusEventKind, status: VaultStatus) {
    if let Err(err) = ipc::broadcast_vault_status_event(paths, kind, status) {
        log_debug!("Failed to broadcast password vault status event: {}", err);
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
