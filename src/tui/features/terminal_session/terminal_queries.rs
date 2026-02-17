//! Terminal query responses for PTY child processes.

use crate::log_debug;
use std::io::Write;
use std::sync::{Arc, Mutex};

/// Respond to terminal query sequences from programs running in the PTY.
///
/// Fish shell and other programs query terminal capabilities via escape
/// sequences like `ESC[c`, `ESC[>c`, and `ESC[6n`.
pub(crate) fn respond_to_terminal_queries(data: &[u8], writer: &Arc<Mutex<Box<dyn Write + Send>>>) {
    let mut scan_idx = 0;
    while scan_idx < data.len() {
        if data[scan_idx] == 0x1b && scan_idx + 1 < data.len() && data[scan_idx + 1] == b'[' {
            // Found CSI sequence start (ESC[)
            let mut param_idx = scan_idx + 2;
            let param_start = param_idx;

            // Collect parameter bytes (0x30-0x3F: digits, semicolons, >, etc.)
            while param_idx < data.len() && (0x30..=0x3F).contains(&data[param_idx]) {
                param_idx += 1;
            }

            let params = &data[param_start..param_idx];

            // Check for terminal query final bytes
            if param_idx < data.len() {
                let final_byte = data[param_idx];

                let response = match final_byte {
                    b'c' if params.is_empty() || params == b"0" => Some(b"\x1b[?62;1;2;6;9;15;22c".as_slice()),
                    b'c' if params.starts_with(b">") => Some(b"\x1b[>41;279;0c".as_slice()),
                    b'n' if params == b"5" => Some(b"\x1b[0n".as_slice()),
                    b'n' if params == b"6" => Some(b"\x1b[1;1R".as_slice()),
                    _ => None,
                };

                if let Some(response_bytes) = response {
                    log_debug!("Detected terminal query, sending response: {:?}", response_bytes);
                    if let Ok(mut writer_guard) = writer.lock() {
                        let _ = writer_guard.write_all(response_bytes);
                        let _ = writer_guard.flush();
                    }
                }

                scan_idx = param_idx + 1;
            } else {
                scan_idx = param_idx;
            }
        } else {
            scan_idx += 1;
        }
    }
}
