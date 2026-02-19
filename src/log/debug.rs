//! Debug logging implementation
//!
//! Provides file-based logging for debug, info, warn, and error messages.
//! Logs are written to `~/.color-ssh/logs/cossh.log` with timestamps and log levels.

use super::{LogError, LogLevel, formatter::LogFormatter};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

const DEBUG_LOG_FLUSH_BYTES: usize = 16 * 1024;
const DEBUG_LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(100);
const DEBUG_LOG_QUEUE_CAPACITY: usize = 2048;

enum DebugLogCommand {
    Entry(LogLevel, String),
    Flush(SyncSender<Result<(), String>>),
}

struct DebugLogWorkerState {
    writer: Option<BufWriter<File>>,
    pending_bytes: usize,
    last_flush: Instant,
    last_error: Option<String>,
}

impl DebugLogWorkerState {
    fn new() -> Self {
        Self {
            writer: None,
            pending_bytes: 0,
            last_flush: Instant::now(),
            last_error: None,
        }
    }
}

/// Debug logger that writes formatted log messages to a file
#[derive(Clone)]
pub(super) struct DebugLogger {
    /// Formatter for log messages (includes timestamp and level)
    formatter: LogFormatter,
    worker_tx: Arc<Mutex<Option<SyncSender<DebugLogCommand>>>>,
}

impl Default for DebugLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugLogger {
    // Construction.
    pub(super) fn new() -> Self {
        let mut formatter = LogFormatter::new();
        formatter.set_include_timestamp(true);
        formatter.set_include_level(true);

        Self {
            formatter,
            worker_tx: Arc::new(Mutex::new(None)),
        }
    }

    // Log writing.
    pub(super) fn log(&self, level: LogLevel, message: &str) -> Result<(), LogError> {
        let tx = self.ensure_worker()?;
        tx.send(DebugLogCommand::Entry(level, message.to_string()))
            .map_err(|err| LogError::FormattingError(format!("failed to enqueue debug log entry: {}", err)))
    }

    // Force-flush buffered log output.
    pub(super) fn flush(&self) -> Result<(), LogError> {
        let tx = {
            let worker_tx_guard = match self.worker_tx.lock() {
                Ok(worker_tx_guard) => worker_tx_guard,
                Err(poisoned) => {
                    eprintln!("Debug log worker lock poisoned during flush; continuing with recovered state");
                    poisoned.into_inner()
                }
            };
            worker_tx_guard.as_ref().cloned()
        };

        let Some(tx) = tx else {
            return Ok(());
        };

        let (ack_tx, ack_rx) = mpsc::sync_channel(0);
        tx.send(DebugLogCommand::Flush(ack_tx))
            .map_err(|err| LogError::FormattingError(format!("failed to enqueue debug log flush: {}", err)))?;

        match ack_rx.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err_msg)) => Err(LogError::FormattingError(err_msg)),
            Err(err) => Err(LogError::FormattingError(format!("failed waiting for debug log flush ack: {}", err))),
        }
    }

    fn ensure_worker(&self) -> Result<SyncSender<DebugLogCommand>, LogError> {
        let mut worker_tx_guard = match self.worker_tx.lock() {
            Ok(worker_tx_guard) => worker_tx_guard,
            Err(poisoned) => {
                eprintln!("Debug log worker lock poisoned; continuing with recovered state");
                poisoned.into_inner()
            }
        };
        if let Some(existing_tx) = worker_tx_guard.as_ref() {
            return Ok(existing_tx.clone());
        }

        let (tx, rx) = mpsc::sync_channel(DEBUG_LOG_QUEUE_CAPACITY);
        let formatter = self.formatter.clone();

        thread::Builder::new()
            .name("debug-log-writer".to_string())
            .spawn(move || run_worker(rx, formatter))
            .map_err(|err| LogError::FormattingError(format!("failed to spawn debug log worker: {}", err)))?;

        *worker_tx_guard = Some(tx.clone());
        Ok(tx)
    }

    // File path and file creation helpers.
    fn create_log_file() -> Result<File, LogError> {
        let log_path = Self::get_debug_log_path()?;

        OpenOptions::new()
            .create(true) // Create if doesn't exist
            .append(true) // Append to preserve existing logs
            .open(log_path)
            .map_err(LogError::from)
    }

    fn get_debug_log_path() -> Result<PathBuf, LogError> {
        let home_dir = dirs::home_dir().ok_or_else(|| LogError::DirectoryCreationError("Home directory not found".to_string()))?;

        let log_dir = home_dir.join(".color-ssh").join("logs");

        // Create directory structure if it doesn't exist
        std::fs::create_dir_all(&log_dir)?;

        Ok(log_dir.join("cossh.log"))
    }
}

fn run_worker(receiver: Receiver<DebugLogCommand>, formatter: LogFormatter) {
    let mut state = DebugLogWorkerState::new();

    loop {
        match receiver.recv_timeout(DEBUG_LOG_FLUSH_INTERVAL) {
            Ok(DebugLogCommand::Entry(level, message)) => {
                if let Err(err) = process_log_entry(&mut state, &formatter, level, &message) {
                    state.last_error = Some(err.to_string());
                }
            }
            Ok(DebugLogCommand::Flush(ack_tx)) => {
                let flush_result = flush_worker(&mut state).map_err(|err| err.to_string());
                let _ = ack_tx.send(flush_result);
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Err(err) = flush_if_due(&mut state) {
                    state.last_error = Some(err.to_string());
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = flush_worker(&mut state);
                break;
            }
        }
    }
}

fn process_log_entry(state: &mut DebugLogWorkerState, formatter: &LogFormatter, level: LogLevel, message: &str) -> Result<(), LogError> {
    if state.writer.is_none() {
        state.writer = Some(BufWriter::new(DebugLogger::create_log_file()?));
    }

    let formatted = formatter.format(Some(level), message);
    if let Some(writer) = state.writer.as_mut() {
        writer.write_all(formatted.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    state.pending_bytes = state.pending_bytes.saturating_add(formatted.len() + 1);

    flush_if_due(state)
}

fn flush_if_due(state: &mut DebugLogWorkerState) -> Result<(), LogError> {
    if should_flush(state.pending_bytes, state.last_flush.elapsed()) {
        flush_writer(state)?;
    }
    Ok(())
}

fn flush_writer(state: &mut DebugLogWorkerState) -> Result<(), LogError> {
    if let Some(writer) = state.writer.as_mut() {
        writer.flush()?;
        state.pending_bytes = 0;
        state.last_flush = Instant::now();
    }
    Ok(())
}

fn flush_worker(state: &mut DebugLogWorkerState) -> Result<(), LogError> {
    flush_writer(state)?;
    if let Some(last_error) = state.last_error.take() {
        return Err(LogError::FormattingError(last_error));
    }
    Ok(())
}

fn should_flush(pending_bytes: usize, elapsed_since_flush: Duration) -> bool {
    pending_bytes >= DEBUG_LOG_FLUSH_BYTES || elapsed_since_flush >= DEBUG_LOG_FLUSH_INTERVAL
}

#[cfg(test)]
mod tests {
    use super::should_flush;
    use std::time::Duration;

    #[test]
    fn should_flush_on_size_or_interval() {
        assert!(!should_flush(512, Duration::from_millis(20)));
        assert!(should_flush(16 * 1024, Duration::from_millis(20)));
        assert!(should_flush(1, Duration::from_millis(100)));
    }
}
