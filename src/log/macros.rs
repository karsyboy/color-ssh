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
    () => {{ $crate::log::LOGGER.is_debug_enabled() }};
}

/// Log a debug message
#[macro_export]
macro_rules! log_debug {
    ($msg:expr) => {{
        if $crate::log::LOGGER.is_debug_enabled() {
            let _ = $crate::log::LOGGER.log_debug(::core::convert::AsRef::<str>::as_ref(&$msg));
        }
    }};
    ($($arg:tt)*) => {{
        if $crate::log::LOGGER.is_debug_enabled() {
            let _ = $crate::log::LOGGER.log_debug(&format!($($arg)*));
        }
    }};
}

/// Log an informational message
#[macro_export]
macro_rules! log_info {
    ($msg:expr) => {{
        if $crate::log::LOGGER.is_debug_enabled() {
            let _ = $crate::log::LOGGER.log_info(::core::convert::AsRef::<str>::as_ref(&$msg));
        }
    }};
    ($($arg:tt)*) => {{
        if $crate::log::LOGGER.is_debug_enabled() {
            let _ = $crate::log::LOGGER.log_info(&format!($($arg)*));
        }
    }};
}

/// Log a warning message
#[macro_export]
macro_rules! log_warn {
    ($msg:expr) => {{
        if $crate::log::LOGGER.is_debug_enabled() {
            let _ = $crate::log::LOGGER.log_warn(::core::convert::AsRef::<str>::as_ref(&$msg));
        }
    }};
    ($($arg:tt)*) => {{
        if $crate::log::LOGGER.is_debug_enabled() {
            let _ = $crate::log::LOGGER.log_warn(&format!($($arg)*));
        }
    }};
}

/// Log an error message
#[macro_export]
macro_rules! log_error {
    ($msg:expr) => {{
        if $crate::log::LOGGER.is_debug_enabled() {
            let _ = $crate::log::LOGGER.log_error(::core::convert::AsRef::<str>::as_ref(&$msg));
        }
    }};
    ($($arg:tt)*) => {{
        if $crate::log::LOGGER.is_debug_enabled() {
            let _ = $crate::log::LOGGER.log_error(&format!($($arg)*));
        }
    }};
}

/// Log SSH session output
#[macro_export]
macro_rules! log_ssh {
    ($($arg:tt)*) => {{
        if $crate::log::LOGGER.is_ssh_logging_enabled() {
            let _ = $crate::log::LOGGER.log_ssh(&format!($($arg)*));
        }
    }};
}

#[cfg(test)]
mod tests {
    use crate::log::LOGGER;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    static LOG_MODE_TEST_LOCK: Mutex<()> = Mutex::new(());
    static FORMAT_SIDE_EFFECT: AtomicUsize = AtomicUsize::new(0);

    fn side_effect_value() -> usize {
        FORMAT_SIDE_EFFECT.fetch_add(1, Ordering::Relaxed);
        42
    }

    #[test]
    fn log_debug_does_not_evaluate_format_args_when_disabled() {
        let _guard = LOG_MODE_TEST_LOCK.lock().expect("test lock");
        let was_debug = LOGGER.is_debug_enabled();
        LOGGER.disable_debug();
        FORMAT_SIDE_EFFECT.store(0, Ordering::Relaxed);

        crate::log_debug!("debug side effect {}", side_effect_value());
        assert_eq!(FORMAT_SIDE_EFFECT.load(Ordering::Relaxed), 0);

        if was_debug {
            LOGGER.enable_debug();
        } else {
            LOGGER.disable_debug();
        }
    }

    #[test]
    fn log_ssh_does_not_evaluate_format_args_when_disabled() {
        let _guard = LOG_MODE_TEST_LOCK.lock().expect("test lock");
        let was_enabled = LOGGER.is_ssh_logging_enabled();
        LOGGER.disable_ssh_logging();
        FORMAT_SIDE_EFFECT.store(0, Ordering::Relaxed);

        crate::log_ssh!("ssh side effect {}", side_effect_value());
        assert_eq!(FORMAT_SIDE_EFFECT.load(Ordering::Relaxed), 0);

        if was_enabled {
            LOGGER.enable_ssh_logging();
        } else {
            LOGGER.disable_ssh_logging();
        }
    }
}
