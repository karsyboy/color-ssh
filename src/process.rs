use crate::{Result, config, highlighter, log, log_debug, log_error, log_info};
use std::{
    io::{self, Read, Write},
    process::{Command, ExitCode, Stdio},
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::{Duration, Instant},
};

const STDOUT_FLUSH_BYTES: usize = 32 * 1024;
const STDOUT_FLUSH_INTERVAL: Duration = Duration::from_millis(50);

fn requires_immediate_terminal_flush(output: &[u8]) -> bool {
    output.iter().any(|byte| matches!(*byte, b'\r' | 0x1b | 0x08))
}

fn update_alt_screen_mode(chunk: &[u8], alt_screen_active: &mut bool) -> bool {
    let mut toggled = false;
    let mut idx = 0usize;

    while idx < chunk.len() {
        let rest = &chunk[idx..];

        if rest.starts_with(b"\x1b[?1049h") || rest.starts_with(b"\x1b[?1047h") || rest.starts_with(b"\x1b[?47h") {
            *alt_screen_active = true;
            toggled = true;
        } else if rest.starts_with(b"\x1b[?1049l") || rest.starts_with(b"\x1b[?1047l") || rest.starts_with(b"\x1b[?47l") {
            *alt_screen_active = false;
            toggled = true;
        }

        idx += 1;
    }

    toggled
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
    let (tx, rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = mpsc::channel();

    let reset_color = "\x1b[0m";

    // Spawn thread for processing and displaying chunks
    // This thread applies highlighting and outputs to the terminal
    let processing_thread = thread::Builder::new()
        .name("output-processor".to_string())
        .spawn(move || {
            log_debug!("Output processing thread started");
            let mut chunk_id = 0;
            let mut highlight_scratch = highlighter::HighlightScratch::default();

            // Cache rules and track config version for hot-reload support.
            let (mut cached_rules, mut cached_rule_set, mut cached_version) = {
                let config_guard = config::get_config().read().unwrap();
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
            let mut alt_screen_active = false;

            while let Ok(chunk) = rx.recv() {
                // Check if config has been reloaded and update rules if needed.
                let current_version = config::current_config_version();
                if current_version != cached_version {
                    let config_guard = config::get_config().read().unwrap();
                    cached_rules = config_guard.metadata.compiled_rules.clone();
                    cached_rule_set = config_guard.metadata.compiled_rule_set.clone();
                    cached_version = current_version;
                    log_debug!("Rules updated due to config reload (version {})", cached_version);
                }

                let alt_screen_toggled = update_alt_screen_mode(&chunk, &mut alt_screen_active);
                if alt_screen_active || alt_screen_toggled {
                    if let Err(err) = stdout.write_all(&chunk) {
                        log_error!("Failed to write passthrough output to stdout: {}", err);
                        break;
                    }
                    pending_stdout_bytes = pending_stdout_bytes.saturating_add(chunk.len());
                    let immediate_flush = requires_immediate_terminal_flush(&chunk);
                    if immediate_flush || pending_stdout_bytes >= STDOUT_FLUSH_BYTES || last_stdout_flush.elapsed() >= STDOUT_FLUSH_INTERVAL {
                        if let Err(err) = stdout.flush() {
                            log_error!("Failed to flush stdout: {}", err);
                            break;
                        }
                        pending_stdout_bytes = 0;
                        last_stdout_flush = Instant::now();
                    }
                    continue;
                }

                let chunk_text = String::from_utf8_lossy(&chunk);
                let processed_chunk = highlighter::process_chunk_with_scratch(
                    &chunk_text,
                    chunk_id,
                    &cached_rules,
                    cached_rule_set.as_ref(),
                    reset_color,
                    &mut highlight_scratch,
                );
                chunk_id += 1;
                let processed_bytes = processed_chunk.as_bytes();
                if let Err(err) = stdout.write_all(processed_bytes) {
                    log_error!("Failed to write processed output to stdout: {}", err);
                    break;
                }

                pending_stdout_bytes = pending_stdout_bytes.saturating_add(processed_bytes.len());
                let immediate_flush = requires_immediate_terminal_flush(processed_bytes);
                if immediate_flush || pending_stdout_bytes >= STDOUT_FLUSH_BYTES || last_stdout_flush.elapsed() >= STDOUT_FLUSH_INTERVAL {
                    if let Err(err) = stdout.flush() {
                        log_error!("Failed to flush stdout: {}", err);
                        break;
                    }
                    pending_stdout_bytes = 0;
                    last_stdout_flush = Instant::now();
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
                let chunk = buffer[..bytes_read].to_vec();
                let chunk_for_log = String::from_utf8_lossy(&chunk);
                if let Err(err) = log::LOGGER.log_ssh_raw(&chunk_for_log) {
                    log_error!("Failed to write SSH log data: {}", err);
                }

                if let Err(err) = tx.send(chunk) {
                    log_error!("Failed to send data to processing thread: {}", err);
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
pub fn spawn_ssh(args: &[String]) -> std::io::Result<std::process::Child> {
    log_debug!("Spawning SSH with args: {:?}", args);

    let child = Command::new("ssh")
        .args(args)
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

    Ok(map_exit_code(status.success(), status.code()))
}

#[cfg(test)]
mod tests {
    use super::{map_exit_code, requires_immediate_terminal_flush, update_alt_screen_mode};
    use std::process::ExitCode;

    #[test]
    fn returns_success_exit_code_for_success_status() {
        assert_eq!(map_exit_code(true, Some(0)), ExitCode::SUCCESS);
    }

    #[test]
    fn preserves_non_zero_exit_status_in_u8_range() {
        assert_eq!(map_exit_code(false, Some(23)), ExitCode::from(23));
    }

    #[test]
    fn clamps_out_of_range_status_and_defaults_missing_to_one() {
        assert_eq!(map_exit_code(false, Some(300)), ExitCode::from(255));
        assert_eq!(map_exit_code(false, Some(-1)), ExitCode::from(255));
        assert_eq!(map_exit_code(false, None), ExitCode::from(1));
    }

    #[test]
    fn immediate_flush_detects_cursor_control_sequences() {
        assert!(requires_immediate_terminal_flush(b"\rprompt"));
        assert!(requires_immediate_terminal_flush(b"\x1b[2J"));
        assert!(requires_immediate_terminal_flush(b"abc\x08"));
        assert!(!requires_immediate_terminal_flush(b"plain text\nnext line"));
    }

    #[test]
    fn alt_screen_mode_toggles_on_and_off() {
        let mut alt_screen_active = false;
        assert!(update_alt_screen_mode(b"\x1b[?1049h", &mut alt_screen_active));
        assert!(alt_screen_active);

        assert!(update_alt_screen_mode(b"\x1b[?1049l", &mut alt_screen_active));
        assert!(!alt_screen_active);
    }
}
