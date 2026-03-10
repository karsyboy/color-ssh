//! Interactive direct-session runtime entry points.

use super::command_spec::PreparedCommand;
use crate::{Result, log_info};
use std::process::ExitCode;

pub(super) fn run_interactive_ssh_session(command_spec: PreparedCommand) -> Result<ExitCode> {
    log_info!("Using PTY-centered interactive SSH runtime");
    super::pty_runtime::run_interactive_ssh(command_spec)
}

pub(super) fn run_interactive_rdp_session(command_spec: PreparedCommand) -> Result<ExitCode> {
    log_info!("Using PTY-centered interactive RDP runtime");
    super::pty_runtime::run_interactive_rdp(command_spec)
}
