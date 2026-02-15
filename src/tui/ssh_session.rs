//! SSH session spawning and PTY management

use super::{App, HostTab, SshSession};
use crate::ssh_config::SshHost;
use crate::{log_debug, log_error};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use vt100::Parser;

/// Respond to terminal query sequences from programs running in the PTY.
///
/// Fish shell and other programs query terminal capabilities via escape sequences like:
/// - ESC[c (Primary Device Attributes - DA1)
/// - ESC[>c (Secondary Device Attributes - DA2)  
/// - ESC[6n (Cursor Position Report - CPR)
///
/// Since we're running in a TUI that has taken over the terminal, we emulate a modern xterm
/// terminal and respond directly. This prevents fish from timing out waiting for responses.
fn respond_to_terminal_queries(data: &[u8], writer: &Arc<Mutex<Box<dyn Write + Send>>>) {
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'[' {
            // Found CSI sequence start (ESC[)
            let mut j = i + 2;
            let param_start = j;

            // Collect parameter bytes (0x30-0x3F: digits, semicolons, >, etc.)
            while j < data.len() && (0x30..=0x3F).contains(&data[j]) {
                j += 1;
            }

            let params = &data[param_start..j];

            // Check for terminal query final bytes
            if j < data.len() {
                let final_byte = data[j];

                // Generate appropriate response for terminal capability queries
                let response = match final_byte {
                    // DA1 - Primary Device Attributes query: ESC[0c or ESC[c
                    b'c' if params.is_empty() || params == b"0" => {
                        // Respond as VT220 with various capabilities
                        // CSI ? 62 ; 1 ; 2 ; 6 ; 9 ; 15 ; 22 c
                        // (VT220, 132-columns, ANSI color, National Replacement Character sets...)
                        // The vt100 parser handles most of these features already
                        Some(b"\x1b[?62;1;2;6;9;15;22c".as_slice())
                    }
                    // DA2 - Secondary Device Attributes query: ESC[>c or ESC[>0c
                    b'c' if params.starts_with(b">") => {
                        // Respond as xterm version 279 (common modern xterm)
                        // CSI > 41 ; 279 ; 0 c
                        Some(b"\x1b[>41;279;0c".as_slice())
                    }
                    // DSR - Device Status Report query: ESC[5n
                    b'n' if params == b"5" => {
                        // Respond: terminal is ready/OK
                        // CSI 0 n
                        Some(b"\x1b[0n".as_slice())
                    }
                    // CPR - Cursor Position Report query: ESC[6n
                    b'n' if params == b"6" => {
                        // Respond with a static cursor position
                        // CSI 1 ; 1 R
                        // NOTE: This is not the real cursor position, but fish mainly needs
                        // *a* response to avoid timeouts. The actual position is used for
                        // optional features like truncating multiline autosuggestions.
                        Some(b"\x1b[1;1R".as_slice())
                    }
                    _ => None,
                };

                if let Some(response_bytes) = response {
                    // Write response back to PTY so the application (fish) receives it
                    log_debug!("Detected terminal query, sending response: {:?}", response_bytes);
                    if let Ok(mut w) = writer.lock() {
                        let _ = w.write_all(response_bytes);
                        let _ = w.flush();
                    }
                }

                i = j + 1;
            } else {
                i = j;
            }
        } else {
            i += 1;
        }
    }
}

/// Scan PTY output for OSC 52 clipboard sequences and forward them to stdout.
///
/// OSC 52 format: `ESC ] 52 ; <selection> ; <base64-data> BEL` or `ESC ] 52 ; <selection> ; <base64-data> ESC \`
/// `osc_buf` accumulates partial sequences across read boundaries.
fn forward_osc52(data: &[u8], osc_buf: &mut Vec<u8>) {
    // If we're accumulating an OSC sequence, append new data
    if !osc_buf.is_empty() {
        osc_buf.extend_from_slice(data);
        // Check for terminator: BEL (0x07) or ST (ESC \)
        if let Some(end) = find_osc_end(osc_buf) {
            let seq = &osc_buf[..end];
            let mut stdout = io::stdout();
            let _ = stdout.write_all(seq);
            let _ = stdout.flush();
            // There might be more data after the sequence; scan remainder
            let rest = osc_buf[end..].to_vec();
            osc_buf.clear();
            if !rest.is_empty() {
                forward_osc52(&rest, osc_buf);
            }
        } else if osc_buf.len() > 100_000 {
            // Safety: don't accumulate forever if no terminator found
            osc_buf.clear();
        }
        return;
    }

    // Scan for OSC 52 start: ESC ] 52 ;
    let mut i = 0;
    while i < data.len() {
        // Look for ESC (0x1b)
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b']' {
            // Check if this is OSC 52
            let rest = &data[i + 2..];
            if rest.starts_with(b"52;") {
                // Found OSC 52 start — look for terminator in remaining data
                let seq_start = i;
                if let Some(end_offset) = find_osc_end(&data[seq_start..]) {
                    // Complete sequence — forward it
                    let seq = &data[seq_start..seq_start + end_offset];
                    let mut stdout = io::stdout();
                    let _ = stdout.write_all(seq);
                    let _ = stdout.flush();
                    i = seq_start + end_offset;
                    continue;
                } else {
                    // Partial sequence — buffer it for next read
                    osc_buf.extend_from_slice(&data[seq_start..]);
                    return;
                }
            }
        }
        i += 1;
    }
}

/// Find the end of an OSC sequence (after BEL or ST terminator), returning the byte position after the terminator.
fn find_osc_end(data: &[u8]) -> Option<usize> {
    for i in 0..data.len() {
        if data[i] == 0x07 {
            // BEL terminator
            return Some(i + 1);
        }
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'\\' {
            // ST terminator (ESC \)
            return Some(i + 2);
        }
    }
    None
}

impl App {
    /// Select a host to open in a new tab
    pub(super) fn select_host_to_connect(&mut self) {
        let Some(host_idx) = self.selected_host_idx() else {
            return;
        };
        let host = self.hosts[host_idx].clone();

        log_debug!("Opening tab for host: {}", host.name);

        // Generate unique tab title with suffix for duplicate hosts
        let existing_count = self.tabs.iter().filter(|tab| tab.host.name == host.name).count();
        let tab_title = if existing_count == 0 {
            host.name.clone()
        } else {
            format!("{}_{}", host.name, existing_count)
        };

        // Spawn SSH session
        let session = match Self::spawn_ssh_session(&host, &tab_title, self.history_buffer) {
            Ok(session) => Some(session),
            Err(e) => {
                log_error!("Failed to spawn SSH session: {}", e);
                None
            }
        };

        // Create new tab
        let tab = HostTab {
            title: tab_title,
            host: host.clone(),
            session,
            scroll_offset: 0,
        };

        self.tabs.push(tab);
        self.selected_tab = self.tabs.len() - 1;
        self.focus_on_manager = false;

        log_debug!("Created new tab at index {}", self.selected_tab);
    }

    /// Spawn an SSH session in a PTY
    fn spawn_ssh_session(host: &SshHost, tab_title: &str, history_buffer: usize) -> io::Result<SshSession> {
        let pty_system = native_pty_system();

        // Create a new PTY with initial size (will be resized later)
        let pty_pair = pty_system
            .openpty(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| io::Error::other(e.to_string()))?;

        // Build cossh command to get syntax highlighting
        let cossh_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("cossh"));

        let mut cmd = if host.use_sshpass {
            // Use sshpass -e to pass the password from SSHPASS env var
            let mut c = CommandBuilder::new("sshpass");
            c.arg("-e");
            c.arg(&cossh_path);
            c
        } else {
            CommandBuilder::new(&cossh_path)
        };

        cmd.arg(&host.name);
        cmd.env("COSSH_SESSION_NAME", tab_title);

        // Pass profile if specified in .ssh/config via #_Profile
        if let Some(profile) = &host.profile {
            cmd.arg("-P");
            cmd.arg(profile);
        }

        let sshpass_info = if host.use_sshpass { " (via sshpass)" } else { "" };
        let profile_info = host.profile.as_ref().map_or(String::new(), |p| format!(" [profile: {}]", p));
        log_debug!(
            "Spawning cossh command: cossh {}{}{} (session: {})",
            host.name,
            sshpass_info,
            profile_info,
            tab_title
        );

        // Spawn the command in the PTY
        let child = pty_pair.slave.spawn_command(cmd).map_err(|e| io::Error::other(e.to_string()))?;

        // Get the master for reading/writing
        let mut reader = pty_pair.master.try_clone_reader().map_err(|e| io::Error::other(e.to_string()))?;
        let writer = pty_pair.master.take_writer().map_err(|e| io::Error::other(e.to_string()))?;

        // Create VT100 parser for terminal emulation
        let parser = Arc::new(Mutex::new(Parser::new(40, 120, history_buffer)));
        let parser_clone = parser.clone();
        let exited = Arc::new(Mutex::new(false));
        let exited_clone = exited.clone();
        let pty_master = Arc::new(Mutex::new(pty_pair.master));
        let writer = Arc::new(Mutex::new(writer));
        let writer_clone = writer.clone();
        let clear_pending = Arc::new(Mutex::new(false));
        let clear_pending_clone = clear_pending.clone();

        // Spawn a thread to read from PTY
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            // Buffer for accumulating a partial OSC 52 sequence across reads
            let mut osc_buf: Vec<u8> = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF - process exited
                        if let Ok(mut exited) = exited_clone.lock() {
                            *exited = true;
                        }
                        break;
                    }
                    Ok(n) => {
                        let data = &buf[..n];

                        // Respond to terminal capability queries from PTY
                        // (fixes fish shell DA query timeout warning)
                        respond_to_terminal_queries(data, &writer_clone);

                        // Forward any OSC 52 clipboard sequences to the real terminal
                        // so the inner TUI app's copy reaches the system clipboard.
                        forward_osc52(data, &mut osc_buf);

                        // Detect clear screen sequences: \x1b[2J or \x1b[3J
                        if Self::contains_clear_sequence(data) {
                            if let Ok(mut clear) = clear_pending_clone.lock() {
                                *clear = true;
                            }
                        }

                        if let Ok(mut parser) = parser_clone.lock() {
                            // Process the data through VT100 parser
                            parser.process(data);
                        }
                    }
                    Err(e) => {
                        log_error!("Error reading from PTY: {}", e);
                        if let Ok(mut exited) = exited_clone.lock() {
                            *exited = true;
                        }
                        break;
                    }
                }
            }
            log_debug!("PTY reader thread exiting");
        });

        Ok(SshSession {
            pty_master,
            writer,
            _child: child,
            parser,
            exited,
            clear_pending,
        })
    }

    /// Check if data contains a clear screen sequence
    fn contains_clear_sequence(data: &[u8]) -> bool {
        // Look for ESC[2J (clear screen) or ESC[3J (clear scrollback)
        // Also check for ESC[H ESC[2J (home + clear, common pattern)
        let mut i = 0;
        while i + 2 < data.len() {
            if data[i] == 0x1b && data[i + 1] == b'[' {
                // Found ESC[
                let mut j = i + 2;
                // Skip any digits or semicolons
                while j < data.len() && (data[j].is_ascii_digit() || data[j] == b';') {
                    j += 1;
                }
                // Check if it ends with 'J'
                if j < data.len() && data[j] == b'J' {
                    // Extract the number before J
                    let num_str = std::str::from_utf8(&data[i + 2..j]).unwrap_or("");
                    if num_str == "2" || num_str == "3" {
                        return true;
                    }
                }
                i = j;
            } else {
                i += 1;
            }
        }
        false
    }

    /// Check if any tab has a pending clear screen and reset scroll offset
    pub(super) fn check_clear_pending(&mut self) {
        for tab in &mut self.tabs {
            if let Some(session) = &tab.session {
                if let Ok(mut clear) = session.clear_pending.lock() {
                    if *clear {
                        // Reset scroll offset to show the cleared screen
                        tab.scroll_offset = 0;

                        // Note: We DON'T recreate the parser because that would lose terminal state
                        // like mouse mode settings from TUI apps. The clear sequence is already
                        // processed by the parser, and scrollback will naturally age out.

                        *clear = false;
                    }
                }
            }
        }
    }

    /// Reconnect a disconnected session in the current tab
    pub(super) fn reconnect_session(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let tab = &self.tabs[self.selected_tab];
        let host = tab.host.clone();

        log_debug!("Reconnecting session for host: {}", host.name);

        // Spawn a new SSH session
        let tab_title = tab.title.clone();
        match Self::spawn_ssh_session(&host, &tab_title, self.history_buffer) {
            Ok(session) => {
                let tab = &mut self.tabs[self.selected_tab];
                tab.session = Some(session);
                tab.scroll_offset = 0;
                log_debug!("Successfully reconnected to {}", host.name);
            }
            Err(e) => {
                log_error!("Failed to reconnect SSH session: {}", e);
            }
        }
    }

    /// Resize PTY for the current tab based on available area
    pub(super) fn resize_current_pty(&mut self, area: ratatui::layout::Rect) {
        if !self.tabs.is_empty()
            && self.selected_tab < self.tabs.len()
            && let Some(session) = &mut self.tabs[self.selected_tab].session
        {
            // Area is already the raw terminal content region for this tab.
            // Keep PTY size aligned 1:1 with the rendered content area.
            let rows = area.height.max(1);
            let cols = area.width.max(1);

            if let Ok(pty_master) = session.pty_master.lock() {
                let _ = pty_master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }

            // Also resize the VT100 parser
            if let Ok(mut parser) = session.parser.lock() {
                parser.set_size(rows, cols);
            }
        }
    }
}
