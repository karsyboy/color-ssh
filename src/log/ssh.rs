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
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

const SSH_LOG_FLUSH_BYTES: usize = 64 * 1024;
const SSH_LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(100);

struct SshLogState {
    line_buffer: String,
    writer: Option<BufWriter<File>>,
    pending_bytes: usize,
    last_flush: Instant,
}

impl SshLogState {
    fn new() -> Self {
        Self {
            line_buffer: String::new(),
            writer: None,
            pending_bytes: 0,
            last_flush: Instant::now(),
        }
    }
}

// Global SSH logging state shared across all sessions.
static SSH_LOG_STATE: Lazy<Mutex<SshLogState>> = Lazy::new(|| Mutex::new(SshLogState::new()));

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

    pub fn log(&self, message: &str) -> Result<(), LogError> {
        self.log_raw(message)
    }

    pub fn log_raw(&self, message: &str) -> Result<(), LogError> {
        let secret_patterns = current_secret_patterns();
        let mut state = SSH_LOG_STATE.lock().unwrap();
        state.line_buffer.push_str(message);
        let lines = extract_complete_lines(&mut state.line_buffer);

        if lines.is_empty() {
            return Ok(());
        }

        if state.writer.is_none() {
            state.writer = Some(BufWriter::new(self.create_log_file()?));
        }

        for line in lines {
            if line.is_empty() {
                continue;
            }

            let sanitized = sanitize_line(&line, &secret_patterns);
            if sanitized.is_empty() {
                continue;
            }

            let formatted = self.formatter.format(None, &sanitized);
            if let Some(writer) = state.writer.as_mut() {
                writer.write_all(formatted.as_bytes())?;
                writer.write_all(b"\n")?;
            }
            state.pending_bytes += formatted.len() + 1;
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

    pub fn flush(&self) -> Result<(), LogError> {
        let mut state = SSH_LOG_STATE.lock().unwrap();
        if let Some(writer) = state.writer.as_mut() {
            writer.flush()?;
            state.pending_bytes = 0;
            state.last_flush = Instant::now();
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

fn current_secret_patterns() -> Vec<Regex> {
    crate::config::SESSION_CONFIG
        .get()
        .and_then(|config| config.read().ok().map(|config_guard| config_guard.metadata.compiled_secret_patterns.clone()))
        .unwrap_or_default()
}

fn sanitize_line(line: &str, secret_patterns: &[Regex]) -> String {
    let cleaned = ANSI_ESCAPE_REGEX.replace_all(line, "");
    let mut sanitized: String = cleaned
        .chars()
        .filter(|ch| (ch.is_alphanumeric() || ch.is_ascii_punctuation() || ch.is_whitespace()) && *ch != '\n' && *ch != '\r')
        .collect();

    for regex in secret_patterns {
        if regex.is_match(&sanitized) {
            sanitized = regex.replace_all(&sanitized, "[REDACTED]").into_owned();
        }
    }

    sanitized
}

fn extract_complete_lines(buffer: &mut String) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0usize;

    while let Some(relative_newline) = buffer[start..].find('\n') {
        let end = start + relative_newline;
        lines.push(buffer[start..end].trim_end_matches('\r').to_string());
        start = end + 1;
    }

    if start > 0 {
        buffer.drain(..start);
    }

    lines
}

fn should_flush(pending_bytes: usize, elapsed_since_flush: Duration) -> bool {
    pending_bytes >= SSH_LOG_FLUSH_BYTES || elapsed_since_flush >= SSH_LOG_FLUSH_INTERVAL
}

#[cfg(test)]
mod tests {
    use super::{extract_complete_lines, sanitize_line, should_flush};
    use regex::Regex;
    use std::time::Duration;

    #[test]
    fn extract_complete_lines_keeps_partial_tail() {
        let mut buffer = "one\ntwo\npartial".to_string();
        let lines = extract_complete_lines(&mut buffer);

        assert_eq!(lines, vec!["one".to_string(), "two".to_string()]);
        assert_eq!(buffer, "partial");
    }

    #[test]
    fn sanitize_line_strips_ansi_and_redacts_patterns() {
        let secrets = vec![Regex::new("token=\\w+").expect("regex compiles")];
        let line = "\x1b[31mtoken=abc123\x1b[0m ok";
        let sanitized = sanitize_line(line, &secrets);
        assert_eq!(sanitized, "[REDACTED] ok");
    }

    #[test]
    fn should_flush_on_size_or_interval() {
        assert!(!should_flush(1024, Duration::from_millis(20)));
        assert!(should_flush(64 * 1024, Duration::from_millis(20)));
        assert!(should_flush(1, Duration::from_millis(100)));
    }
}
