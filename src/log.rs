mod debug;
mod errors;
mod formatter;
mod macros;
mod ssh;

pub use errors::LogError;

use once_cell::sync::Lazy;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum DebugVerbosity {
    Off = 0,
    Safe = 1,
    Raw = 2,
}

impl DebugVerbosity {
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

// Global flags for enabling different logging types
static DEBUG_VERBOSITY: AtomicU8 = AtomicU8::new(DebugVerbosity::Off as u8);
static SSH_LOGGING: AtomicBool = AtomicBool::new(false);

// Global logger instance to avoid recreating loggers on every macro call
pub static LOGGER: Lazy<Logger> = Lazy::new(Logger::new);

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl LogLevel {
    // Display helpers.
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
pub struct Logger {
    debug_logger: debug::DebugLogger,
    ssh_logger: ssh::SshLogger,
}

impl Logger {
    // Construction.
    pub fn new() -> Self {
        Self::default()
    }

    // Feature toggles.
    pub fn enable_debug(&self) {
        self.enable_debug_with_verbosity(DebugVerbosity::Safe);
    }

    pub fn enable_debug_with_verbosity(&self, verbosity: DebugVerbosity) {
        DEBUG_VERBOSITY.store(verbosity as u8, Ordering::SeqCst);
    }

    pub fn disable_debug(&self) {
        let was_enabled = self.is_debug_enabled();
        DEBUG_VERBOSITY.store(DebugVerbosity::Off as u8, Ordering::SeqCst);
        if was_enabled {
            let _ = self.debug_logger.flush();
        }
    }

    pub fn enable_ssh_logging(&self) {
        SSH_LOGGING.store(true, Ordering::SeqCst);
    }

    pub fn disable_ssh_logging(&self) {
        SSH_LOGGING.store(false, Ordering::SeqCst);
    }

    // State checks.
    pub fn debug_verbosity(&self) -> DebugVerbosity {
        DebugVerbosity::from_stored(DEBUG_VERBOSITY.load(Ordering::SeqCst))
    }

    pub fn is_debug_enabled(&self) -> bool {
        self.debug_verbosity() >= DebugVerbosity::Safe
    }

    pub fn is_raw_debug_enabled(&self) -> bool {
        self.debug_verbosity() >= DebugVerbosity::Raw
    }

    pub fn is_ssh_logging_enabled(&self) -> bool {
        SSH_LOGGING.load(Ordering::SeqCst)
    }

    // Debug log writing.
    pub fn log_debug(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Debug, message)?;
        }
        Ok(())
    }

    pub fn log_info(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Info, message)?;
        }
        Ok(())
    }

    pub fn log_warn(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Warning, message)?;
        }
        Ok(())
    }

    pub fn log_error(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Error, message)?;
        }
        Ok(())
    }

    pub fn flush_debug(&self) -> Result<(), LogError> {
        self.debug_logger.flush()
    }

    // SSH session log writing.
    pub fn log_ssh(&self, message: &str) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.log(message)?;
        }
        Ok(())
    }

    pub fn log_ssh_raw(&self, message: &str) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.log_raw(message)?;
        }
        Ok(())
    }

    pub fn log_ssh_raw_shared(&self, message: Arc<String>) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.log_raw_shared(message)?;
        }
        Ok(())
    }

    pub fn flush_ssh(&self) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.flush()?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "test/log.rs"]
mod tests;
