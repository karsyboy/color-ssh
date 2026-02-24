use crate::{Result, config, highlighter, log, log_debug, log_error, log_info};
use std::{
    io::{self, Read, Write},
    process::{Command, ExitCode, Stdio},
    sync::{
        Arc,
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, Instant},
};

const STDOUT_FLUSH_BYTES: usize = 32 * 1024;
const STDOUT_FLUSH_INTERVAL: Duration = Duration::from_millis(25);
const HIGHLIGHT_FLUSH_HINT_BYTES: usize = 256;

enum OutputChunk {
    Owned(String),
    Shared(Arc<String>),
}

impl OutputChunk {
    fn as_str(&self) -> &str {
        match self {
            Self::Owned(chunk) => chunk.as_str(),
            Self::Shared(chunk) => chunk.as_str(),
        }
    }
}

fn requires_immediate_terminal_flush(output: &str) -> bool {
    output.as_bytes().iter().any(|byte| matches!(*byte, b'\r' | 0x1b | 0x08))
}

fn should_flush_immediately(raw_chunk: &str, processed_chunk: &str) -> bool {
    if requires_immediate_terminal_flush(raw_chunk) {
        return true;
    }

    let highlight_changed_chunk = !(raw_chunk.len() == processed_chunk.len() && raw_chunk.as_ptr() == processed_chunk.as_ptr());
    // Prompt-like chunks are short and commonly have no newline. Flush them
    // immediately when highlighting changed the visible output to keep cursor
    // placement responsive.
    highlight_changed_chunk && raw_chunk.len() <= HIGHLIGHT_FLUSH_HINT_BYTES && !raw_chunk.as_bytes().contains(&b'\n')
}

fn map_exit_code(success: bool, code: Option<i32>) -> ExitCode {
    if success {
        ExitCode::SUCCESS
    } else {
        // Clamp exit code to valid u8 range (0-255)
        let clamped_code = code.map_or(1, |status_code| u8::try_from(status_code).unwrap_or(255));
        ExitCode::from(clamped_code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedSshCommand {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

fn build_ssh_command(args: &[String], pass_password: Option<&str>) -> PreparedSshCommand {
    match pass_password {
        Some(password) => {
            let mut wrapped_args = vec!["-e".to_string(), "ssh".to_string()];
            wrapped_args.extend(args.iter().cloned());
            PreparedSshCommand {
                program: "sshpass".to_string(),
                args: wrapped_args,
                env: vec![("SSHPASS".to_string(), password.to_string())],
            }
        }
        None => PreparedSshCommand {
            program: "ssh".to_string(),
            args: args.to_vec(),
            env: Vec::new(),
        },
    }
}

fn command_from_spec(spec: &PreparedSshCommand) -> Command {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    command
}

/// Main process handler for SSH subprocess returns an exit code based on the SSH process status
pub fn process_handler(process_args: Vec<String>, is_non_interactive: bool, pass_password: Option<String>) -> Result<ExitCode> {
    log_info!("Starting SSH process with args: {:?}", process_args);
    log_debug!("Non-interactive mode: {}", is_non_interactive);
    if pass_password.is_some() {
        log_debug!("Password auto-login path enabled for this launch");
    }

    if is_non_interactive {
        log_info!("Using passthrough mode for non-interactive command");
        return spawn_ssh_passthrough(&process_args, pass_password.as_deref());
    }

    // Spawn the SSH process
    let mut child = spawn_ssh(&process_args, pass_password.as_deref()).map_err(|err| {
        log_error!("Failed to spawn SSH process: {}", err);
        err
    })?;

    log_debug!("SSH process spawned successfully (PID: {:?})", child.id());

    let mut stdout = child.stdout.take().ok_or_else(|| {
        log_error!("Failed to capture stdout from SSH process");
        io::Error::other("Failed to capture stdout")
    })?;

    // Create a channel for sending and receiving chunks from SSH
    let (tx, rx): (Sender<OutputChunk>, Receiver<OutputChunk>) = mpsc::channel();

    let reset_color = "\x1b[0m";

    // Spawn thread for processing and displaying chunks
    // This thread applies highlighting and outputs to the terminal
    let processing_thread = thread::Builder::new()
        .name("output-processor".to_string())
        .spawn(move || {
            log_debug!("Output processing thread started");
            let mut chunk_id = 0;
            let mut highlight_scratch = highlighter::HighlightScratch::default();
            let mut color_state = highlighter::AnsiColorState::default();

            // Cache rules and track config version for hot-reload support.
            let (mut cached_rules, mut cached_rule_set, mut cached_version) = {
                let config_guard = match config::get_config().read() {
                    Ok(config_guard) => config_guard,
                    Err(poisoned) => {
                        log_error!("Configuration lock poisoned while loading highlight rules; continuing with recovered state");
                        poisoned.into_inner()
                    }
                };
                (
                    config_guard.metadata.compiled_rules.clone(),
                    config_guard.metadata.compiled_rule_set.clone(),
                    config::current_config_version(),
                )
            };
            let stdout = io::stdout();
            let mut stdout = stdout.lock();
            let mut pending_stdout_bytes = 0usize;
            let mut last_stdout_flush = Instant::now();
            loop {
                match rx.recv_timeout(STDOUT_FLUSH_INTERVAL) {
                    Ok(chunk) => {
                        // Check if config has been reloaded and update rules if needed.
                        let current_version = config::current_config_version();
                        if current_version != cached_version {
                            let config_guard = match config::get_config().read() {
                                Ok(config_guard) => config_guard,
                                Err(poisoned) => {
                                    log_error!("Configuration lock poisoned while reloading highlight rules; continuing with recovered state");
                                    poisoned.into_inner()
                                }
                            };
                            cached_rules = config_guard.metadata.compiled_rules.clone();
                            cached_rule_set = config_guard.metadata.compiled_rule_set.clone();
                            cached_version = current_version;
                            log_debug!("Rules updated due to config reload (version {})", cached_version);
                        }

                        let raw_chunk = chunk.as_str();
                        let processed_chunk = highlighter::process_chunk_with_scratch(
                            raw_chunk,
                            chunk_id,
                            &cached_rules,
                            cached_rule_set.as_ref(),
                            reset_color,
                            &mut color_state,
                            &mut highlight_scratch,
                        );
                        chunk_id += 1;
                        if let Err(err) = stdout.write_all(processed_chunk.as_bytes()) {
                            log_error!("Failed to write processed output to stdout: {}", err);
                            break;
                        }

                        pending_stdout_bytes = pending_stdout_bytes.saturating_add(processed_chunk.len());
                        let immediate_flush = should_flush_immediately(raw_chunk, &processed_chunk);
                        if immediate_flush || pending_stdout_bytes >= STDOUT_FLUSH_BYTES || last_stdout_flush.elapsed() >= STDOUT_FLUSH_INTERVAL {
                            if let Err(err) = stdout.flush() {
                                log_error!("Failed to flush stdout: {}", err);
                                break;
                            }
                            pending_stdout_bytes = 0;
                            last_stdout_flush = Instant::now();
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        // Flush idle prompt/output fragments (e.g. sudo password prompts).
                        if pending_stdout_bytes > 0 && last_stdout_flush.elapsed() >= STDOUT_FLUSH_INTERVAL {
                            if let Err(err) = stdout.flush() {
                                log_error!("Failed to flush stdout on idle timeout: {}", err);
                                break;
                            }
                            pending_stdout_bytes = 0;
                            last_stdout_flush = Instant::now();
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }

            if let Err(err) = stdout.flush() {
                log_error!("Failed to flush stdout at thread end: {}", err);
            }
            log_debug!("Output processing thread finished (processed {} chunks)", chunk_id);
        })
        .map_err(|err| {
            log_error!("Failed to spawn output processing thread: {}", err);
            io::Error::other("Failed to spawn processing thread")
        })?;

    // Buffer for reading data from SSH output
    let mut buffer = [0; 8192];
    let mut pending_utf8: Vec<u8> = Vec::with_capacity(buffer.len());
    let mut total_bytes = 0;

    log_debug!("Starting to read SSH output...");

    let emit_chunk = |tx: &Sender<OutputChunk>, chunk: String| -> bool {
        if chunk.is_empty() {
            return true;
        }

        if log::LOGGER.is_ssh_logging_enabled() {
            let shared_chunk = Arc::new(chunk);
            if let Err(err) = log::LOGGER.log_ssh_raw_shared(shared_chunk.clone()) {
                log_error!("Failed to write SSH log data: {}", err);
            }

            if let Err(err) = tx.send(OutputChunk::Shared(shared_chunk)) {
                log_error!("Failed to send data to processing thread: {}", err);
                return false;
            }
        } else if let Err(err) = tx.send(OutputChunk::Owned(chunk)) {
            log_error!("Failed to send data to processing thread: {}", err);
            return false;
        }

        true
    };

    loop {
        match stdout.read(&mut buffer) {
            Ok(0) => {
                if !pending_utf8.is_empty() {
                    let chunk = String::from_utf8_lossy(&pending_utf8).to_string();
                    pending_utf8.clear();
                    let _ = emit_chunk(&tx, chunk);
                }
                log_debug!("EOF reached (total bytes read: {})", total_bytes);
                break;
            }
            Ok(bytes_read) => {
                total_bytes += bytes_read;

                // Fast path: the common case where this read chunk is complete UTF-8
                // and there are no carry-over bytes from a prior partial sequence.
                if pending_utf8.is_empty() {
                    if let Ok(valid_chunk) = std::str::from_utf8(&buffer[..bytes_read]) {
                        if !emit_chunk(&tx, valid_chunk.to_string()) {
                            break;
                        }
                        continue;
                    }
                }

                pending_utf8.extend_from_slice(&buffer[..bytes_read]);

                let mut chunk = String::new();
                loop {
                    match std::str::from_utf8(&pending_utf8) {
                        Ok(valid) => {
                            chunk.push_str(valid);
                            pending_utf8.clear();
                            break;
                        }
                        Err(err) => {
                            let valid_up_to = err.valid_up_to();
                            if valid_up_to > 0 {
                                let valid = unsafe { std::str::from_utf8_unchecked(&pending_utf8[..valid_up_to]) };
                                chunk.push_str(valid);
                                pending_utf8.drain(..valid_up_to);
                                continue;
                            }
                            if let Some(error_len) = err.error_len() {
                                chunk.push('\u{FFFD}');
                                pending_utf8.drain(..error_len);
                                continue;
                            }
                            // Incomplete UTF-8 sequence at the end; wait for next read.
                            break;
                        }
                    }
                }

                if chunk.is_empty() {
                    continue;
                }

                if !emit_chunk(&tx, chunk) {
                    break;
                }
            }
            Err(err) => {
                log_error!("Error reading from SSH process: {}", err);
                let _ = log::LOGGER.flush_ssh();
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
    if let Err(err) = log::LOGGER.flush_ssh() {
        log_error!("Failed to flush SSH logs: {}", err);
    }

    // Wait for the SSH process to finish and use its status code
    let status = child.wait().map_err(|err| {
        log_error!("Failed to wait for SSH process (PID: {:?}): {}", child.id(), err);
        err
    })?;

    let exit_code = status.code().unwrap_or(1);
    log_info!("SSH process exited with code: {}", exit_code);

    Ok(map_exit_code(status.success(), status.code()))
}

/// Spawns an SSH process with the provided arguments and returns the child process
pub fn spawn_ssh(args: &[String], pass_password: Option<&str>) -> std::io::Result<std::process::Child> {
    let command_spec = build_ssh_command(args, pass_password);
    log_debug!("Spawning {} with args: {:?}", command_spec.program, command_spec.args);

    let child = command_from_spec(&command_spec)
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
fn spawn_ssh_passthrough(args: &[String], pass_password: Option<&str>) -> Result<ExitCode> {
    let command_spec = build_ssh_command(args, pass_password);
    log_debug!("Spawning {} in passthrough mode with args: {:?}", command_spec.program, command_spec.args);

    let status = command_from_spec(&command_spec)
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

    Ok(map_exit_code(status.success(), status.code()))
}

#[cfg(test)]
#[path = "test/process.rs"]
mod tests;
