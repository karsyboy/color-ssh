//! SSH session logging utilities
//!
//! Provides structured logging for SSH sessions with:
//! - Session output logging
//! - Secret redaction based on patterns
//! - ANSI escape sequence filtering
//! - Per-session log files organized by date

use super::{LogError, formatter::LogFormatter};
use crate::config::SESSION_CONFIG;
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

// Compiled regex for removing ANSI escape sequences
static ANSI_ESCAPE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(\x1B\[[0-9;]*[mK]|\x1B\][0-9];.*?\x07|\x1B\][0-9];.*?\x1B\\)").unwrap()
});

#[derive(Clone)]
pub struct SshLogger {
    formatter: LogFormatter,
}

impl SshLogger {
    pub fn new() -> Self {
        let mut formatter = LogFormatter::new();
        formatter.set_include_timestamp(true);
        formatter.set_include_break(true);

        Self { formatter: formatter }
    }

    fn remove_secrets(&self, message: &str) -> String {
        let secret_patterns = SESSION_CONFIG.read().unwrap().settings.remove_secrets.clone();
        let mut redacted_message = message.to_string();

        if let Some(secret_pattern) = secret_patterns {
            for pattern in secret_pattern {
                if let Ok(regex) = Regex::new(&pattern) {
                    redacted_message = regex.replace_all(&redacted_message, "[REDACTED]").to_string();
                }
            }
        }
        redacted_message
    }

    pub fn log(&self, message: &str) -> Result<(), LogError> {
        let mut buffer = SSH_LOG_BUFFER.lock().unwrap();
        let mut log_file = self.get_or_create_log_file()?;
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
                .filter(|c| c.is_alphanumeric() || c.is_ascii_punctuation() || c.is_whitespace() && *c != '\n' && *c != '\r')
                .collect();

            let message = if SESSION_CONFIG.read().unwrap().settings.remove_secrets.is_some() {
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
                writeln!(log_file, "{}", formatted)?; // Write each line to the log file
            }
        }

        Ok(())
    }

    fn get_or_create_log_file(&self) -> Result<File, LogError> {
        let log_path = self.get_ssh_log_path()?;

        OpenOptions::new().create(true).append(true).open(log_path).map_err(LogError::from)
    }

    fn get_ssh_log_path(&self) -> Result<PathBuf, LogError> {
        let home_dir = dirs::home_dir().ok_or_else(|| LogError::DirectoryCreationError("Home directory not found".to_string()))?;

        let date = Local::now().format("%Y-%m-%d");
        let log_dir = home_dir.join(".csh").join("logs").join("ssh_sessions").join(date.to_string());

        std::fs::create_dir_all(&log_dir)?;

        Ok(log_dir.join(format!("{}.log", SESSION_CONFIG.read().unwrap().metadata.session_name.replace(".", "_"))))
    }
}
