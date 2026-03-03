pub mod agent;
mod cli;
pub mod ipc;
mod prompt;
pub mod secret;
pub mod transport;
pub mod vault;

pub(crate) use cli::{run_internal_askpass, run_vault_command};
pub(crate) use prompt::prompt_hidden_secret;
