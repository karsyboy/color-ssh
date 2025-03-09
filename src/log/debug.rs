use super::{LogError, LogLevel, formatter::LogFormatter};
use once_cell::sync::Lazy;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};

static DEBUG_LOG_FILE: Lazy<Mutex<Option<File>>> = Lazy::new(|| Mutex::new(None));

#[derive(Clone)]
pub struct DebugLogger {
    formatter: LogFormatter,
}

impl DebugLogger {
    pub fn new() -> Self {
        let mut formatter = LogFormatter::new();
        formatter.set_include_timestamp(true);
        formatter.set_include_level(true);

        Self { formatter: formatter }
    }

    pub fn log(&self, level: LogLevel, message: &str) -> Result<(), LogError> {
        let formatted = self.formatter.format(Some(level), message);
        let mut file_guard = DEBUG_LOG_FILE.lock().unwrap();

        if file_guard.is_none() {
            *file_guard = Some(self.create_log_file()?);
        }

        if let Some(file) = file_guard.as_mut() {
            writeln!(file, "{}", formatted)?;
            file.flush()?;
        }

        Ok(())
    }

    fn create_log_file(&self) -> Result<File, LogError> {
        let log_path = self.get_debug_log_path()?;

        OpenOptions::new().create(true).append(true).open(log_path).map_err(LogError::from)
    }

    fn get_debug_log_path(&self) -> Result<PathBuf, LogError> {
        let home_dir = dirs::home_dir().ok_or_else(|| LogError::DirectoryCreationError("Home directory not found".to_string()))?;

        let log_dir = home_dir.join(".csh").join("logs");
        std::fs::create_dir_all(&log_dir)?;

        Ok(log_dir.join("debug.log"))
    }
}
