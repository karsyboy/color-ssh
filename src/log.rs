/*
TODO:
    - Create log_info, log_warn, log_error methods for different log levels
*/

mod debug;
mod errors;
mod formatter;
mod macros;
mod ssh;

pub use errors::LogError;

use std::sync::atomic::{AtomicBool, Ordering};

// Global flags for enabling different logging types
static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
static SSH_LOGGING: AtomicBool = AtomicBool::new(false);

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

#[derive(Clone)]
pub struct Logger {
    debug_logger: debug::DebugLogger,
    ssh_logger: ssh::SshLogger,
}

impl Logger {
    pub fn new() -> Self {
        Self {
            debug_logger: debug::DebugLogger::new(),
            ssh_logger: ssh::SshLogger::new(),
        }
    }

    pub fn enable_debug(&self) {
        DEBUG_MODE.store(true, Ordering::SeqCst);
    }

    pub fn enable_ssh_logging(&self) {
        SSH_LOGGING.store(true, Ordering::SeqCst);
    }

    pub fn is_debug_enabled(&self) -> bool {
        DEBUG_MODE.load(Ordering::SeqCst)
    }

    pub fn is_ssh_logging_enabled(&self) -> bool {
        SSH_LOGGING.load(Ordering::SeqCst)
    }

    pub fn log_debug(&self, message: &str) -> Result<(), LogError> {
        if self.is_debug_enabled() {
            self.debug_logger.log(LogLevel::Debug, message)?;
        }
        Ok(())
    }

    pub fn log_ssh(&self, message: &str) -> Result<(), LogError> {
        if self.is_ssh_logging_enabled() {
            self.ssh_logger.log(message)?;
        }
        Ok(())
    }
}
