//! Debug logging implementation
//!
//! Provides file-based logging for debug, info, warn, and error messages.
//! Logs are written to `~/.color-ssh/logs/cossh.log` with timestamps and log levels.

use super::{LogError, LogLevel, formatter::LogFormatter};
use once_cell::sync::Lazy;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

const DEBUG_LOG_FLUSH_BYTES: usize = 16 * 1024;
const DEBUG_LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(100);

struct DebugLogState {
    writer: Option<BufWriter<File>>,
    pending_bytes: usize,
    last_flush: Instant,
}

impl DebugLogState {
    // State construction.
    fn new() -> Self {
        Self {
            writer: None,
            pending_bytes: 0,
            last_flush: Instant::now(),
        }
    }
}

/// Global debug log writer state shared across all logger instances.
static DEBUG_LOG_STATE: Lazy<Mutex<DebugLogState>> = Lazy::new(|| Mutex::new(DebugLogState::new()));

/// Debug logger that writes formatted log messages to a file
#[derive(Clone)]
pub(super) struct DebugLogger {
    /// Formatter for log messages (includes timestamp and level)
    formatter: LogFormatter,
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

        Self { formatter }
    }

    // Log writing.
    pub(super) fn log(&self, level: LogLevel, message: &str) -> Result<(), LogError> {
        let formatted = self.formatter.format(Some(level), message);
        let mut state = DEBUG_LOG_STATE.lock().unwrap();

        // Lazy initialization: create log file on first use
        if state.writer.is_none() {
            state.writer = Some(BufWriter::new(self.create_log_file()?));
        }

        if let Some(writer) = state.writer.as_mut() {
            writer.write_all(formatted.as_bytes())?;
            writer.write_all(b"\n")?;
            state.pending_bytes = state.pending_bytes.saturating_add(formatted.len() + 1);
        }

        if should_flush(state.pending_bytes, state.last_flush.elapsed()) {
            if let Some(writer) = state.writer.as_mut() {
                writer.flush()?;
            }
            state.pending_bytes = 0;
            state.last_flush = Instant::now();
        }

        Ok(())
    }

    // Force-flush buffered log output.
    pub(super) fn flush(&self) -> Result<(), LogError> {
        let mut state = DEBUG_LOG_STATE.lock().unwrap();
        if let Some(writer) = state.writer.as_mut() {
            writer.flush()?;
            state.pending_bytes = 0;
            state.last_flush = Instant::now();
        }
        Ok(())
    }

    // File path and file creation helpers.
    fn create_log_file(&self) -> Result<File, LogError> {
        let log_path = self.get_debug_log_path()?;

        OpenOptions::new()
            .create(true) // Create if doesn't exist
            .append(true) // Append to preserve existing logs
            .open(log_path)
            .map_err(LogError::from)
    }

    fn get_debug_log_path(&self) -> Result<PathBuf, LogError> {
        let home_dir = dirs::home_dir().ok_or_else(|| LogError::DirectoryCreationError("Home directory not found".to_string()))?;

        let log_dir = home_dir.join(".color-ssh").join("logs");

        // Create directory structure if it doesn't exist
        std::fs::create_dir_all(&log_dir)?;

        Ok(log_dir.join("cossh.log"))
    }
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
