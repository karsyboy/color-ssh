//! Transitional captured-output runtime for interactive child processes.
//!
//! This path remains only for interactive sessions that cannot yet use the
//! PTY-centered renderer stack. It forwards captured child output to the local
//! terminal without rewriting stdout bytes.
//!
//! Current callers:
//! - direct SSH launches without an interactive controlling TTY
//! - direct RDP launches that still need captured stdout/stderr forwarding

use super::exit::map_exit_code;
use crate::{Result, log, log_debug, log_error};
use std::io::{self, Read, Write};
use std::process::{Child, ExitCode};
use std::sync::{
    Arc,
    mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
};
use std::thread;
use std::time::{Duration, Instant};

const OUTPUT_FLUSH_BYTES: usize = 32 * 1024;
const OUTPUT_FLUSH_INTERVAL: Duration = Duration::from_millis(25);
const OUTPUT_QUEUE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputTarget {
    Stdout,
    Stderr,
}

enum OutputPayload {
    Owned(String),
    Shared(Arc<String>),
}

impl OutputPayload {
    fn as_str(&self) -> &str {
        match self {
            Self::Owned(chunk) => chunk.as_str(),
            Self::Shared(chunk) => chunk.as_str(),
        }
    }
}

struct OutputChunk {
    target: OutputTarget,
    payload: OutputPayload,
}

impl OutputChunk {
    fn as_str(&self) -> &str {
        self.payload.as_str()
    }
}

#[derive(Debug)]
struct OutputFlushState {
    pending_bytes: usize,
    last_flush_at: Instant,
}

impl OutputFlushState {
    fn new() -> Self {
        Self {
            pending_bytes: 0,
            last_flush_at: Instant::now(),
        }
    }

    fn record_write(&mut self, bytes_written: usize) {
        self.pending_bytes = self.pending_bytes.saturating_add(bytes_written);
    }

    fn flush_if_needed(&mut self, output: &mut impl Write, chunk: &str) -> io::Result<()> {
        if should_flush_immediately(chunk) || self.pending_bytes >= OUTPUT_FLUSH_BYTES || self.last_flush_at.elapsed() >= OUTPUT_FLUSH_INTERVAL {
            output.flush()?;
            self.pending_bytes = 0;
            self.last_flush_at = Instant::now();
        }
        Ok(())
    }

    fn flush_on_idle(&mut self, output: &mut impl Write) -> io::Result<()> {
        if self.pending_bytes > 0 && self.last_flush_at.elapsed() >= OUTPUT_FLUSH_INTERVAL {
            output.flush()?;
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

pub(super) fn requires_immediate_terminal_flush(output: &str) -> bool {
    output.as_bytes().iter().any(|byte| matches!(*byte, b'\r' | 0x1b | 0x08))
}

pub(super) fn should_flush_immediately(output: &str) -> bool {
    requires_immediate_terminal_flush(output)
}

fn spawn_output_forwarder(rx: Receiver<OutputChunk>) -> io::Result<thread::JoinHandle<()>> {
    thread::Builder::new().name("output-forwarder".to_string()).spawn(move || {
        log_debug!("Interactive passthrough output thread started");
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        let mut stdout_flush = OutputFlushState::new();
        let mut stderr_flush = OutputFlushState::new();

        loop {
            match rx.recv_timeout(OUTPUT_FLUSH_INTERVAL) {
                Ok(chunk) => {
                    let output = chunk.as_str();
                    let write_result = match chunk.target {
                        OutputTarget::Stdout => match stdout.write_all(output.as_bytes()) {
                            Ok(()) => {
                                stdout_flush.record_write(output.len());
                                stdout_flush.flush_if_needed(&mut stdout, output)
                            }
                            Err(err) => Err(err),
                        },
                        OutputTarget::Stderr => {
                            let stderr = io::stderr();
                            let mut stderr = stderr.lock();
                            match stderr.write_all(output.as_bytes()) {
                                Ok(()) => {
                                    stderr_flush.record_write(output.len());
                                    stderr_flush.flush_if_needed(&mut stderr, output)
                                }
                                Err(err) => Err(err),
                            }
                        }
                    };

                    if let Err(err) = write_result {
                        log_error!("Failed to write interactive passthrough output: {}", err);
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Err(err) = stdout_flush.flush_on_idle(&mut stdout) {
                        log_error!("Failed to flush stdout on idle timeout: {}", err);
                        break;
                    }
                    if stderr_flush.pending_bytes > 0 {
                        let stderr = io::stderr();
                        let mut stderr = stderr.lock();
                        if let Err(err) = stderr_flush.flush_on_idle(&mut stderr) {
                            log_error!("Failed to flush stderr on idle timeout: {}", err);
                            break;
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }

        if let Err(err) = stdout.flush() {
            log_error!("Failed to flush stdout at thread end: {}", err);
        }
        if stderr_flush.pending_bytes > 0 {
            let stderr = io::stderr();
            let mut stderr = stderr.lock();
            if let Err(err) = stderr.flush() {
                log_error!("Failed to flush stderr at thread end: {}", err);
            }
        }
        log_debug!("Interactive passthrough output thread finished");
    })
}

fn send_output_chunk(tx: &SyncSender<OutputChunk>, target: OutputTarget, chunk: String) -> bool {
    if chunk.is_empty() {
        return true;
    }

    let output_chunk = if log::LOGGER.is_ssh_logging_enabled() {
        let shared_chunk = Arc::new(chunk);
        if let Err(err) = log::LOGGER.log_ssh_raw_shared(shared_chunk.clone()) {
            log_error!("Failed to write session log data: {}", err);
        }
        OutputChunk {
            target,
            payload: OutputPayload::Shared(shared_chunk),
        }
    } else {
        OutputChunk {
            target,
            payload: OutputPayload::Owned(chunk),
        }
    };

    if let Err(err) = tx.send(output_chunk) {
        log_error!("Failed to send data to processing thread: {}", err);
        return false;
    }

    true
}

fn spawn_stream_reader<R>(name: &str, mut reader: R, target: OutputTarget, tx: SyncSender<OutputChunk>) -> io::Result<thread::JoinHandle<()>>
where
    R: Read + Send + 'static,
{
    let thread_name = format!("process-{}", name);
    thread::Builder::new().name(thread_name).spawn(move || {
        let mut buffer = [0; 8192];
        let mut decoder = Utf8ChunkDecoder::with_capacity(buffer.len());

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    if let Some(chunk) = decoder.finish() {
                        let _ = send_output_chunk(&tx, target, chunk);
                    }
                    break;
                }
                Ok(bytes_read) => {
                    let Some(chunk) = decoder.decode_read(&buffer[..bytes_read]) else {
                        continue;
                    };

                    if !send_output_chunk(&tx, target, chunk) {
                        break;
                    }
                }
                Err(err) => {
                    log_error!("Error reading {:?} stream: {}", target, err);
                    break;
                }
            }
        }
    })
}

fn run_interactive_passthrough(mut child: Child, capture_stderr: bool) -> Result<ExitCode> {
    let stdout = child.stdout.take().ok_or_else(|| {
        log_error!("Failed to capture child stdout");
        io::Error::other("Failed to capture stdout")
    })?;
    let stderr = if capture_stderr { child.stderr.take() } else { None };

    let (tx, rx): (SyncSender<OutputChunk>, Receiver<OutputChunk>) = mpsc::sync_channel(OUTPUT_QUEUE_CAPACITY);
    let processing_thread = spawn_output_forwarder(rx).map_err(|err| {
        log_error!("Failed to spawn output processing thread: {}", err);
        io::Error::other("Failed to spawn processing thread")
    })?;

    let mut reader_threads = Vec::new();
    reader_threads.push(spawn_stream_reader("stdout", stdout, OutputTarget::Stdout, tx.clone()).map_err(|err| {
        log_error!("Failed to spawn stdout reader: {}", err);
        io::Error::other("Failed to spawn stdout reader")
    })?);

    if let Some(stderr) = stderr {
        reader_threads.push(spawn_stream_reader("stderr", stderr, OutputTarget::Stderr, tx.clone()).map_err(|err| {
            log_error!("Failed to spawn stderr reader: {}", err);
            io::Error::other("Failed to spawn stderr reader")
        })?);
    }

    drop(tx);

    for thread in reader_threads {
        if let Err(err) = thread.join() {
            log_error!("Reader thread panicked: {:?}", err);
        }
    }

    if let Err(err) = processing_thread.join() {
        log_error!("Processing thread panicked: {:?}", err);
    }

    if let Err(err) = io::stdout().flush() {
        log_error!("Failed to flush stdout after processing: {}", err);
    }
    if let Err(err) = io::stderr().flush() {
        log_error!("Failed to flush stderr after processing: {}", err);
    }
    if let Err(err) = log::LOGGER.flush_ssh() {
        log_error!("Failed to flush session logs: {}", err);
    }

    let status = child.wait().map_err(|err| {
        log_error!("Failed to wait for process (PID: {:?}): {}", child.id(), err);
        err
    })?;

    Ok(map_exit_code(status.success(), status.code()))
}

/// Forward interactive SSH output without mutating stdout content.
pub(super) fn run_interactive_ssh(child: Child) -> Result<ExitCode> {
    run_interactive_passthrough(child, false)
}

/// Forward interactive RDP client output without shell highlighting.
pub(super) fn run_interactive_rdp(child: Child) -> Result<ExitCode> {
    run_interactive_passthrough(child, true)
}
