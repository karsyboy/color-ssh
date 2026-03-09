use crate::log_debug;
use crate::log_error;
use crate::terminal_core::TerminalEngine;
use std::io::Read;
use std::process::Child as ProcessChild;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

pub(crate) fn normalize_managed_output_newlines(bytes: &[u8], previous_ended_with_cr: &mut bool) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut ended_with_cr = *previous_ended_with_cr;

    for &byte in bytes {
        if byte == b'\n' && !ended_with_cr {
            normalized.push(b'\r');
        }
        normalized.push(byte);
        ended_with_cr = byte == b'\r';
    }

    *previous_ended_with_cr = ended_with_cr;
    normalized
}

pub(crate) fn spawn_output_reader<R>(name: &'static str, mut reader: R, engine: Arc<Mutex<TerminalEngine>>, render_epoch: Arc<AtomicU64>)
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut previous_ended_with_cr = false;
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(bytes_read) => {
                    let normalized = normalize_managed_output_newlines(&buf[..bytes_read], &mut previous_ended_with_cr);
                    if let Ok(mut engine) = engine.lock() {
                        engine.process_output(&normalized);
                        render_epoch.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(err) => {
                    log_error!("Error reading from {} stream: {}", name, err);
                    break;
                }
            }
        }
        log_debug!("{} reader thread exiting", name);
    });
}

pub(crate) fn spawn_process_exit_watcher(child: Arc<Mutex<ProcessChild>>, exited: Arc<Mutex<bool>>) {
    std::thread::spawn(move || {
        loop {
            let should_exit = match exited.lock() {
                Ok(exited) => *exited,
                Err(_) => true,
            };
            if should_exit {
                break;
            }

            let status = match child.lock() {
                Ok(mut child) => child.try_wait(),
                Err(err) => {
                    log_error!("Failed to lock managed child for exit polling: {}", err);
                    break;
                }
            };

            match status {
                Ok(Some(_)) => {
                    if let Ok(mut exited) = exited.lock() {
                        *exited = true;
                    }
                    break;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(err) => {
                    log_error!("Failed to poll RDP process state: {}", err);
                    if let Ok(mut exited) = exited.lock() {
                        *exited = true;
                    }
                    break;
                }
            }
        }
    });
}
