//! SSH process management and output handling
//!
//! Handles:
//! - Spawning SSH subprocess with proper I/O configuration
//! - Reading and buffering SSH output
//! - Applying syntax highlighting to output chunks
//! - Managing non-interactive SSH commands
//!
//! # Process Flow
//! 1. Detect if command is interactive or non-interactive
//! 2. Spawn SSH process with appropriate stdio configuration
//! 3. For interactive: read output in chunks, apply highlighting, display
//! 4. For non-interactive: passthrough directly without processing

use crate::{Result, config, highlighter, log_debug, log_error, log_info, log_ssh};
use std::{
    io::{self, BufReader, Read, Write},
    process::{Command, ExitCode, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

/// Main process handler for SSH subprocess
///
/// Manages the SSH subprocess lifecycle, including spawning, output processing,
/// and exit code handling. Automatically detects non-interactive commands and
/// uses passthrough mode for them.
///
/// # Arguments
/// * `process_args` - Command-line arguments to pass to SSH
/// * `is_non_interactive` - Whether this is a non-interactive command (-G, -V, etc.)
///
/// # Returns
/// Exit code from the SSH process
pub fn process_handler(process_args: Vec<String>, is_non_interactive: bool) -> Result<ExitCode> {
    log_info!("Starting SSH process with args: {:?}", process_args);
    log_debug!("Non-interactive mode: {}", is_non_interactive);

    // For non-interactive commands, use direct passthrough
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

    let stdout = child.stdout.take().ok_or_else(|| {
        log_error!("Failed to capture stdout from SSH process");
        io::Error::other("Failed to capture stdout")
    })?;

    let mut reader = BufReader::new(stdout);

    // Create a channel for sending and receiving output chunks
    let (tx, rx): (Sender<String>, Receiver<String>) = mpsc::channel();

    let reset_color = "\x1b[0m"; // ANSI reset color sequence

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

                let processed = highlighter::process_chunk(chunk, chunk_id, &cached_rules, reset_color);
                chunk_id += 1;
                print!("{}", processed); // Print the processed chunk
                if let Err(e) = io::stdout().flush() {
                    log_error!("Failed to flush stdout: {}", e);
                }
            }
            log_debug!("Output processing thread finished (processed {} chunks)", chunk_id);
        })
        .map_err(|err| {
            log_error!("Failed to spawn output processing thread: {}", err);
            io::Error::other("Failed to spawn processing thread")
        })?;

    // Buffer for reading data from SSH output (4KB chunks)
    let mut buffer = [0; 4096];
    let mut total_bytes = 0;

    log_debug!("Starting to read SSH output...");

    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                log_debug!("EOF reached (total bytes read: {})", total_bytes);
                break; // Exit loop when EOF is reached
            }
            Ok(n) => {
                total_bytes += n;
                // Convert the read data to a String and send it to the processing thread
                let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
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

/// Spawns an SSH process with the provided arguments.
///
/// Configures the process with:
/// - Inherited stdin (for user input)
/// - Piped stdout (for processing and highlighting)
/// - Inherited stderr (for error messages)
///
/// # Arguments
/// * `args` - CLI arguments provided by the user
///
/// # Returns
/// The spawned child process or an I/O error
pub fn spawn_ssh(args: &[String]) -> std::io::Result<std::process::Child> {
    log_debug!("Spawning SSH with args: {:?}", args);

    let child = Command::new("ssh")
        .args(args)
        .stdin(Stdio::inherit()) // Inherit the input from the current terminal
        .stdout(Stdio::piped()) // Pipe the output for processing
        .stderr(Stdio::inherit()) // Inherit the error stream from the SSH process
        .spawn()
        .map_err(|err| {
            log_error!("Failed to spawn SSH command: {}", err);
            err
        })?;

    log_debug!("SSH process spawned (PID: {:?})", child.id());
    Ok(child)
}

/// Spawns SSH for non-interactive commands with direct stdout passthrough.
///
/// Used for commands like -G, -V, -O, -Q, -T that don't need highlighting.
/// All stdio streams are inherited for direct passthrough.
///
/// # Arguments
/// * `args` - CLI arguments provided by the user
///
/// # Returns
/// The exit code from the SSH process
fn spawn_ssh_passthrough(args: &[String]) -> Result<ExitCode> {
    log_debug!("Spawning SSH in passthrough mode with args: {:?}", args);

    let status = Command::new("ssh")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit()) // Pass through directly, no buffering
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
