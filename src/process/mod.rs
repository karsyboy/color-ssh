//! Direct subprocess orchestration for SSH and RDP launches.

mod exit;
mod interactive;
mod launch;
mod stream;

use crate::args::RdpCommandArgs;
use crate::{Result, log_debug, log_debug_raw, log_error, log_info, log_warn, ssh_args};
use std::process::ExitCode;

pub(crate) use launch::{build_rdp_command_for_host, spawn_command};

pub(crate) fn run_ssh_process(process_args: Vec<String>, is_non_interactive: bool, explicit_pass_entry: Option<String>) -> Result<ExitCode> {
    log_info!(
        "Starting SSH process: interactive={} ssh_arg_count={} explicit_pass_entry={} destination_resolved={}",
        !is_non_interactive,
        process_args.len(),
        explicit_pass_entry.is_some(),
        ssh_args::extract_destination_host(&process_args).is_some()
    );
    log_debug_raw!("Starting SSH process with args: {:?}", process_args);
    log_debug!("Non-interactive mode: {}", is_non_interactive);

    let command_spec = if is_non_interactive {
        launch::build_plain_ssh_command(&process_args)
    } else {
        launch::build_ssh_command(&process_args, explicit_pass_entry.as_deref())?
    };

    if let Some(notice) = &command_spec.fallback_notice {
        log_warn!("{}", notice);
        eprintln!("[color-ssh] {}", notice);
    }

    if is_non_interactive {
        log_info!("Using passthrough mode for non-interactive command");
        return launch::spawn_passthrough(command_spec);
    }

    let child = launch::spawn_command(command_spec, std::process::Stdio::piped(), std::process::Stdio::inherit()).map_err(|err| {
        log_error!("Failed to spawn SSH process: {}", err);
        err
    })?;
    log_debug!("SSH process spawned successfully (PID: {:?})", child.id());

    interactive::run_interactive_ssh_session(child)
}

pub(crate) fn run_rdp_process(rdp_args: RdpCommandArgs, explicit_pass_entry: Option<String>) -> Result<ExitCode> {
    log_info!(
        "Starting RDP process: target={} explicit_pass_entry={} extra_arg_count={}",
        rdp_args.target,
        explicit_pass_entry.is_some(),
        rdp_args.extra_args.len()
    );
    log_debug_raw!("Starting RDP process with args: {:?}", rdp_args);

    let command_spec = launch::build_rdp_command(&rdp_args, explicit_pass_entry.as_deref())?;
    if let Some(notice) = &command_spec.fallback_notice {
        log_warn!("{}", notice);
        eprintln!("[color-ssh] {}", notice);
    }

    let child = launch::spawn_command(command_spec, std::process::Stdio::piped(), std::process::Stdio::piped()).map_err(|err| {
        log_error!("Failed to spawn RDP process: {}", err);
        err
    })?;
    log_debug!("RDP process spawned successfully (PID: {:?})", child.id());

    interactive::run_interactive_rdp_session(child)
}

#[cfg(test)]
#[path = "../test/process.rs"]
mod tests;
