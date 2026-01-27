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
        $crate::log::LOGGER.is_debug_enabled()
    }};
}

/// Log a debug message
#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        let _ = $crate::log::LOGGER.log_debug(&format!($($arg)*));
    };
}

/// Log an informational message
#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        let _ = $crate::log::LOGGER.log_info(&format!($($arg)*));
    };
}

/// Log a warning message
#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        let _ = $crate::log::LOGGER.log_warn(&format!($($arg)*));
    };
}

/// Log an error message
#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        let _ = $crate::log::LOGGER.log_error(&format!($($arg)*));
    };
}

/// Log SSH session output
#[macro_export]
macro_rules! log_ssh {
    ($($arg:tt)*) => {
        let _ = $crate::log::LOGGER.log_ssh(&format!($($arg)*));
    };
}
