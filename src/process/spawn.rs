use super::command_spec::{PreparedCommand, command_from_spec};
use super::map_exit_code;
use crate::{Result, log_debug, log_debug_raw, log_error, log_info};
use std::io::{self, Write};
use std::process::{Child, ExitCode, Stdio};

fn write_stdin_payload(child: &mut Child, stdin_payload: crate::auth::secret::SensitiveString) -> io::Result<()> {
    let Some(mut stdin) = child.stdin.take() else {
        return Ok(());
    };

    use crate::auth::secret::ExposeSecret;
    stdin.write_all(stdin_payload.expose_secret().as_bytes())?;
    stdin.flush()?;
    Ok(())
}

pub(crate) fn spawn_command(command_spec: PreparedCommand, stdout: Stdio, stderr: Stdio) -> io::Result<Child> {
    log_debug!(
        "Spawning {}: arg_count={} env_override_count={} stdin_payload={}",
        command_spec.program,
        command_spec.args.len(),
        command_spec.env.len(),
        command_spec.stdin_payload.is_some()
    );
    log_debug_raw!("Spawning {} with args: {:?}", command_spec.program, command_spec.args);

    let stdin_mode = if command_spec.stdin_payload.is_some() {
        Stdio::piped()
    } else {
        Stdio::inherit()
    };

    let mut child = command_from_spec(&command_spec)?
        .stdin(stdin_mode)
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|err| {
            log_error!("Failed to spawn {} command: {}", command_spec.program, err);
            err
        })?;

    if let Some(stdin_payload) = command_spec.stdin_payload
        && let Err(err) = write_stdin_payload(&mut child, stdin_payload)
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(err);
    }

    log_debug!("{} process spawned (PID: {:?})", command_spec.program, child.id());
    Ok(child)
}

pub(crate) fn spawn_passthrough(command_spec: PreparedCommand) -> Result<ExitCode> {
    log_debug!(
        "Spawning {} in passthrough mode: arg_count={} env_override_count={} stdin_payload={}",
        command_spec.program,
        command_spec.args.len(),
        command_spec.env.len(),
        command_spec.stdin_payload.is_some()
    );
    log_debug_raw!("Spawning {} in passthrough mode with args: {:?}", command_spec.program, command_spec.args);

    let mut child = spawn_command(command_spec, Stdio::inherit(), Stdio::inherit()).map_err(|err| {
        log_error!("Failed to execute command in passthrough mode: {}", err);
        err
    })?;

    let status = child.wait().map_err(|err| {
        log_error!("Failed to wait for passthrough process: {}", err);
        err
    })?;

    let exit_code = status.code().unwrap_or(1);
    log_info!("Passthrough process exited with code: {}", exit_code);

    Ok(map_exit_code(status.success(), status.code()))
}
