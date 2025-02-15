#[macro_export]
macro_rules! debug_enabled {
    () => {
        if let Ok(logger) = $crate::logging::Logger::global().try_lock() {
            logger.is_debug_enabled()
        } else {
            false
        }
    };
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {{
        if let Ok(logger) = $crate::logging::Logger::global().try_lock() {
            let _ = logger.log_debug(&format!($($arg)*));
        }
    }};
}

#[macro_export]
macro_rules! log_ssh {
    ($($arg:tt)*) => {
        if let Ok(logger) = $crate::logging::Logger::global().lock() {
            let _ = logger.log_ssh(&format!($($arg)*));
        }
    };
}
