//! Debug logging implementation
//!
//! Provides file-based logging for debug, info, warn, and error messages.
//! Logs are written to `~/.csh/logs/csh.log` with timestamps and log levels.

use super::{LogError, LogLevel, formatter::LogFormatter};
use once_cell::sync::Lazy;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

/// Global debug log file handle
///
/// Uses lazy initialization to create the file only when first accessed.
/// The Mutex ensures thread-safe access to the file handle.
static DEBUG_LOG_FILE: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));

/// Debug logger that writes formatted log messages to a file
#[derive(Clone)]
pub struct DebugLogger {
    /// Formatter for log messages (includes timestamp and level)
    formatter: LogFormatter,
}

impl DebugLogger {
    /// Create a new debug logger
    ///
    /// Initializes with a formatter configured to include:
    /// - Timestamps (for tracking when events occur)
    /// - Log levels (for filtering and prioritization)
    pub fn new() -> Self {
        let mut formatter = LogFormatter::new();
        formatter.set_include_timestamp(true);
        formatter.set_include_level(true);

        Self { formatter }
    }

    /// Write a log message to the debug log file
    ///
    /// # Arguments
    /// * `level` - The severity level (DEBUG, INFO, WARN, ERROR)
    /// * `message` - The message to log
    ///
    /// # Behavior
    /// - Lazily creates the log file on first use
    /// - Formats the message with timestamp and level
    /// - Flushes immediately to ensure messages are written (important for crash scenarios)
    ///
    /// # Returns
    /// Returns `Ok(())` on success, or a `LogError` if file operations fail
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

    /// Create or open the debug log file for appending
    ///
    /// Opens the file in append mode to preserve existing logs across runs.
    /// Creates the file if it doesn't exist.
    fn create_log_file(&self) -> Result<File, LogError> {
        let log_path = self.get_debug_log_path()?;

        OpenOptions::new()
            .create(true) // Create if doesn't exist
            .append(true) // Append to preserve existing logs
            .open(log_path)
            .map_err(LogError::from)
    }

    /// Get the path to the debug log file
    ///
    /// # Path Structure
    /// `~/.csh/logs/csh.log`
    ///
    /// # Behavior
    /// - Creates the directory structure if it doesn't exist
    /// - Returns an error if home directory cannot be determined
    ///
    /// # Returns
    /// The full path to the debug log file, or a `LogError` on failure
    fn get_debug_log_path(&self) -> Result<PathBuf, LogError> {
        let home_dir = dirs::home_dir().ok_or_else(|| LogError::DirectoryCreationError("Home directory not found".to_string()))?;

        let log_dir = home_dir.join(".csh").join("logs");

        // Create directory structure if it doesn't exist
        std::fs::create_dir_all(&log_dir)?;

        Ok(log_dir.join("csh.log"))
    }
}
