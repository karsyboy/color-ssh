//! Log message formatting utilities
//!
//! Provides customizable formatting for log messages with support for:
//! - Timestamps in RFC3339 format
//! - Log level indicators
//! - Visual separators

use super::LogLevel;
use chrono::Local;

#[derive(Clone)]
pub(super) struct LogFormatter {
    include_timestamp: bool,
    include_level: bool,
    include_break: bool,
}

impl LogFormatter {
    // Construction.
    pub(super) fn new() -> Self {
        Self {
            include_timestamp: false,
            include_level: false,
            include_break: false,
        }
    }

    // Formatting.
    /// Format a log message with optional timestamp, level, and separator
    pub(super) fn format(&self, level: Option<LogLevel>, message: &str) -> String {
        let mut formatted = String::new();

        // Add timestamp with consistent formatting (e.g., "2026-01-25 14:30:45.123")
        if self.include_timestamp {
            formatted.push_str(&Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string());
            formatted.push(' ');
        }

        // Add log level with consistent padding for alignment
        if self.include_level
            && let Some(lvl) = level
        {
            formatted.push_str(&format!("[{:5}] ", lvl.as_str()));
        }

        // Add visual separator
        if self.include_break {
            formatted.push_str("█ ");
        }

        formatted.push_str(message);

        formatted
    }

    // Option toggles.
    pub(super) fn set_include_timestamp(&mut self, include: bool) {
        self.include_timestamp = include;
    }

    pub(super) fn set_include_level(&mut self, include: bool) {
        self.include_level = include;
    }

    pub(super) fn set_include_break(&mut self, include: bool) {
        self.include_break = include;
    }

    // Fast-path capability checks.
    pub(crate) fn uses_cached_timestamp_prefix_without_level(&self) -> bool {
        self.include_timestamp && self.include_break && !self.include_level
    }
}

impl Default for LogFormatter {
    fn default() -> Self {
        Self::new()
    }
}
