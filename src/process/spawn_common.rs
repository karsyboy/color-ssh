//! Shared PTY/captured process spawn helpers.

use crate::command_path;
use std::io::{self, Read, Write};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use portable_pty::{Child as PtyChild, CommandBuilder, MasterPty, PtySize, native_pty_system};

pub(crate) fn io_other_error(err: impl std::fmt::Display) -> io::Error {
    io::Error::other(err.to_string())
}

pub(crate) struct SpawnedPtyCommand {
    pub(crate) child: Arc<Mutex<Box<dyn PtyChild + Send + Sync>>>,
    pub(crate) master: Box<dyn MasterPty + Send>,
    pub(crate) reader: Box<dyn Read + Send>,
    pub(crate) writer: Box<dyn Write + Send>,
}

pub(crate) struct SpawnedCapturedCommand {
    pub(crate) child: Arc<Mutex<Box<dyn PtyChild + Send + Sync>>>,
    pub(crate) stdout: Box<dyn Read + Send>,
    pub(crate) stderr: Box<dyn Read + Send>,
}

pub(crate) fn spawn_pty_command(program: &str, args: &[String], env: &[(String, String)], rows: u16, cols: u16) -> io::Result<SpawnedPtyCommand> {
    let pty_system = native_pty_system();
    let rows = rows.max(1);
    let cols = cols.max(1);

    let pty_pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(io_other_error)?;

    let cmd = pty_command_builder(program, args, env)?;
    let child = Arc::new(Mutex::new(pty_pair.slave.spawn_command(cmd).map_err(io_other_error)?));
    drop(pty_pair.slave);

    let reader = pty_pair.master.try_clone_reader().map_err(io_other_error)?;
    let writer = pty_pair.master.take_writer().map_err(io_other_error)?;

    Ok(SpawnedPtyCommand {
        child,
        master: pty_pair.master,
        reader,
        writer,
    })
}

pub(crate) fn spawn_captured_command(
    program: &str,
    args: &[String],
    env: &[(String, String)],
    stdin_payload: Option<&[u8]>,
) -> io::Result<SpawnedCapturedCommand> {
    let program_path = command_path::resolve_known_command_path(program)?;
    let mut cmd = std::process::Command::new(program_path);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin_payload.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }

    for arg in args {
        cmd.arg(arg);
    }
    for (key, value) in env {
        cmd.env(key, value);
    }

    let mut child_process = cmd.spawn().map_err(io_other_error)?;
    let stdout = child_process
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("captured session stdout pipe missing"))?;
    let stderr = child_process
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("captured session stderr pipe missing"))?;

    if let Some(stdin_payload) = stdin_payload {
        let Some(mut stdin) = child_process.stdin.take() else {
            let _ = child_process.kill();
            let _ = child_process.wait();
            return Err(io::Error::other("captured session stdin pipe missing"));
        };

        if let Err(err) = stdin.write_all(stdin_payload).and_then(|_| stdin.flush()) {
            let _ = child_process.kill();
            let _ = child_process.wait();
            return Err(err);
        }
    }

    Ok(SpawnedCapturedCommand {
        child: Arc::new(Mutex::new(Box::new(child_process))),
        stdout: Box::new(stdout),
        stderr: Box::new(stderr),
    })
}

fn pty_command_builder(program: &str, args: &[String], env: &[(String, String)]) -> io::Result<CommandBuilder> {
    let program_path = command_path::resolve_known_command_path(program)?;
    let mut builder = CommandBuilder::new(program_path.as_os_str());
    for arg in args {
        builder.arg(arg);
    }
    for (key, value) in env {
        builder.env(key, value);
    }
    Ok(builder)
}
