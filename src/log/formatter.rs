/*
TODO:
    - Add a formatter option to remove passwords form the log file
*/

use crate::log::LogLevel;
use chrono::Local;

#[derive(Clone)]
pub struct LogFormatter {
    include_timestamp: bool,
    include_level: bool,
}

impl LogFormatter {
    pub fn new(include_timestamp: bool, include_level: bool) -> Self {
        Self {
            include_timestamp: include_timestamp,
            include_level: include_level,
        }
    }

    pub fn format(&self, level: Option<LogLevel>, message: &str) -> String {
        let mut formatted = String::new();

        if self.include_timestamp {
            formatted.push_str(&Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string());
            formatted.push(' ');
        }

        if self.include_level {
            if let Some(lvl) = level {
                formatted.push_str(&format!("[{}] ", lvl.as_str()));
            }
        }

        formatted.push_str(message);

        formatted
    }

    pub fn set_include_timestamp(&mut self, include: bool) {
        self.include_timestamp = include;
    }

    pub fn set_include_level(&mut self, include: bool) {
        self.include_level = include;
    }
}

impl Default for LogFormatter {
    fn default() -> Self {
        Self::new(true, true)
    }
}
