use crate::log;
use crate::log::SessionSshLogger;
use crate::{log_debug, log_error};
use std::io::{self, Read};
use std::sync::Arc;
use std::thread;

pub(crate) enum PtyLogTarget {
    Disabled,
    GlobalSsh,
    Session(SessionSshLogger),
}

impl PtyLogTarget {
    pub(crate) fn global_ssh() -> Self {
        Self::GlobalSsh
    }

    pub(crate) fn session(session_logger: Option<SessionSshLogger>) -> Self {
        match session_logger {
            Some(session_logger) => Self::Session(session_logger),
            None => Self::Disabled,
        }
    }

    fn log_chunk(&self, chunk: Arc<String>) {
        match self {
            Self::Disabled => {}
            Self::GlobalSsh => {
                if let Err(err) = log::LOGGER.log_ssh_raw_shared(chunk) {
                    log_error!("Failed to write session log data: {}", err);
                }
            }
            Self::Session(session_logger) => {
                if let Err(err) = session_logger.log_raw_shared(chunk) {
                    log_error!("Failed to write session log data: {}", err);
                }
            }
        }
    }

    fn log_bytes(&mut self, decoder: &mut Utf8ChunkDecoder, bytes: &[u8]) {
        if matches!(self, Self::Disabled) || bytes.is_empty() {
            return;
        }

        if let Some(chunk) = decoder.decode_read(bytes) {
            self.log_chunk(Arc::new(chunk));
        }
    }

    fn finish(&mut self, decoder: &mut Utf8ChunkDecoder) {
        if !matches!(self, Self::Disabled)
            && let Some(chunk) = decoder.finish()
        {
            self.log_chunk(Arc::new(chunk));
        }

        if let Self::Session(session_logger) = self
            && let Err(err) = session_logger.flush()
        {
            log_error!("Failed to flush session logs: {}", err);
        }
    }
}

#[derive(Debug)]
pub(crate) struct Utf8ChunkDecoder {
    pending_utf8: Vec<u8>,
}

impl Utf8ChunkDecoder {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            pending_utf8: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn decode_read(&mut self, bytes: &[u8]) -> Option<String> {
        if self.pending_utf8.is_empty()
            && let Ok(valid_chunk) = std::str::from_utf8(bytes)
        {
            return Some(valid_chunk.to_string());
        }

        self.pending_utf8.extend_from_slice(bytes);
        self.take_decoded_chunk()
    }

    pub(crate) fn finish(&mut self) -> Option<String> {
        if self.pending_utf8.is_empty() {
            None
        } else {
            let chunk = String::from_utf8_lossy(&self.pending_utf8).to_string();
            self.pending_utf8.clear();
            Some(chunk)
        }
    }

    fn take_decoded_chunk(&mut self) -> Option<String> {
        let mut chunk = String::new();

        loop {
            match std::str::from_utf8(&self.pending_utf8) {
                Ok(valid) => {
                    chunk.push_str(valid);
                    self.pending_utf8.clear();
                    break;
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0
                        && let Ok(valid) = std::str::from_utf8(&self.pending_utf8[..valid_up_to])
                    {
                        chunk.push_str(valid);
                        self.pending_utf8.drain(..valid_up_to);
                        continue;
                    }
                    if let Some(error_len) = err.error_len() {
                        chunk.push('\u{FFFD}');
                        self.pending_utf8.drain(..error_len);
                        continue;
                    }
                    break;
                }
            }
        }

        (!chunk.is_empty()).then_some(chunk)
    }
}

pub(crate) fn spawn_pty_output_reader<F, C>(
    thread_name: String,
    mut reader: Box<dyn Read + Send>,
    mut handle_bytes: F,
    mut handle_closed: C,
    mut log_target: PtyLogTarget,
) -> io::Result<()>
where
    F: FnMut(&[u8]) -> bool + Send + 'static,
    C: FnMut() + Send + 'static,
{
    thread::Builder::new().name(thread_name).spawn(move || {
        let mut buf = [0u8; 8192];
        let mut decoder = Utf8ChunkDecoder::with_capacity(buf.len());

        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    handle_closed();
                    break;
                }
                Ok(bytes_read) => {
                    let bytes = &buf[..bytes_read];
                    log_target.log_bytes(&mut decoder, bytes);
                    if !handle_bytes(bytes) {
                        break;
                    }
                }
                Err(err) => {
                    log_error!("Error reading from session output: {}", err);
                    handle_closed();
                    break;
                }
            }
        }

        log_target.finish(&mut decoder);
        log_debug!("Session output reader thread exiting");
    })?;

    Ok(())
}
