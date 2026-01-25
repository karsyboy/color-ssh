//! Logging macros for convenient logging throughout the codebase
//!
//! These macros provide a simple interface to the logging system:
//! - `debug_enabled!()` - Check if debug mode is enabled
//! - `log_debug!(...)` - Log debug messages
//! - `log_info!(...)` - Log informational messages
//! - `log_warn!(...)` - Log warning messages
//! - `log_error!(...)` - Log error messages
//! - `log_ssh!(...)` - Log SSH session output

/// Check if debug logging is enabled
#[macro_export]
macro_rules! debug_enabled {
    () => {{
        let logger = $crate::log::Logger::new();
        logger.is_debug_enabled()
    }};
}

/// Log a debug message (only when debug mode is enabled)
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        let logger = $crate::log::Logger::new();
        let _ = logger.log_debug(&format!($($arg)*));
    };
}

/// Log an informational message (only when debug mode is enabled)
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        let logger = $crate::log::Logger::new();
        let _ = logger.log_info(&format!($($arg)*));
    };
}

/// Log a warning message (only when debug mode is enabled)
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        let logger = $crate::log::Logger::new();
        let _ = logger.log_warn(&format!($($arg)*));
    };
}

/// Log an error message (only when debug mode is enabled)
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        let logger = $crate::log::Logger::new();
        let _ = logger.log_error(&format!($($arg)*));
    };
}

/// Log SSH session output (only when SSH logging is enabled)
#[macro_export]
macro_rules! log_ssh {
    ($($arg:tt)*) => {
        let logger = $crate::log::Logger::new();
        let _ = logger.log_ssh(&format!($($arg)*));
    };
}
