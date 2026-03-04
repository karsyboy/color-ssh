mod client;
mod error;
mod runtime;
mod server;

pub use crate::auth::ipc::{AgentRequest, UnlockPolicy as AgentUnlockPolicy, VaultStatus as AgentVaultStatus};
pub use client::{AgentClient, AgentEntryStatus};
pub use error::AgentError;
pub use server::run_server;

#[cfg(test)]
pub(super) use crate::auth::ipc::AgentResponse;
#[cfg(test)]
pub(super) use crate::auth::{ipc, vault};
#[cfg(test)]
pub(super) use error::map_remote_error;
#[cfg(test)]
pub(super) use runtime::{AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MAX, AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MIN, AgentRuntime, next_idle_shutdown_poll_interval};
#[cfg(test)]
pub(super) use server::handle_request;

#[cfg(test)]
#[path = "../../test/auth/agent.rs"]
mod tests;
