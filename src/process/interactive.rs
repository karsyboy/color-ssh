use super::command_spec::PreparedCommand;
use super::spawn::spawn_command;
use crate::{Result, log_error, log_info};
use std::process::{Child, ExitCode, Stdio};

pub(super) fn run_interactive_ssh_session(command_spec: PreparedCommand) -> Result<ExitCode> {
    if super::pty_runtime::prefer_pty_centered_ssh_runtime() {
        log_info!("Using PTY-centered interactive SSH runtime");
        return super::pty_runtime::run_interactive_ssh(command_spec);
    }

    log_info!("Using legacy stream interactive SSH runtime");
    let child = spawn_command(command_spec, Stdio::piped(), Stdio::inherit()).map_err(|err| {
        log_error!("Failed to spawn SSH process: {}", err);
        err
    })?;

    super::stream::run_interactive_ssh(child)
}

pub(super) fn run_interactive_rdp_session(child: Child) -> Result<ExitCode> {
    super::stream::run_interactive_rdp(child)
}
