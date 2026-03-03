use crate::auth::{self, agent, ipc::UnlockPolicy, secret::ExposeSecret, transport, vault};
use crate::ssh_config::SshHost;
use crate::{Result, command_path, config, highlighter, log, log_debug, log_debug_raw, log_error, log_info, log_warn};
use std::{
    io::IsTerminal,
    io::{self, Read, Write},
    process::{Command, ExitCode, Stdio},
    sync::{
        Arc,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

const STDOUT_FLUSH_BYTES: usize = 32 * 1024;
const STDOUT_FLUSH_INTERVAL: Duration = Duration::from_millis(25);
const HIGHLIGHT_FLUSH_HINT_BYTES: usize = 256;
const OUTPUT_QUEUE_CAPACITY: usize = 256;

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

#[derive(Debug)]
struct PreparedSshCommand {
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    fallback_notice: Option<String>,
}

fn build_plain_ssh_command(args: &[String]) -> PreparedSshCommand {
    PreparedSshCommand {
        program: "ssh".to_string(),
        args: args.to_vec(),
        env: Vec::new(),
        fallback_notice: None,
    }
}

fn command_from_spec(spec: &PreparedSshCommand) -> io::Result<Command> {
    let program_path = command_path::resolve_known_command_path(&spec.program)?;
    let mut command = Command::new(&program_path);
    command.args(&spec.args);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    Ok(command)
}

fn extract_ssh_destination(ssh_args: &[String]) -> Option<String> {
    let flags_with_args = [
        "-b", "-B", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o", "-p", "-P", "-Q", "-R", "-S", "-w", "-W",
    ];

    let mut skip_next = false;
    for arg in ssh_args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with('-') {
            if flags_with_args.contains(&arg.as_str()) {
                skip_next = true;
            }
            continue;
        }
        return Some(arg.split_once('@').map_or_else(|| arg.clone(), |(_, host)| host.to_string()));
    }
    None
}

fn resolve_pass_entry_from_hosts(destination: &str, explicit_entry: Option<&str>, hosts: &[SshHost]) -> Option<String> {
    if let Some(explicit_entry) = explicit_entry {
        return Some(explicit_entry.to_string());
    }

    if let Some(host) = hosts.iter().find(|host| host.name == destination && host.pass_key.is_some()) {
        return host.pass_key.clone();
    }

    let mut hostname_matches = hosts
        .iter()
        .filter(|host| host.hostname.as_deref() == Some(destination) && host.pass_key.is_some());
    let first = hostname_matches.next()?;
    if hostname_matches.next().is_some() {
        return None;
    }

    first.pass_key.clone()
}

fn current_unlock_policy() -> UnlockPolicy {
    let auth_settings = config::auth_settings();
    UnlockPolicy::new(auth_settings.unlock_idle_timeout_seconds, auth_settings.unlock_absolute_timeout_seconds)
}

fn unlock_agent_interactively(client: &agent::AgentClient) -> io::Result<()> {
    let policy = current_unlock_policy();
    for attempt in 1..=3 {
        log_debug!("Prompting for password vault unlock (attempt {} of 3)", attempt);
        let master_password = auth::prompt_hidden_secret("Enter vault master password: ")?;
        if master_password.expose_secret().is_empty() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "master password cannot be empty"));
        }

        match client.unlock(master_password.expose_secret(), policy.clone()) {
            Ok(_) => {
                log_debug!("Interactive password vault unlock succeeded");
                return Ok(());
            }
            Err(agent::AgentError::InvalidMasterPassword) => {
                log_debug!("Interactive password vault unlock failed due to invalid master password");
                if attempt == 3 {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "failed to unlock password vault after 3 attempts",
                    ));
                }
                eprintln!("Invalid master password. Try again.");
            }
            Err(agent::AgentError::VaultNotInitialized) => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "password vault is not initialized; run `cossh vault init` or `cossh vault add <name>`",
                ));
            }
            Err(err) => {
                log_debug!("Interactive password vault unlock failed: {}", err);
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()));
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "failed to unlock password vault after 3 attempts",
    ))
}

fn build_ssh_command(args: &[String], explicit_pass_entry: Option<&str>) -> io::Result<PreparedSshCommand> {
    let mut command = build_plain_ssh_command(args);
    let auth_settings = config::auth_settings();
    if !auth_settings.direct_password_autologin {
        log_debug!("Direct password auto-login disabled in auth settings");
        return Ok(command);
    }

    let pass_entry_source = if explicit_pass_entry.is_some() {
        "direct override"
    } else {
        "ssh config lookup"
    };
    let Some(pass_entry_name) = explicit_pass_entry.map(|name| name.to_string()).or_else(|| {
        let destination = extract_ssh_destination(args)?;
        let hosts = crate::ssh_config::load_ssh_host_tree().ok()?.hosts;
        resolve_pass_entry_from_hosts(&destination, None, &hosts)
    }) else {
        log_debug!("No password vault entry resolved for direct SSH launch");
        return Ok(command);
    };
    log_debug!("Resolved password vault entry for direct SSH launch via {}", pass_entry_source);

    if !vault::validate_entry_name(&pass_entry_name) {
        log_debug!("Resolved password vault entry name was invalid");
        command.fallback_notice = Some(
            "Password auto-login is unavailable because the requested password entry name is invalid; continuing with the standard SSH password prompt."
                .to_string(),
        );
        return Ok(command);
    }

    let client = agent::AgentClient::new().map_err(|err| io::Error::other(err.to_string()))?;
    let entry_status = match client.entry_status(&pass_entry_name) {
        Ok(status) => status,
        Err(agent::AgentError::VaultNotInitialized) => {
            log_debug!("Password vault is not initialized during direct SSH launch");
            command.fallback_notice = Some(
                "Password auto-login is unavailable because the password vault is not initialized; continuing with the standard SSH password prompt."
                    .to_string(),
            );
            return Ok(command);
        }
        Err(err) => {
            log_debug!("Password vault lookup failed during direct SSH launch: {}", err);
            command.fallback_notice = Some(format!(
                "Password auto-login is unavailable because the password vault could not be queried ({err}); continuing with the standard SSH password prompt."
            ));
            return Ok(command);
        }
    };

    if !entry_status.exists {
        log_debug!("Password vault entry '{}' was not found", pass_entry_name);
        command.fallback_notice = Some(format!(
            "Password auto-login is unavailable because vault entry '{}' was not found; continuing with the standard SSH password prompt.",
            pass_entry_name
        ));
        return Ok(command);
    }

    if !entry_status.status.unlocked {
        log_debug!("Password vault was locked during direct SSH launch");
        if !io::stdin().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "password vault is locked; run `cossh vault unlock`",
            ));
        }
        unlock_agent_interactively(&client)?;
        log_debug!("Retrying password vault entry lookup after unlock");
        let entry_status = client
            .entry_status(&pass_entry_name)
            .map_err(|err| io::Error::new(io::ErrorKind::PermissionDenied, err.to_string()))?;
        if !entry_status.exists {
            log_debug!("Password vault entry '{}' was not found after unlock", pass_entry_name);
            command.fallback_notice = Some(format!(
                "Password auto-login is unavailable because vault entry '{}' was not found; continuing with the standard SSH password prompt.",
                pass_entry_name
            ));
            return Ok(command);
        }
        if !entry_status.status.unlocked {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "password vault remains locked after unlock attempt",
            ));
        }
    }

    let askpass_token = match client.authorize_askpass(&pass_entry_name) {
        Ok(token) => token,
        Err(err) => {
            log_debug!("Failed to authorize internal askpass token: {}", err);
            command.fallback_notice = Some(format!(
                "Password auto-login is unavailable because a vault access token could not be issued ({err}); continuing with the standard SSH password prompt."
            ));
            return Ok(command);
        }
    };

    if let Err(err) = transport::configure_internal_askpass_env(&mut command.env, askpass_token.expose_secret()) {
        log_debug!("Failed to configure internal askpass helper: {}", err);
        command.fallback_notice = Some(format!(
            "Password auto-login is unavailable because the internal askpass helper could not be configured ({err}); continuing with the standard SSH password prompt."
        ));
        return Ok(command);
    }
    log_debug!("Configured internal askpass helper for direct SSH launch");
    Ok(command)
}

/// Main process handler for SSH subprocess returns an exit code based on the SSH process status
pub fn process_handler(process_args: Vec<String>, is_non_interactive: bool, explicit_pass_entry: Option<String>) -> Result<ExitCode> {
    log_info!(
        "Starting SSH process: interactive={} ssh_arg_count={} explicit_pass_entry={} destination_resolved={}",
        !is_non_interactive,
        process_args.len(),
        explicit_pass_entry.is_some(),
        extract_ssh_destination(&process_args).is_some()
    );
    log_debug_raw!("Starting SSH process with args: {:?}", process_args);
    log_debug!("Non-interactive mode: {}", is_non_interactive);

    let command_spec = if is_non_interactive {
        build_plain_ssh_command(&process_args)
    } else {
        build_ssh_command(&process_args, explicit_pass_entry.as_deref())?
    };

    if let Some(notice) = &command_spec.fallback_notice {
        log_warn!("{}", notice);
        eprintln!("[color-ssh] {}", notice);
    }

    if is_non_interactive {
        log_info!("Using passthrough mode for non-interactive command");
        return spawn_ssh_passthrough(&command_spec);
    }

    // Spawn the SSH process
    let mut child = spawn_ssh(&command_spec).map_err(|err| {
        log_error!("Failed to spawn SSH process: {}", err);
        err
    })?;

    log_debug!("SSH process spawned successfully (PID: {:?})", child.id());

    let mut stdout = child.stdout.take().ok_or_else(|| {
        log_error!("Failed to capture stdout from SSH process");
        io::Error::other("Failed to capture stdout")
    })?;

    // Create a channel for sending and receiving chunks from SSH
    let (tx, rx): (SyncSender<OutputChunk>, Receiver<OutputChunk>) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);

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

    let emit_chunk = |tx: &SyncSender<OutputChunk>, chunk: String| -> bool {
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
                if pending_utf8.is_empty()
                    && let Ok(valid_chunk) = std::str::from_utf8(&buffer[..bytes_read])
                {
                    if !emit_chunk(&tx, valid_chunk.to_string()) {
                        break;
                    }
                    continue;
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
                            if valid_up_to > 0
                                && let Ok(valid) = std::str::from_utf8(&pending_utf8[..valid_up_to])
                            {
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
fn spawn_ssh(command_spec: &PreparedSshCommand) -> std::io::Result<std::process::Child> {
    log_debug!(
        "Spawning {}: arg_count={} env_override_count={}",
        command_spec.program,
        command_spec.args.len(),
        command_spec.env.len()
    );
    log_debug_raw!("Spawning {} with args: {:?}", command_spec.program, command_spec.args);

    let mut command = command_from_spec(command_spec)?;
    command.stdin(Stdio::inherit()).stdout(Stdio::piped()).stderr(Stdio::inherit());

    let child = command.spawn().map_err(|err| {
        log_error!("Failed to spawn SSH command: {}", err);
        err
    })?;

    log_debug!("SSH process spawned (PID: {:?})", child.id());
    Ok(child)
}

/// Spawns SSH for non-interactive commands with direct stdout passthrough returns SSH exit code
fn spawn_ssh_passthrough(command_spec: &PreparedSshCommand) -> Result<ExitCode> {
    log_debug!(
        "Spawning {} in passthrough mode: arg_count={} env_override_count={}",
        command_spec.program,
        command_spec.args.len(),
        command_spec.env.len()
    );
    log_debug_raw!("Spawning {} in passthrough mode with args: {:?}", command_spec.program, command_spec.args);

    let mut child = command_from_spec(command_spec)?
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|err| {
            log_error!("Failed to execute SSH command in passthrough mode: {}", err);
            err
        })?;

    let status = child.wait().map_err(|err| {
        log_error!("Failed to wait for SSH passthrough process: {}", err);
        err
    })?;

    let exit_code = status.code().unwrap_or(1);
    log_info!("SSH passthrough process exited with code: {}", exit_code);

    Ok(map_exit_code(status.success(), status.code()))
}

#[cfg(test)]
#[path = "test/process.rs"]
mod tests;
