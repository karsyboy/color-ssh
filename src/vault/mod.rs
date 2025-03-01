mod commands;
pub mod core;
mod errors;

pub use core::vault_handler;
pub use core::VaultManager;
pub use errors::VaultError;
