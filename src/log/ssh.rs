//! SSH session logging utilities
//!
//! Provides structured logging for SSH sessions with:
//! - Session output logging
//! - Secret redaction based on patterns
//! - ANSI escape sequence filtering
//! - Per-session log files organized by date

use super::{LogError, formatter::LogFormatter};
use chrono::Local;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

// A global buffer to accumulate output until full lines are available.
static SSH_LOG_BUFFER: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

// Global SSH log file handle
static SSH_LOG_FILE: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));

// Compiled regex for removing ANSI escape sequences
static ANSI_ESCAPE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        \x1B\[[\x30-\x3F]*[\x20-\x2F]*[\x40-\x7E]    # CSI: ESC [ params intermediates final
        |\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)           # OSC: ESC ] ... (BEL or ESC \)
        |\x1B[PX^_].*?\x1B\\                         # DCS/SOS/PM/APC: ESC P/X/^/_ ... ESC \
        |\x1B.                                       # Other ESC sequences (2 bytes)
        |\x1B                                        # Stray ESC character
    ",
    )
    .unwrap()
});

#[derive(Clone)]
pub struct SshLogger {
    formatter: LogFormatter,
}

impl Default for SshLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl SshLogger {
    pub fn new() -> Self {
        let mut formatter = LogFormatter::new();
        formatter.set_include_timestamp(true);
        formatter.set_include_break(true);

        Self { formatter }
    }

    fn remove_secrets(&self, message: &str) -> String {
        let compiled_patterns = crate::config::get_config().read().unwrap().metadata.compiled_secret_patterns.clone();
        let mut redacted_message = message.to_string();

        for regex in &compiled_patterns {
            redacted_message = regex.replace_all(&redacted_message, "[REDACTED]").to_string();
        }

        redacted_message
    }

    pub fn log(&self, message: &str) -> Result<(), LogError> {
        let mut buffer = SSH_LOG_BUFFER.lock().unwrap();
        buffer.push_str(message);

        while let Some(newline_pos) = buffer.find('\n') {
            // Extract one complete line (without the newline).
            let message = buffer[..newline_pos].trim_end().to_string();

            // Remove the processed line (and the newline) from the buffer.
            *buffer = buffer[newline_pos + 1..].to_string();

            // Filter out special ASCII characters
            let cleaned_message = ANSI_ESCAPE_REGEX.replace_all(&message, "").to_string();
            let message: String = cleaned_message
                .chars()
                .filter(|ch| (ch.is_alphanumeric() || ch.is_ascii_punctuation() || ch.is_whitespace()) && *ch != '\n' && *ch != '\r')
                .collect();

            let message = if !crate::config::get_config().read().unwrap().metadata.compiled_secret_patterns.is_empty() {
                self.remove_secrets(&message)
            } else {
                message
            };

            // format one line at a time with date and time and write to the log file
            for msg in message.lines() {
                if msg.is_empty() {
                    continue; // Skip empty lines
                }
                let formatted = self.formatter.format(None, msg);

                // Get or create log file with caching
                let mut file_guard = SSH_LOG_FILE.lock().unwrap();
                if file_guard.is_none() {
                    *file_guard = Some(self.create_log_file()?);
                }

                if let Some(file) = file_guard.as_mut() {
                    writeln!(file, "{}", formatted)?;
                    file.flush()?; // Ensure logs are written immediately
                }
            }
        }

        Ok(())
    }

    fn create_log_file(&self) -> Result<File, LogError> {
        let log_path = self.get_ssh_log_path()?;

        OpenOptions::new().create(true).append(true).open(log_path).map_err(LogError::from)
    }

    fn get_ssh_log_path(&self) -> Result<PathBuf, LogError> {
        let home_dir = dirs::home_dir().ok_or_else(|| LogError::DirectoryCreationError("Home directory not found".to_string()))?;

        let date = Local::now().format("%Y-%m-%d");
        let log_dir = home_dir.join(".color-ssh").join("logs").join("ssh_sessions").join(date.to_string());

        std::fs::create_dir_all(&log_dir)?;

        Ok(log_dir.join(format!(
            "{}.log",
            crate::config::get_config().read().unwrap().metadata.session_name.replace(".", "_")
        )))
    }
}
