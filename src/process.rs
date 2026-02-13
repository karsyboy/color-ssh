use crate::{Result, config, highlighter, log_debug, log_error, log_info, log_ssh};
use std::{
    io::{self, Read, Write},
    process::{Command, ExitCode, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

/// Main process handler for SSH subprocess returns an exit code based on the SSH process status
pub fn process_handler(process_args: Vec<String>, is_non_interactive: bool) -> Result<ExitCode> {
    log_info!("Starting SSH process with args: {:?}", process_args);
    log_debug!("Non-interactive mode: {}", is_non_interactive);

    if is_non_interactive {
        log_info!("Using passthrough mode for non-interactive command");
        return spawn_ssh_passthrough(&process_args);
    }

    // Spawn the SSH process
    let mut child = spawn_ssh(&process_args).map_err(|err| {
        log_error!("Failed to spawn SSH process: {}", err);
        err
    })?;

    log_debug!("SSH process spawned successfully (PID: {:?})", child.id());

    let mut stdout = child.stdout.take().ok_or_else(|| {
        log_error!("Failed to capture stdout from SSH process");
        io::Error::other("Failed to capture stdout")
    })?;

    // Create a channel for sending and receiving chunks from SSH
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();

    let reset_color = "\x1b[0m";

    // Spawn thread for processing and displaying chunks
    // This thread applies highlighting and outputs to the terminal
    let processing_thread = thread::Builder::new()
        .name("output-processor".to_string())
        .spawn(move || {
            log_debug!("Output processing thread started");
            let mut chunk_id = 0;

            // Cache rules and track config version for hot-reload support
            let mut cached_rules = config::get_config().read().unwrap().metadata.compiled_rules.clone();
            let mut cached_version = config::get_config().read().unwrap().metadata.version;

            while let Ok(chunk) = rx.recv() {
                // Check if config has been reloaded and update rules if needed
                let current_version = config::get_config().read().unwrap().metadata.version;
                if current_version != cached_version {
                    cached_rules = config::get_config().read().unwrap().metadata.compiled_rules.clone();
                    cached_version = current_version;
                    log_debug!("Rules updated due to config reload (version {})", cached_version);
                }

                let processed_chunk = highlighter::process_chunk(chunk, chunk_id, &cached_rules, reset_color);
                chunk_id += 1;
                print!("{}", processed_chunk);
                if let Err(err) = io::stdout().flush() {
                    log_error!("Failed to flush stdout: {}", err);
                }
            }
            log_debug!("Output processing thread finished (processed {} chunks)", chunk_id);
        })
        .map_err(|err| {
            log_error!("Failed to spawn output processing thread: {}", err);
            io::Error::other("Failed to spawn processing thread")
        })?;

    // Buffer for reading data from SSH output
    let mut buffer = [0; 8192];
    let mut total_bytes = 0;

    log_debug!("Starting to read SSH output...");

    loop {
        match stdout.read(&mut buffer) {
            Ok(0) => {
                log_debug!("EOF reached (total bytes read: {})", total_bytes);
                break;
            }
            Ok(bytes_read) => {
                total_bytes += bytes_read;

                // Convert the read data to a String and send it to the processing thread
                let chunk = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                log_ssh!("{}", chunk);

                if let Err(err) = tx.send(chunk) {
                    log_error!("Failed to send data to processing thread: {}", err);
                    break;
                }
            }
            Err(err) => {
                log_error!("Error reading from SSH process: {}", err);
                return Err(err.into());
            }
        }
    }

    // Drop the sender to signal the processing thread to finish
    drop(tx);

    // Wait for the processing thread to finish
    if let Err(err) = processing_thread.join() {
        log_error!("Processing thread panicked: {:?}", err);
    }

    // Ensure all output is flushed to terminal
    if let Err(err) = io::stdout().flush() {
        log_error!("Failed to flush stdout after processing: {}", err);
    }

    // Wait for the SSH process to finish and use its status code
    let status = child.wait().map_err(|err| {
        log_error!("Failed to wait for SSH process (PID: {:?}): {}", child.id(), err);
        err
    })?;

    let exit_code = status.code().unwrap_or(1);
    log_info!("SSH process exited with code: {}", exit_code);

    if status.success() {
        Ok(ExitCode::SUCCESS)
    } else {
        // Clamp exit code to valid u8 range (0-255)
        let clamped_code = u8::try_from(exit_code).unwrap_or(255);
        Ok(ExitCode::from(clamped_code))
    }
}

/// Spawns an SSH process with the provided arguments and returns the child process
pub fn spawn_ssh(args: &[String]) -> std::io::Result<std::process::Child> {
    log_debug!("Spawning SSH with args: {:?}", args);

    // Force PTY allocation to ensure proper interactive behavior and prevent buffering issues
    let mut ssh_args = vec!["-t".to_string()];
    ssh_args.extend_from_slice(args);

    let child = Command::new("ssh")
        .args(&ssh_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|err| {
            log_error!("Failed to spawn SSH command: {}", err);
            err
        })?;

    log_debug!("SSH process spawned (PID: {:?})", child.id());
    Ok(child)
}

/// Spawns SSH for non-interactive commands with direct stdout passthrough returns SSH exit code
fn spawn_ssh_passthrough(args: &[String]) -> Result<ExitCode> {
    log_debug!("Spawning SSH in passthrough mode with args: {:?}", args);

    let status = Command::new("ssh")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|err| {
            log_error!("Failed to execute SSH command in passthrough mode: {}", err);
            err
        })?;

    let exit_code = status.code().unwrap_or(1);
    log_info!("SSH passthrough process exited with code: {}", exit_code);

    if status.success() {
        Ok(ExitCode::SUCCESS)
    } else {
        // Clamp exit code to valid u8 range (0-255)
        let clamped_code = u8::try_from(exit_code).unwrap_or(255);
        Ok(ExitCode::from(clamped_code))
    }
}
