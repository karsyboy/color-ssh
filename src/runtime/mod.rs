//! Top-level application startup and command dispatch.

mod dispatch;
mod logging;
mod reload_notice;
mod startup;

use std::process::ExitCode;

/// Execute the `cossh` runtime and return process exit code.
pub fn run() -> crate::Result<ExitCode> {
    dispatch::run()
}

pub(crate) use reload_notice::{ReloadNoticeToast, format_reload_notice};
