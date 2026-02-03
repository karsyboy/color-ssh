//! Debug logging implementation
//!
//! Provides file-based logging for debug, info, warn, and error messages.
//! Logs are written to `~/.color-ssh/logs/cossh.log` with timestamps and log levels.

use super::{LogError, LogLevel, formatter::LogFormatter};
use once_cell::sync::Lazy;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

/// Global debug log file handle
static DEBUG_LOG_FILE: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));

/// Debug logger that writes formatted log messages to a file
#[derive(Clone)]
pub struct DebugLogger {
    /// Formatter for log messages (includes timestamp and level)
    formatter: LogFormatter,
}

impl Default for DebugLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugLogger {
    // Create a new debug logger
    pub fn new() -> Self {
        let mut formatter = LogFormatter::new();
        formatter.set_include_timestamp(true);
        formatter.set_include_level(true);

        Self { formatter }
    }

    // Write a log message to the debug log file
    pub fn log(&self, level: LogLevel, message: &str) -> Result<(), LogError> {
        let formatted = self.formatter.format(Some(level), message);
        let mut file_guard = DEBUG_LOG_FILE.lock().unwrap();

        // Lazy initialization: create log file on first use
        if file_guard.is_none() {
            *file_guard = Some(self.create_log_file()?);
        }

        // Write and flush to ensure the log is persisted immediately
        if let Some(file) = file_guard.as_mut() {
            writeln!(file, "{}", formatted)?;
            file.flush()?; // Flush immediately so logs aren't lost on crash
        }

        Ok(())
    }

    // Create or open the debug log file for appending
    fn create_log_file(&self) -> Result<File, LogError> {
        let log_path = self.get_debug_log_path()?;

        OpenOptions::new()
            .create(true) // Create if doesn't exist
            .append(true) // Append to preserve existing logs
            .open(log_path)
            .map_err(LogError::from)
    }

    // Get the path to the debug log file
    fn get_debug_log_path(&self) -> Result<PathBuf, LogError> {
        let home_dir = dirs::home_dir().ok_or_else(|| LogError::DirectoryCreationError("Home directory not found".to_string()))?;

        let log_dir = home_dir.join(".color-ssh").join("logs");

        // Create directory structure if it doesn't exist
        std::fs::create_dir_all(&log_dir)?;

        Ok(log_dir.join("cossh.log"))
    }
}
