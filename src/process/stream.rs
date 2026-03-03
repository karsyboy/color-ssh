//! Interactive SSH output streaming and highlighting.

use super::exit::map_exit_code;
use crate::{Result, config, highlighter, log, log_debug, log_error};
use std::io::{self, Read, Write};
use std::process::{Child, ExitCode};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
};
use std::thread;
use std::time::{Duration, Instant};

const STDOUT_FLUSH_BYTES: usize = 32 * 1024;
const STDOUT_FLUSH_INTERVAL: Duration = Duration::from_millis(25);
const HIGHLIGHT_FLUSH_HINT_BYTES: usize = 256;
const OUTPUT_QUEUE_CAPACITY: usize = 256;
const RESET_COLOR: &str = "\x1b[0m";

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

#[derive(Debug)]
struct StdoutFlushState {
    pending_bytes: usize,
    last_flush_at: Instant,
}

impl StdoutFlushState {
    fn new() -> Self {
        Self {
            pending_bytes: 0,
            last_flush_at: Instant::now(),
        }
    }

    fn record_write(&mut self, bytes_written: usize) {
        self.pending_bytes = self.pending_bytes.saturating_add(bytes_written);
    }

    fn flush_if_needed(&mut self, stdout: &mut impl Write, raw_chunk: &str, processed_chunk: &str) -> io::Result<()> {
        let immediate_flush = should_flush_immediately(raw_chunk, processed_chunk);
        if immediate_flush || self.pending_bytes >= STDOUT_FLUSH_BYTES || self.last_flush_at.elapsed() >= STDOUT_FLUSH_INTERVAL {
            stdout.flush()?;
            self.pending_bytes = 0;
            self.last_flush_at = Instant::now();
        }
        Ok(())
    }

    fn flush_on_idle(&mut self, stdout: &mut impl Write) -> io::Result<()> {
        if self.pending_bytes > 0 && self.last_flush_at.elapsed() >= STDOUT_FLUSH_INTERVAL {
            stdout.flush()?;
            self.pending_bytes = 0;
            self.last_flush_at = Instant::now();
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Utf8ChunkDecoder {
    pending_utf8: Vec<u8>,
}

impl Utf8ChunkDecoder {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            pending_utf8: Vec::with_capacity(capacity),
        }
    }

    fn decode_read(&mut self, bytes: &[u8]) -> Option<String> {
        if self.pending_utf8.is_empty()
            && let Ok(valid_chunk) = std::str::from_utf8(bytes)
        {
            return Some(valid_chunk.to_string());
        }

        self.pending_utf8.extend_from_slice(bytes);
        self.take_decoded_chunk()
    }

    fn finish(&mut self) -> Option<String> {
        if self.pending_utf8.is_empty() {
            None
        } else {
            let chunk = String::from_utf8_lossy(&self.pending_utf8).to_string();
            self.pending_utf8.clear();
            Some(chunk)
        }
    }

    fn take_decoded_chunk(&mut self) -> Option<String> {
        let mut chunk = String::new();

        loop {
            match std::str::from_utf8(&self.pending_utf8) {
                Ok(valid) => {
                    chunk.push_str(valid);
                    self.pending_utf8.clear();
                    break;
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0
                        && let Ok(valid) = std::str::from_utf8(&self.pending_utf8[..valid_up_to])
                    {
                        chunk.push_str(valid);
                        self.pending_utf8.drain(..valid_up_to);
                        continue;
                    }
                    if let Some(error_len) = err.error_len() {
                        chunk.push('\u{FFFD}');
                        self.pending_utf8.drain(..error_len);
                        continue;
                    }
                    break;
                }
            }
        }

        (!chunk.is_empty()).then_some(chunk)
    }
}

#[derive(Debug, Default)]
struct HighlightRuleCache {
    rules: Vec<highlighter::CompiledHighlightRule>,
    rule_set: Option<regex::RegexSet>,
    version: u64,
}

impl HighlightRuleCache {
    fn load() -> Self {
        let (rules, rule_set) = config::with_current_config("loading highlight rules", |cfg| {
            (cfg.metadata.compiled_rules.clone(), cfg.metadata.compiled_rule_set.clone())
        });

        Self {
            rules,
            rule_set,
            version: config::current_config_version(),
        }
    }

    fn refresh_if_changed(&mut self) {
        let current_version = config::current_config_version();
        if current_version == self.version {
            return;
        }

        let (rules, rule_set) = config::with_current_config("reloading highlight rules", |cfg| {
            (cfg.metadata.compiled_rules.clone(), cfg.metadata.compiled_rule_set.clone())
        });
        self.rules = rules;
        self.rule_set = rule_set;
        self.version = current_version;

        log_debug!("Rules updated due to config reload (version {})", self.version);
    }
}

pub(super) fn requires_immediate_terminal_flush(output: &str) -> bool {
    output.as_bytes().iter().any(|byte| matches!(*byte, b'\r' | 0x1b | 0x08))
}

pub(super) fn should_flush_immediately(raw_chunk: &str, processed_chunk: &str) -> bool {
    if requires_immediate_terminal_flush(raw_chunk) {
        return true;
    }

    let highlight_changed_chunk = !(raw_chunk.len() == processed_chunk.len() && raw_chunk.as_ptr() == processed_chunk.as_ptr());
    // Prompt-like chunks are short and commonly have no newline. Flush them
    // immediately when highlighting changed the visible output to keep cursor
    // placement responsive.
    highlight_changed_chunk && raw_chunk.len() <= HIGHLIGHT_FLUSH_HINT_BYTES && !raw_chunk.as_bytes().contains(&b'\n')
}

fn spawn_output_processor(rx: Receiver<OutputChunk>) -> io::Result<thread::JoinHandle<()>> {
    thread::Builder::new().name("output-processor".to_string()).spawn(move || {
        log_debug!("Output processing thread started");
        let mut chunk_id = 0;
        let mut highlight_scratch = highlighter::HighlightScratch::default();
        let mut color_state = highlighter::AnsiColorState::default();
        let mut highlight_rules = HighlightRuleCache::load();
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        let mut flush_state = StdoutFlushState::new();

        loop {
            match rx.recv_timeout(STDOUT_FLUSH_INTERVAL) {
                Ok(chunk) => {
                    highlight_rules.refresh_if_changed();

                    let raw_chunk = chunk.as_str();
                    let processed_chunk = highlighter::process_chunk_with_scratch(
                        raw_chunk,
                        chunk_id,
                        &highlight_rules.rules,
                        highlight_rules.rule_set.as_ref(),
                        RESET_COLOR,
                        &mut color_state,
                        &mut highlight_scratch,
                    );
                    chunk_id += 1;
                    if let Err(err) = stdout.write_all(processed_chunk.as_bytes()) {
                        log_error!("Failed to write processed output to stdout: {}", err);
                        break;
                    }

                    flush_state.record_write(processed_chunk.len());
                    if let Err(err) = flush_state.flush_if_needed(&mut stdout, raw_chunk, &processed_chunk) {
                        log_error!("Failed to flush stdout: {}", err);
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Err(err) = flush_state.flush_on_idle(&mut stdout) {
                        log_error!("Failed to flush stdout on idle timeout: {}", err);
                        break;
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
}

fn send_output_chunk(tx: &SyncSender<OutputChunk>, chunk: String) -> bool {
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
}

/// Stream interactive SSH output, apply highlighting, and return the final exit code.
pub(super) fn run_interactive_ssh(mut child: Child) -> Result<ExitCode> {
    let mut stdout = child.stdout.take().ok_or_else(|| {
        log_error!("Failed to capture stdout from SSH process");
        io::Error::other("Failed to capture stdout")
    })?;

    let (tx, rx): (SyncSender<OutputChunk>, Receiver<OutputChunk>) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
    let processing_thread = spawn_output_processor(rx).map_err(|err| {
        log_error!("Failed to spawn output processing thread: {}", err);
        io::Error::other("Failed to spawn processing thread")
    })?;

    let mut buffer = [0; 8192];
    let mut decoder = Utf8ChunkDecoder::with_capacity(buffer.len());
    let mut total_bytes = 0;

    log_debug!("Starting to read SSH output...");

    loop {
        match stdout.read(&mut buffer) {
            Ok(0) => {
                if let Some(chunk) = decoder.finish() {
                    let _ = send_output_chunk(&tx, chunk);
                }
                log_debug!("EOF reached (total bytes read: {})", total_bytes);
                break;
            }
            Ok(bytes_read) => {
                total_bytes += bytes_read;
                let Some(chunk) = decoder.decode_read(&buffer[..bytes_read]) else {
                    continue;
                };

                if !send_output_chunk(&tx, chunk) {
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

    drop(tx);

    if let Err(err) = processing_thread.join() {
        log_error!("Processing thread panicked: {:?}", err);
    }

    if let Err(err) = io::stdout().flush() {
        log_error!("Failed to flush stdout after processing: {}", err);
    }
    if let Err(err) = log::LOGGER.flush_ssh() {
        log_error!("Failed to flush SSH logs: {}", err);
    }

    let status = child.wait().map_err(|err| {
        log_error!("Failed to wait for SSH process (PID: {:?}): {}", child.id(), err);
        err
    })?;

    let exit_code = status.code().unwrap_or(1);
    log_debug!("Interactive SSH process exited with raw code: {}", exit_code);

    Ok(map_exit_code(status.success(), status.code()))
}
