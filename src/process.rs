//! SSH subprocess orchestration.

mod command;
mod stream;

use crate::{Result, log_debug, log_debug_raw, log_error, log_info, log_warn, ssh_args};
use std::process::ExitCode;

fn map_exit_code(success: bool, code: Option<i32>) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        // Clamp exit code to valid u8 range (0-255).
        let clamped_code = code.map_or(1, |status_code| u8::try_from(status_code).unwrap_or(255));
        ExitCode::from(clamped_code)
    }
}

/// Run an SSH subprocess and return the final process exit code.
pub fn process_handler(process_args: Vec<String>, is_non_interactive: bool, explicit_pass_entry: Option<String>) -> Result<ExitCode> {
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
        command::build_plain_ssh_command(&process_args)
    } else {
        command::build_ssh_command(&process_args, explicit_pass_entry.as_deref())?
    };

    if let Some(notice) = &command_spec.fallback_notice {
        log_warn!("{}", notice);
        eprintln!("[color-ssh] {}", notice);
    }

    if is_non_interactive {
        log_info!("Using passthrough mode for non-interactive command");
        return command::spawn_ssh_passthrough(&command_spec);
    }

    let child = command::spawn_ssh(&command_spec).map_err(|err| {
        log_error!("Failed to spawn SSH process: {}", err);
        err
    })?;
    log_debug!("SSH process spawned successfully (PID: {:?})", child.id());

    stream::run_interactive_ssh(child)
}

#[cfg(test)]
#[path = "test/process.rs"]
mod tests;
