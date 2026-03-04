//! Authentication and vault integration.
//!
//! This module provides:
//! - encrypted local password vault primitives
//! - unlock agent IPC/client/server plumbing
//! - CLI flows for vault lifecycle commands
//! - internal askpass transport for password auto-login

pub mod agent;
mod cli;
pub mod ipc;
mod prompt;
pub mod secret;
pub mod transport;
pub mod vault;

pub(crate) use cli::{run_internal_askpass, run_vault_command};
pub(crate) use prompt::prompt_hidden_secret;
