#[macro_export]
macro_rules! debug_enabled {
    () => {{
        let logger = $crate::log::Logger::new();
        logger.is_debug_enabled()
    }};
}

#[macro_export]
macro_rules! log_debug {
    ($($arg:tt)*) => {
        let logger = $crate::log::Logger::new();
        let _ = logger.log_debug(&format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_ssh {
    ($($arg:tt)*) => {
        let logger = $crate::log::Logger::new();
        let _ = logger.log_ssh(&format!($($arg)*));
    };
}
