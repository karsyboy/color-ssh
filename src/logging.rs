use chrono::Local;
use once_cell::sync::Lazy;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

// A global buffer to accumulate output until full lines are available.
static OUTPUT_BUFFER: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

// Flag for enabling/disabling debug mode
pub static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

pub fn enable_debug_mode() {
    DEBUG_MODE.store(true, Ordering::Relaxed);
}

// Used to enable SSH logging in the logging module
pub static SSH_LOGGING: AtomicBool = AtomicBool::new(false);

pub fn enable_ssh_logging() {
    SSH_LOGGING.store(true, Ordering::Relaxed);
}

/// Log messages to a debug log file, if DEBUG_MODE is enabled
///
///  - `message`: The message to log
///
/// Writes the message to a file named "csh-debug.log".
pub fn log_debug(message: &str) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("csh-debug.log")?;
    writeln!(file, "{}", message)
}

/// Log SSH output to a log file, if SSH_LOGGING is enabled
///
/// - `chunk`: The chunk of output to log
/// - `args`: The arguments passed to the SSH command
///
/// Writes the output to a file named "HOSTNAME-MM-DD-YYYY.log" in the ".csh/ssh-logs" directory.
pub fn log_ssh_output(chunk: &str, args: &[String]) -> io::Result<()> {
    // Lock the global output buffer and append the new chunk.
    let mut buffer = OUTPUT_BUFFER.lock().unwrap();
    buffer.push_str(chunk);

    // Process every complete line (separated by '\n') in the buffer.
    while let Some(newline_pos) = buffer.find('\n') {
        // Extract one complete line (without the newline).
        let line = buffer[..newline_pos].trim_end().to_string();
        // Remove the processed line (and the newline) from the buffer.
        *buffer = buffer[newline_pos + 1..].to_string();

        // Build the log file path.
        let mut path = dirs::home_dir().expect("Unable to get home directory\r");
        path.push(".csh");
        path.push("ssh-logs");
        // Use the current date for the folder name.
        let now = Local::now();
        let date_folder = now.format("%m-%d-%Y").to_string();
        path.push(&date_folder);
        std::fs::create_dir_all(&path)?;

        // Build the log filename: HOSTNAME-MM-DD-YYYY.log
        let hostname = args
            .get(0)
            .expect("Expected at least one argument for hostname\r");
        let file_name = format!("{}-{}.log", hostname, now.format("%m-%d-%Y"));

        let mut file_path = PathBuf::from(&path);
        file_path.push(&file_name);

        // Open (or create) the log file in append mode.
        let mut ssh_log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;

        // Write the complete line to the log file.
        writeln!(ssh_log_file, "{}", line)?;
    }

    Ok(())
}
