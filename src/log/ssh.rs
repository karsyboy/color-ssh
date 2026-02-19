//! SSH session logging utilities
//!
//! Provides structured logging for SSH sessions with:
//! - Session output logging
//! - Secret redaction based on patterns
//! - ANSI escape sequence filtering
//! - Per-session log files organized by date

use super::{LogError, formatter::LogFormatter, sanitize_session_name};
use chrono::Local;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    borrow::Cow,
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

const SSH_LOG_FLUSH_BYTES: usize = 64 * 1024;
const SSH_LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(100);
// 1024 * ~8KiB chunks ~= ~8MiB bounded backlog.
const SSH_LOG_QUEUE_CAPACITY: usize = 1024;

enum SshLogCommand {
    Chunk(String),
    Flush(SyncSender<Result<(), String>>),
}

type LogFileFactory = Arc<dyn Fn() -> Result<File, LogError> + Send + Sync>;

struct SshLogWorkerState {
    line_buffer: String,
    writer: Option<BufWriter<File>>,
    pending_bytes: usize,
    last_flush: Instant,
    cached_secret_patterns: Vec<Regex>,
    cached_config_version: Option<u64>,
    last_error: Option<String>,
    cached_timestamp_millis: Option<i64>,
    cached_prefix: String,
}

impl SshLogWorkerState {
    // Worker state construction.
    fn new() -> Self {
        Self {
            line_buffer: String::new(),
            writer: None,
            pending_bytes: 0,
            last_flush: Instant::now(),
            cached_secret_patterns: Vec::new(),
            cached_config_version: None,
            last_error: None,
            cached_timestamp_millis: None,
            cached_prefix: String::new(),
        }
    }
}

// Compiled regex for removing ANSI escape sequences
static ANSI_ESCAPE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        \x1B\[[\x30-\x3F]*[\x20-\x2F]*[\x40-\x7E]    # CSI: ESC [ params intermediates final
        |\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)           # OSC: ESC ] ... (BEL or ESC \)
        |\x1B[PX^_].*?\x1B\\                         # DCS/SOS/PM/APC: ESC P/X/^/_ ... ESC \
        |\x1B.                                       # Other ESC sequences (2 bytes)
        |\x1B                                        # Stray ESC character
    ",
    )
    .unwrap()
});

#[derive(Clone)]
pub(super) struct SshLogger {
    formatter: LogFormatter,
    worker_tx: Arc<Mutex<Option<SyncSender<SshLogCommand>>>>,
}

impl Default for SshLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl SshLogger {
    // Construction.
    pub(super) fn new() -> Self {
        let mut formatter = LogFormatter::new();
        formatter.set_include_timestamp(true);
        formatter.set_include_break(true);

        Self {
            formatter,
            worker_tx: Arc::new(Mutex::new(None)),
        }
    }

    // Public write/flush entry points.
    pub(super) fn log(&self, message: &str) -> Result<(), LogError> {
        self.log_raw(message)
    }

    pub(super) fn log_raw(&self, message: &str) -> Result<(), LogError> {
        let tx = self.ensure_worker()?;
        tx.send(SshLogCommand::Chunk(message.to_string()))
            .map_err(|err| LogError::FormattingError(format!("failed to enqueue ssh log chunk: {}", err)))
    }

    pub(super) fn flush(&self) -> Result<(), LogError> {
        let tx = self.ensure_worker()?;
        let (ack_tx, ack_rx) = mpsc::sync_channel(0);

        tx.send(SshLogCommand::Flush(ack_tx))
            .map_err(|err| LogError::FormattingError(format!("failed to enqueue ssh log flush: {}", err)))?;

        match ack_rx.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err_msg)) => Err(LogError::FormattingError(err_msg)),
            Err(err) => Err(LogError::FormattingError(format!("failed waiting for ssh log flush ack: {}", err))),
        }
    }

    // Worker lifecycle.
    fn ensure_worker(&self) -> Result<SyncSender<SshLogCommand>, LogError> {
        let mut worker_tx_guard = self.worker_tx.lock().unwrap();
        if let Some(existing_tx) = worker_tx_guard.as_ref() {
            return Ok(existing_tx.clone());
        }

        let (tx, rx) = mpsc::sync_channel(SSH_LOG_QUEUE_CAPACITY);
        let formatter = self.formatter.clone();
        let file_factory: LogFileFactory = Arc::new(SshLogger::create_log_file);

        thread::Builder::new()
            .name("ssh-log-writer".to_string())
            .spawn(move || run_worker(rx, formatter, file_factory))
            .map_err(|err| LogError::FormattingError(format!("failed to spawn ssh log worker: {}", err)))?;

        *worker_tx_guard = Some(tx.clone());
        Ok(tx)
    }

    // File creation helper.
    fn create_log_file() -> Result<File, LogError> {
        let log_path = get_ssh_log_path()?;

        OpenOptions::new().create(true).append(true).open(log_path).map_err(LogError::from)
    }
}

fn run_worker(receiver: Receiver<SshLogCommand>, formatter: LogFormatter, file_factory: LogFileFactory) {
    let mut state = SshLogWorkerState::new();

    loop {
        match receiver.recv_timeout(SSH_LOG_FLUSH_INTERVAL) {
            Ok(SshLogCommand::Chunk(message)) => {
                if let Err(err) = process_chunk_message(&mut state, &formatter, &message, file_factory.as_ref()) {
                    state.last_error = Some(err.to_string());
                }
            }
            Ok(SshLogCommand::Flush(ack_tx)) => {
                let flush_result = flush_worker(&mut state).map_err(|err| err.to_string());
                let _ = ack_tx.send(flush_result);
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Err(err) = flush_if_due(&mut state) {
                    state.last_error = Some(err.to_string());
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = flush_worker(&mut state);
                break;
            }
        }
    }
}

fn process_chunk_message(
    state: &mut SshLogWorkerState,
    formatter: &LogFormatter,
    message: &str,
    create_log_file: &dyn Fn() -> Result<File, LogError>,
) -> Result<(), LogError> {
    state.line_buffer.push_str(message);
    let lines = extract_complete_lines(&mut state.line_buffer);

    if lines.is_empty() {
        return Ok(());
    }

    refresh_secret_patterns_if_needed(
        &mut state.cached_config_version,
        crate::config::current_config_version(),
        &mut state.cached_secret_patterns,
        current_secret_patterns,
    );

    if state.writer.is_none() {
        state.writer = Some(BufWriter::new(create_log_file()?));
    }

    for line in lines {
        if line.is_empty() {
            continue;
        }

        let sanitized = sanitize_line(&line, &state.cached_secret_patterns);
        if sanitized.is_empty() {
            continue;
        }

        let formatted = format_log_line(state, formatter, sanitized.as_ref());
        if let Some(writer) = state.writer.as_mut() {
            writer.write_all(formatted.as_bytes())?;
            writer.write_all(b"\n")?;
        }
        state.pending_bytes = state.pending_bytes.saturating_add(formatted.len() + 1);
    }

    flush_if_due(state)
}

fn format_log_line(state: &mut SshLogWorkerState, formatter: &LogFormatter, message: &str) -> String {
    if formatter.uses_cached_timestamp_prefix_without_level() {
        let now = Local::now();
        let now_millis = now.timestamp_millis();
        if state.cached_timestamp_millis != Some(now_millis) {
            state.cached_prefix.clear();
            state.cached_prefix.push_str(&now.format("%Y-%m-%d %H:%M:%S%.3f").to_string());
            state.cached_prefix.push(' ');
            state.cached_prefix.push('█');
            state.cached_prefix.push(' ');
            state.cached_timestamp_millis = Some(now_millis);
        }

        let mut formatted = String::with_capacity(state.cached_prefix.len().saturating_add(message.len()));
        formatted.push_str(&state.cached_prefix);
        formatted.push_str(message);
        formatted
    } else {
        formatter.format(None, message)
    }
}

fn flush_if_due(state: &mut SshLogWorkerState) -> Result<(), LogError> {
    if should_flush(state.pending_bytes, state.last_flush.elapsed()) {
        flush_writer(state)?;
    }
    Ok(())
}

fn flush_writer(state: &mut SshLogWorkerState) -> Result<(), LogError> {
    if let Some(writer) = state.writer.as_mut() {
        writer.flush()?;
        state.pending_bytes = 0;
        state.last_flush = Instant::now();
    }
    Ok(())
}

fn flush_worker(state: &mut SshLogWorkerState) -> Result<(), LogError> {
    flush_writer(state)?;
    if let Some(last_error) = state.last_error.take() {
        return Err(LogError::FormattingError(last_error));
    }
    Ok(())
}

fn refresh_secret_patterns_if_needed<F>(cached_version: &mut Option<u64>, current_version: u64, cached_patterns: &mut Vec<Regex>, mut load_patterns: F)
where
    F: FnMut() -> Vec<Regex>,
{
    if cached_version.is_some_and(|version| version == current_version) {
        return;
    }

    *cached_patterns = load_patterns();
    *cached_version = Some(current_version);
}

fn get_ssh_log_path() -> Result<PathBuf, LogError> {
    let home_dir = dirs::home_dir().ok_or_else(|| LogError::DirectoryCreationError("Home directory not found".to_string()))?;

    let date = Local::now().format("%Y-%m-%d");
    let log_dir = home_dir.join(".color-ssh").join("logs").join("ssh_sessions").join(date.to_string());

    std::fs::create_dir_all(&log_dir)?;

    let session_name = crate::config::get_config().read().unwrap().metadata.session_name.clone();
    let sanitized = sanitize_session_name(&session_name);
    Ok(log_dir.join(format!("{sanitized}.log")))
}

fn current_secret_patterns() -> Vec<Regex> {
    crate::config::SESSION_CONFIG
        .get()
        .and_then(|config| config.read().ok().map(|config_guard| config_guard.metadata.compiled_secret_patterns.clone()))
        .unwrap_or_default()
}

fn sanitize_line<'a>(line: &'a str, secret_patterns: &[Regex]) -> Cow<'a, str> {
    let cleaned = if line.as_bytes().contains(&0x1b) {
        ANSI_ESCAPE_REGEX.replace_all(line, "")
    } else {
        Cow::Borrowed(line)
    };

    let needs_filter = cleaned
        .chars()
        .any(|ch| !(ch.is_alphanumeric() || ch.is_ascii_punctuation() || ch.is_whitespace()) || ch == '\n' || ch == '\r');

    if !needs_filter && secret_patterns.iter().all(|regex| !regex.is_match(cleaned.as_ref())) {
        return cleaned;
    }

    let mut sanitized = if needs_filter {
        cleaned
            .chars()
            .filter(|ch| (ch.is_alphanumeric() || ch.is_ascii_punctuation() || ch.is_whitespace()) && *ch != '\n' && *ch != '\r')
            .collect::<String>()
    } else {
        cleaned.into_owned()
    };

    for regex in secret_patterns {
        if regex.is_match(&sanitized) {
            sanitized = regex.replace_all(&sanitized, "[REDACTED]").into_owned();
        }
    }

    Cow::Owned(sanitized)
}

fn extract_complete_lines(buffer: &mut String) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0usize;

    while let Some(relative_newline) = buffer[start..].find('\n') {
        let end = start + relative_newline;
        lines.push(buffer[start..end].trim_end_matches('\r').to_string());
        start = end + 1;
    }

    if start > 0 {
        buffer.drain(..start);
    }

    lines
}

fn should_flush(pending_bytes: usize, elapsed_since_flush: Duration) -> bool {
    pending_bytes >= SSH_LOG_FLUSH_BYTES || elapsed_since_flush >= SSH_LOG_FLUSH_INTERVAL
}

#[cfg(test)]
mod tests {
    use super::{LogFileFactory, SshLogCommand, extract_complete_lines, refresh_secret_patterns_if_needed, run_worker, sanitize_line, should_flush};
    use regex::Regex;
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, mpsc},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn temp_log_path() -> PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock should be after epoch").as_nanos();
        std::env::temp_dir().join(format!("cossh-ssh-log-test-{}.log", unique))
    }

    #[test]
    fn extract_complete_lines_keeps_partial_tail() {
        let mut buffer = "one\ntwo\npartial".to_string();
        let lines = extract_complete_lines(&mut buffer);

        assert_eq!(lines, vec!["one".to_string(), "two".to_string()]);
        assert_eq!(buffer, "partial");
    }

    #[test]
    fn sanitize_line_strips_ansi_and_redacts_patterns() {
        let secrets = vec![Regex::new("token=\\w+").expect("regex compiles")];
        let line = "\x1b[31mtoken=abc123\x1b[0m ok";
        let sanitized = sanitize_line(line, &secrets);
        assert_eq!(sanitized, "[REDACTED] ok");
    }

    #[test]
    fn should_flush_on_size_or_interval() {
        assert!(!should_flush(1024, Duration::from_millis(20)));
        assert!(should_flush(64 * 1024, Duration::from_millis(20)));
        assert!(should_flush(1, Duration::from_millis(100)));
    }

    #[test]
    fn refresh_secret_patterns_only_reloads_on_version_change() {
        let mut cached_version = None;
        let mut cached_patterns: Vec<Regex> = Vec::new();
        let mut loads = 0usize;

        refresh_secret_patterns_if_needed(&mut cached_version, 1, &mut cached_patterns, || {
            loads += 1;
            vec![Regex::new("token").expect("regex")]
        });
        assert_eq!(loads, 1);

        refresh_secret_patterns_if_needed(&mut cached_version, 1, &mut cached_patterns, || {
            loads += 1;
            vec![Regex::new("never-called").expect("regex")]
        });
        assert_eq!(loads, 1);

        refresh_secret_patterns_if_needed(&mut cached_version, 2, &mut cached_patterns, || {
            loads += 1;
            vec![Regex::new("token2").expect("regex")]
        });
        assert_eq!(loads, 2);
    }

    #[test]
    fn worker_preserves_order_and_flush_barrier() {
        let log_path = temp_log_path();
        let (tx, rx) = mpsc::sync_channel(8);

        let mut formatter = crate::log::formatter::LogFormatter::new();
        formatter.set_include_timestamp(false);
        formatter.set_include_break(false);
        let path_for_worker = log_path.clone();
        let file_factory: LogFileFactory = Arc::new(move || {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path_for_worker)
                .map_err(crate::log::LogError::from)
        });

        let worker = std::thread::spawn(move || {
            run_worker(rx, formatter, file_factory);
        });

        tx.send(SshLogCommand::Chunk("line-one\n".to_string())).expect("send line one");
        tx.send(SshLogCommand::Chunk("line-two\n".to_string())).expect("send line two");

        let (ack_tx, ack_rx) = mpsc::sync_channel(0);
        tx.send(SshLogCommand::Flush(ack_tx)).expect("send flush");
        assert!(ack_rx.recv().expect("flush ack").is_ok());

        drop(tx);
        worker.join().expect("worker should exit cleanly");
        let content = fs::read_to_string(&log_path).expect("read log file");
        assert!(content.contains("line-one"));
        assert!(content.contains("line-two"));
        assert!(content.find("line-one").expect("line one exists") < content.find("line-two").expect("line two exists"));
        let _ = fs::remove_file(log_path);
    }
}
