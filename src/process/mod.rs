//! Direct subprocess orchestration for SSH and RDP launches.

mod command_spec;
mod exit;
mod interactive;
mod launch;
mod pty_output;
mod pty_runtime;
mod rdp_builder;
mod spawn;
mod ssh_builder;
mod vault;

use crate::args::RdpCommandArgs;
use crate::{Result, log_debug, log_debug_raw, log_info, log_warn, ssh_args};
use std::process::ExitCode;

pub(crate) use launch::{build_rdp_command_for_host_with_auth_settings, build_ssh_command_for_host, resolve_host_by_destination};
pub(crate) use pty_output::{PtyLogTarget, spawn_pty_output_reader};
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
        ssh_args::extract_destination_host(&process_args).is_some()
    );
    log_debug_raw!("Starting SSH process with args: {:?}", process_args);
    log_debug!("Non-interactive mode: {}", is_non_interactive);

    let command_spec = if is_non_interactive {
        launch::build_plain_ssh_command(&process_args)
    } else {
        launch::build_ssh_command(&process_args, explicit_pass_entry.as_deref())?
    };

    if is_non_interactive {
        if let Some(notice) = &command_spec.fallback_notice {
            log_warn!("{}", notice);
            eprintln!("[color-ssh] {}", notice);
        }
        log_info!("Using passthrough mode for non-interactive command");
        return launch::spawn_passthrough(command_spec);
    }

    if let Some(notice) = &command_spec.fallback_notice {
        log_warn!("{}", notice);
    }

    if !pty_runtime::prefer_direct_pty_runtime() {
        if let Some(notice) = &command_spec.fallback_notice {
            eprintln!("[color-ssh] {}", notice);
        }
        log_info!("Using passthrough compatibility mode for SSH command without an interactive controlling terminal");
        return launch::spawn_passthrough(command_spec);
    }

    interactive::run_interactive_ssh_session(command_spec)
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

    if command_spec.stdin_payload.is_none() {
        log_info!("Using passthrough mode for RDP command so FreeRDP can prompt on the controlling terminal");
        return launch::spawn_passthrough(command_spec);
    }

    if !pty_runtime::prefer_direct_pty_runtime() {
        log_info!("Using passthrough compatibility mode for vault-backed RDP command without an interactive controlling terminal");
        return launch::spawn_passthrough(command_spec);
    }

    interactive::run_interactive_rdp_session(command_spec)
}

#[cfg(test)]
#[path = "../test/process.rs"]
mod tests;
