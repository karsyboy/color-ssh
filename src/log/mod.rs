//! Debug and session logging primitives.

mod debug;
mod errors;
mod formatter;
mod macros;
mod ssh;

pub use errors::LogError;

use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
/// Debug logging verbosity levels.
pub enum DebugVerbosity {
    /// Debug logging disabled.
    Off = 0,
    /// Safe debug logging enabled.
    Safe = 1,
    /// Raw debug logging enabled (may include sensitive output).
    Raw = 2,
}

impl DebugVerbosity {
    /// Convert CLI debug flag count (`-d`, `-dd`) to verbosity.
    pub fn from_count(count: u8) -> Self {
        match count {
            0 => Self::Off,
            1 => Self::Safe,
            _ => Self::Raw,
        }
    }

    fn from_stored(value: u8) -> Self {
        match value {
            0 => Self::Off,
            1 => Self::Safe,
            _ => Self::Raw,
        }
    }
}

/// Sanitize session name for use in log filenames.
pub fn sanitize_session_name(raw: &str) -> String {
    let mut sanitized = String::with_capacity(raw.len());
    let mut has_valid = false;

    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            sanitized.push(ch);
            has_valid = true;
        } else {
            sanitized.push('_');
        }
    }

    if !has_valid || sanitized == "." || sanitized == ".." {
        return "session".to_string();
    }

    sanitized
}

// Global flags for enabling different logging types.
static DEBUG_VERBOSITY: AtomicU8 = AtomicU8::new(DebugVerbosity::Off as u8);
static SSH_LOGGING: AtomicBool = AtomicBool::new(false);

// Global logger instance to avoid repeated logger construction.
pub static LOGGER: Lazy<Logger> = Lazy::new(Logger::new);

#[derive(Debug, Clone, Copy)]
/// Internal log level enum for debug logger formatting.
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warning => "WARN",
            LogLevel::Error => "ERROR",
        }
    }
}

#[derive(Clone, Default)]
/// Runtime logger for debug and SSH session output.
pub struct Logger {
    debug_logger: debug::DebugLogger,
    ssh_logger: ssh::SshLogger,
}

#[derive(Clone)]
pub(crate) struct SessionSshLogger {
    ssh_logger: ssh::SshLogger,
}

impl SessionSshLogger {
    pub(crate) fn new(session_name: &str, secret_patterns: Vec<Regex>) -> Self {
        Self {
            ssh_logger: ssh::SshLogger::with_session_name_and_secret_patterns(session_name, secret_patterns),
        }
    }

    pub(crate) fn log_raw_shared(&self, message: Arc<String>) -> Result<(), LogError> {
        self.ssh_logger.log_raw_shared(message)
    }

    pub(crate) fn flush(&self) -> Result<(), LogError> {
        self.ssh_logger.flush()
    }
}

impl Logger {
    /// Construct a logger with both channels disabled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable safe debug logging.
    pub fn enable_debug(&self) {
        self.enable_debug_with_verbosity(DebugVerbosity::Safe);
    }

    /// Enable debug logging with explicit verbosity.
    pub fn enable_debug_with_verbosity(&self, verbosity: DebugVerbosity) {
        DEBUG_VERBOSITY.store(verbosity as u8, Ordering::SeqCst);
    }

    /// Disable debug logging and flush pending messages.
    pub fn disable_debug(&self) {
        let was_enabled = self.is_debug_enabled();
        DEBUG_VERBOSITY.store(DebugVerbosity::Off as u8, Ordering::SeqCst);
        if was_enabled {
            let _ = self.debug_logger.flush();
        }
    }

    /// Enable SSH session logging.
    pub fn enable_ssh_logging(&self) {
        SSH_LOGGING.store(true, Ordering::SeqCst);
    }

    /// Disable SSH session logging.
    pub fn disable_ssh_logging(&self) {
        SSH_LOGGING.store(false, Ordering::SeqCst);
    }

    /// Return currently configured debug verbosity.
    pub fn debug_verbosity(&self) -> DebugVerbosity {
        DebugVerbosity::from_stored(DEBUG_VERBOSITY.load(Ordering::SeqCst))
    }

    /// Returns `true` when safe or raw debug is enabled.
    pub fn is_debug_enabled(&self) -> bool {
        self.debug_verbosity() >= DebugVerbosity::Safe
    }

    /// Returns `true` only when raw debug logging is enabled.
    pub fn is_raw_debug_enabled(&self) -> bool {
        self.debug_verbosity() >= DebugVerbosity::Raw
    }

    /// Returns `true` when SSH session logging is enabled.
    pub fn is_ssh_logging_enabled(&self) -> bool {
        SSH_LOGGING.load(Ordering::SeqCst)
    }

    /// Write a debug-level message if debug logging is enabled.
    pub fn log_debug(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Debug, message)?;
        }
        Ok(())
    }

    /// Write an info-level message if debug logging is enabled.
    pub fn log_info(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Info, message)?;
        }
        Ok(())
    }

    /// Write a warning-level message if debug logging is enabled.
    pub fn log_warn(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Warning, message)?;
        }
        Ok(())
    }

    /// Write an error-level message if debug logging is enabled.
    pub fn log_error(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Error, message)?;
        }
        Ok(())
    }

    /// Flush debug logger output.
    pub fn flush_debug(&self) -> Result<(), LogError> {
        self.debug_logger.flush()
    }

    /// Write one sanitized SSH session log line when enabled.
    pub fn log_ssh(&self, message: &str) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.log(message)?;
        }
        Ok(())
    }

    /// Write one raw SSH chunk when enabled.
    pub fn log_ssh_raw(&self, message: &str) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.log_raw(message)?;
        }
        Ok(())
    }

    /// Write one shared raw SSH chunk without cloning the string.
    pub fn log_ssh_raw_shared(&self, message: Arc<String>) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.log_raw_shared(message)?;
        }
        Ok(())
    }

    /// Flush SSH session log output.
    pub fn flush_ssh(&self) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../test/log/mod.rs"]
mod tests;
