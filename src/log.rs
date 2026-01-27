//! Logging module for color-ssh
//!
//! Provides structured logging capabilities with different levels:
//! - DEBUG: Detailed diagnostic information
//! - INFO: General informational messages
//! - WARN: Warning messages for potentially problematic situations
//! - ERROR: Error messages for failures
//!
//! Also includes specialized SSH session logging.

mod debug;
mod errors;
mod formatter;
mod macros;
mod ssh;

pub use errors::LogError;

use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicBool, Ordering};

// Global flags for enabling different logging types
static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
static SSH_LOGGING: AtomicBool = AtomicBool::new(false);

// Global logger instance to avoid recreating loggers on every macro call
pub static LOGGER: Lazy<Logger> = Lazy::new(|| Logger::new());

#[derive(Debug, Clone, Copy)]
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
pub struct Logger {
    debug_logger: debug::DebugLogger,
    ssh_logger: ssh::SshLogger,
}

impl Logger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enable_debug(&self) {
        DEBUG_MODE.store(true, Ordering::SeqCst);
    }

    pub fn disable_debug(&self) {
        DEBUG_MODE.store(false, Ordering::SeqCst);
    }

    pub fn enable_ssh_logging(&self) {
        SSH_LOGGING.store(true, Ordering::SeqCst);
    }

    pub fn disable_ssh_logging(&self) {
        SSH_LOGGING.store(false, Ordering::SeqCst);
    }

    pub fn is_debug_enabled(&self) -> bool {
        DEBUG_MODE.load(Ordering::SeqCst)
    }

    pub fn is_ssh_logging_enabled(&self) -> bool {
        SSH_LOGGING.load(Ordering::SeqCst)
    }

    /// Log a debug message (only when debug mode is enabled)
    pub fn log_debug(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Debug, message)?;
        }
        Ok(())
    }

    /// Log an informational message (only when debug mode is enabled)
    pub fn log_info(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Info, message)?;
        }
        Ok(())
    }

    /// Log a warning message (only when debug mode is enabled)
    pub fn log_warn(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Warning, message)?;
        }
        Ok(())
    }

    /// Log an error message (only when debug mode is enabled)
    pub fn log_error(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Error, message)?;
        }
        Ok(())
    }

    /// Log SSH session output
    pub fn log_ssh(&self, message: &str) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.log(message)?;
        }
        Ok(())
    }
}
