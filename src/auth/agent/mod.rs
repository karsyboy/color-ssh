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
