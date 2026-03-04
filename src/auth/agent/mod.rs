//! Password-vault unlock agent interfaces.
//!
//! `AgentClient` is used by command execution paths to query and unlock the
//! vault through a shared local IPC endpoint.

mod client;
mod error;
mod runtime;
mod server;

/// Shared request/response payload types and status metadata.
pub use crate::auth::ipc::{AgentRequest, UnlockPolicy as AgentUnlockPolicy, VaultStatus as AgentVaultStatus};
/// Client for querying and controlling the local unlock agent.
pub use client::{AgentClient, AgentEntryStatus};
/// Error type returned by agent client/server operations.
pub use error::AgentError;
/// Starts the unlock-agent server process loop.
pub use server::run_server;

#[cfg(test)]
pub(super) use crate::auth::ipc;
#[cfg(test)]
pub(super) use crate::auth::ipc::AgentResponse;
#[cfg(test)]
pub(super) use error::map_remote_error;
#[cfg(test)]
pub(super) use runtime::{AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MIN, AgentRuntime, next_idle_shutdown_poll_interval};
#[cfg(test)]
pub(super) use server::handle_request;

#[cfg(test)]
#[path = "../../test/auth/agent.rs"]
mod tests;
