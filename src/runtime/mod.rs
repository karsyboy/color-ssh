//! Top-level application startup and command dispatch.

mod dispatch;
mod logging;
mod startup;

use std::process::ExitCode;

/// Execute the `cossh` runtime and return process exit code.
pub fn run() -> crate::Result<ExitCode> {
    dispatch::run()
}

#[cfg(test)]
pub(crate) use logging::{DebugModeSource, debug_mode_source, resolve_logging_settings, resolve_session_name_for_logging};

#[cfg(test)]
#[path = "../test/runtime.rs"]
mod tests;
