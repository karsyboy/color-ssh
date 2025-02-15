/*
TODO:
    - Figure out how to remove weir ASCII characters from the log file
*/

use chrono::Local;
use once_cell::sync::Lazy;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::config::CONFIG;
use crate::logging::{LogError, LogFormatter};

// A global buffer to accumulate output until full lines are available.
static SSH_LOG_BUFFER: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

#[derive(Clone)]
pub struct SshLogger {
    formatter: LogFormatter,
}

impl SshLogger {
    pub fn new() -> Self {
        Self {
            formatter: LogFormatter::new(true, false),
        }
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
            // let message: String = message.chars().filter(|c| c.is_alphanumeric() || c.is_whitespace() || c.is_ascii_punctuation()).collect();

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

        OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
            .map_err(LogError::from)
    }

    fn get_ssh_log_path(&self) -> Result<PathBuf, LogError> {
        let home_dir = dirs::home_dir().ok_or_else(|| {
            LogError::DirectoryCreationError("Home directory not found".to_string())
        })?;

        let date = Local::now().format("%Y-%m-%d");
        let log_dir = home_dir
            .join(".csh")
            .join("logs")
            .join("ssh_sessions")
            .join(date.to_string());

        std::fs::create_dir_all(&log_dir)?;

        Ok(log_dir.join(format!(
            "{}.log",
            CONFIG
                .read()
                .unwrap()
                .metadata
                .session_name
                .replace(".", "_")
        )))
    }
}
