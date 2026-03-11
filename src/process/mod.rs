//! Direct subprocess orchestration for SSH and RDP launches.

mod command_spec;
mod pty_output;
mod pty_runtime;
mod rdp_builder;
mod spawn;
mod spawn_common;
mod ssh_builder;
mod vault;

use crate::args::RdpCommandArgs;
use crate::{Result, args, log_debug, log_debug_raw, log_info, log_warn};
use std::process::ExitCode;

pub(crate) use pty_output::{PtyLogTarget, spawn_pty_output_reader};
pub(crate) use rdp_builder::build_rdp_command_for_host_with_auth_settings;
pub(crate) use spawn_common::{io_other_error, spawn_captured_command, spawn_pty_command};
pub(crate) use ssh_builder::{build_ssh_command_for_host, resolve_host_by_destination};
pub(crate) const DISABLE_VAULT_AUTOLOGIN_ENV: &str = "COSSH_DISABLE_VAULT_AUTOLOGIN";

pub(crate) fn prefer_pty_centered_interactive_ssh_runtime() -> bool {
    pty_runtime::prefer_pty_centered_ssh_runtime()
}

pub(crate) fn run_ssh_process(process_args: Vec<String>, is_non_interactive: bool, explicit_pass_entry: Option<String>) -> Result<ExitCode> {
    log_info!(
        "Starting SSH process: interactive={} ssh_arg_count={} explicit_pass_entry={} destination_resolved={}",
        !is_non_interactive,
        process_args.len(),
        explicit_pass_entry.is_some(),
        args::extract_destination_host(&process_args).is_some()
    );
    log_debug_raw!("Starting SSH process with args: {:?}", process_args);
    log_debug!("Non-interactive mode: {}", is_non_interactive);

    let command_spec = if is_non_interactive {
        command_spec::build_plain_ssh_command(&process_args)
    } else {
        ssh_builder::build_ssh_command(&process_args, explicit_pass_entry.as_deref())?
    };

    if is_non_interactive {
        if let Some(notice) = &command_spec.fallback_notice {
            log_warn!("{}", notice);
            eprintln!("[color-ssh] {}", notice);
        }
        log_info!("Using passthrough mode for non-interactive command");
        return spawn::spawn_passthrough(command_spec);
    }

    if let Some(notice) = &command_spec.fallback_notice {
        log_warn!("{}", notice);
    }

    if !pty_runtime::prefer_direct_pty_runtime() {
        if let Some(notice) = &command_spec.fallback_notice {
            eprintln!("[color-ssh] {}", notice);
        }
        log_info!("Using passthrough compatibility mode for SSH command without an interactive controlling terminal");
        return spawn::spawn_passthrough(command_spec);
    }

    log_info!("Using PTY-centered interactive SSH runtime");
    pty_runtime::run_interactive_ssh(command_spec)
}

pub(crate) fn run_rdp_process(rdp_args: RdpCommandArgs, explicit_pass_entry: Option<String>) -> Result<ExitCode> {
    log_info!(
        "Starting RDP process: target={} explicit_pass_entry={} extra_arg_count={}",
        rdp_args.target,
        explicit_pass_entry.is_some(),
        rdp_args.extra_args.len()
    );
    log_debug_raw!("Starting RDP process with args: {:?}", rdp_args);

    let command_spec = rdp_builder::build_rdp_command(&rdp_args, explicit_pass_entry.as_deref())?;
    if let Some(notice) = &command_spec.fallback_notice {
        log_warn!("{}", notice);
        eprintln!("[color-ssh] {}", notice);
    }

    if command_spec.stdin_payload.is_some() {
        log_info!("Using passthrough mode for vault-backed RDP command so FreeRDP receives startup arguments over a closed pipe");
    } else {
        log_info!("Using passthrough mode for RDP command so FreeRDP can prompt on the controlling terminal");
    }

    spawn::spawn_passthrough(command_spec)
}

pub(super) fn map_exit_code(success: bool, code: Option<i32>) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        let clamped_code = code.map_or(1, |status_code| u8::try_from(status_code).unwrap_or(255));
        ExitCode::from(clamped_code)
    }
}

#[cfg(test)]
#[path = "../test/process/mod.rs"]
mod tests;
