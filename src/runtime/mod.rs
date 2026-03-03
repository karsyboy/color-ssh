//! Top-level application startup and command dispatch.

mod dispatch;
mod logging;
mod startup;

use std::process::ExitCode;

pub fn run() -> crate::Result<ExitCode> {
    dispatch::run()
}

#[cfg(test)]
pub(crate) use logging::{DebugModeSource, debug_mode_source, resolve_logging_settings};

#[cfg(test)]
#[path = "../test/runtime.rs"]
mod tests;
