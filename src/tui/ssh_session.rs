//! SSH session spawning and PTY management

use super::{HostTab, SessionManager, SshSession, TerminalSearchState};
use crate::ssh_config::SshHost;
use crate::{debug_enabled, log_debug, log_error};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{self, Read, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;
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

/// Scan PTY output for OSC 52 clipboard sequences and forward them to stdout.
///
/// OSC 52 format: `ESC ] 52 ; <selection> ; <base64-data> BEL` or `ESC ] 52 ; <selection> ; <base64-data> ESC \`
/// `osc_buf` accumulates partial sequences across read boundaries.
fn forward_osc52(data: &[u8], osc_buf: &mut Vec<u8>) {
    let forwarded_sequences = collect_osc52_sequences(data, osc_buf);
    if forwarded_sequences.is_empty() {
        return;
    }

    let mut stdout = io::stdout();
    for sequence in forwarded_sequences {
        let _ = stdout.write_all(&sequence);
    }
    let _ = stdout.flush();
}

fn collect_osc52_sequences(data: &[u8], osc_buf: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut forwarded = Vec::new();
    let mut owned_input: Option<Vec<u8>> = None;
    let mut input = owned_input.as_deref().unwrap_or(data);

    loop {
        if !osc_buf.is_empty() {
            osc_buf.extend_from_slice(input);
            if let Some(end) = find_osc_end(osc_buf) {
                forwarded.push(osc_buf[..end].to_vec());
                owned_input = Some(osc_buf[end..].to_vec());
                osc_buf.clear();
                if owned_input.as_ref().is_some_and(Vec::is_empty) {
                    return forwarded;
                }
                input = owned_input.as_deref().unwrap_or(&[]);
                continue;
            }

            if osc_buf.len() > 100_000 {
                osc_buf.clear();
            }
            return forwarded;
        }

        let mut scan_idx = 0;
        while scan_idx < input.len() {
            if input[scan_idx] == 0x1b && scan_idx + 1 < input.len() && input[scan_idx + 1] == b']' {
                let rest = &input[scan_idx + 2..];
                if rest.starts_with(b"52;") {
                    let seq_start = scan_idx;
                    if let Some(end_offset) = find_osc_end(&input[seq_start..]) {
                        forwarded.push(input[seq_start..seq_start + end_offset].to_vec());
                        scan_idx = seq_start + end_offset;
                        continue;
                    }

                    osc_buf.extend_from_slice(&input[seq_start..]);
                    return forwarded;
                }
            }
            scan_idx += 1;
        }

        return forwarded;
    }
}

/// Find the end of an OSC sequence (after BEL or ST terminator), returning the byte position after the terminator.
fn find_osc_end(data: &[u8]) -> Option<usize> {
    for byte_idx in 0..data.len() {
        if data[byte_idx] == 0x07 {
            // BEL terminator
            return Some(byte_idx + 1);
        }
        if data[byte_idx] == 0x1b && byte_idx + 1 < data.len() && data[byte_idx + 1] == b'\\' {
            // ST terminator (ESC \)
            return Some(byte_idx + 2);
        }
    }
    None
}

impl SessionManager {
    /// Select a host to open in a new tab
    pub(super) fn select_host_to_connect(&mut self) {
        let Some(host_idx) = self.selected_host_idx() else {
            return;
        };
        let host = self.hosts[host_idx].clone();
        self.open_host_tab(host, false);
    }

    /// Open a quick-connect host in a new tab.
    pub(super) fn open_quick_connect_host(&mut self, user: String, hostname: String, profile: Option<String>, force_ssh_logging: bool) {
        let user = user.trim().to_string();
        let target = if user.is_empty() {
            hostname.clone()
        } else {
            format!("{}@{}", user, hostname)
        };
        let mut host = SshHost::new(target);
        host.user = if user.is_empty() { None } else { Some(user) };
        host.hostname = Some(hostname);
        host.profile = profile;
        self.open_host_tab(host, force_ssh_logging);
    }

    fn open_host_tab(&mut self, host: SshHost, force_ssh_logging: bool) {
        log_debug!("Opening tab for host: {}", host.name);

        // Generate unique tab title with suffix for duplicate hosts
        let existing_count = self.tabs.iter().filter(|tab| tab.host.name == host.name).count();
        let tab_title = if existing_count == 0 {
            host.name.clone()
        } else {
            format!("{}_{}", host.name, existing_count)
        };

        // Spawn SSH session
        let session = match Self::spawn_ssh_session(&host, &tab_title, self.history_buffer, force_ssh_logging) {
            Ok(session) => Some(session),
            Err(err) => {
                log_error!("Failed to spawn SSH session: {}", err);
                None
            }
        };

        // Create new tab
        let tab = HostTab {
            title: tab_title,
            host: host.clone(),
            session,
            scroll_offset: 0,
            terminal_search: TerminalSearchState::default(),
            force_ssh_logging,
            last_pty_size: None,
        };

        self.tabs.push(tab);
        self.selected_tab = self.tabs.len() - 1;
        self.focus_on_manager = false;
        // Opening a host into a terminal should leave host search-edit mode.
        self.search_mode = false;
        // Close quick-connect modal if it was open.
        self.quick_connect = None;

        log_debug!("Created new tab at index {}", self.selected_tab);
    }

    /// Spawn an SSH session in a PTY
    fn spawn_ssh_session(host: &SshHost, tab_title: &str, history_buffer: usize, force_ssh_logging: bool) -> io::Result<SshSession> {
        let pty_system = native_pty_system();

        // Create a new PTY with initial size (will be resized later)
        let pty_pair = pty_system
            .openpty(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| io::Error::other(err.to_string()))?;

        // Build cossh command to get syntax highlighting
        let cossh_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("cossh"));

        let mut cmd = if host.use_sshpass {
            // Use sshpass -e to pass the password from SSHPASS env var
            let mut sshpass_cmd = CommandBuilder::new("sshpass");
            sshpass_cmd.arg("-e");
            sshpass_cmd.arg(&cossh_path);
            sshpass_cmd
        } else {
            CommandBuilder::new(&cossh_path)
        };

        if force_ssh_logging {
            cmd.arg("-l");
        }

        cmd.arg(&host.name);
        cmd.env("COSSH_SESSION_NAME", tab_title);

        // Pass profile if specified in .ssh/config via #_Profile
        if let Some(profile) = &host.profile {
            cmd.arg("-P");
            cmd.arg(profile);
        }

        let sshpass_info = if host.use_sshpass { " (via sshpass)" } else { "" };
        let profile_info = host.profile.as_ref().map_or(String::new(), |profile| format!(" [profile: {}]", profile));
        let logging_info = if force_ssh_logging { " [ssh-logging]" } else { "" };
        log_debug!(
            "Spawning cossh command: cossh {}{}{}{} (session: {})",
            host.name,
            sshpass_info,
            profile_info,
            logging_info,
            tab_title
        );

        // Spawn the command in the PTY
        let child = pty_pair.slave.spawn_command(cmd).map_err(|err| io::Error::other(err.to_string()))?;

        // Get the master for reading/writing
        let mut reader = pty_pair.master.try_clone_reader().map_err(|err| io::Error::other(err.to_string()))?;
        let writer = pty_pair.master.take_writer().map_err(|err| io::Error::other(err.to_string()))?;

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
        let render_epoch = Arc::new(AtomicU64::new(0));
        let render_epoch_clone = render_epoch.clone();

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
                    Ok(bytes_read) => {
                        let data = &buf[..bytes_read];

                        // Respond to terminal capability queries from PTY
                        // (fixes fish shell DA query timeout warning)
                        respond_to_terminal_queries(data, &writer_clone);

                        // Forward any OSC 52 clipboard sequences to the real terminal
                        // so the inner TUI app's copy reaches the system clipboard.
                        forward_osc52(data, &mut osc_buf);

                        // Detect clear screen sequences: \x1b[2J or \x1b[3J
                        if Self::contains_clear_sequence(data)
                            && let Ok(mut clear) = clear_pending_clone.lock()
                        {
                            *clear = true;
                        }

                        if let Ok(mut parser) = parser_clone.lock() {
                            // Process the data through VT100 parser
                            parser.process(data);
                            render_epoch_clone.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Err(err) => {
                        log_error!("Error reading from PTY: {}", err);
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
            render_epoch,
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
            if let Some(session) = &tab.session
                && let Ok(mut clear) = session.clear_pending.lock()
                && *clear
            {
                // Reset scroll offset to show the cleared screen
                tab.scroll_offset = 0;

                // Note: We DON'T recreate the parser because that would lose terminal state
                // like mouse mode settings from TUI apps. The clear sequence is already
                // processed by the parser, and scrollback will naturally age out.

                *clear = false;
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
        let force_ssh_logging = self.tabs[self.selected_tab].force_ssh_logging;
        match Self::spawn_ssh_session(&host, &tab_title, self.history_buffer, force_ssh_logging) {
            Ok(session) => {
                let tab = &mut self.tabs[self.selected_tab];
                tab.session = Some(session);
                tab.scroll_offset = 0;
                log_debug!("Successfully reconnected to {}", host.name);
            }
            Err(err) => {
                log_error!("Failed to reconnect SSH session: {}", err);
            }
        }
    }

    /// Resize PTY for the current tab based on available area
    pub(super) fn resize_current_pty(&mut self, area: ratatui::layout::Rect) {
        if !self.tabs.is_empty()
            && self.selected_tab < self.tabs.len()
            && let Some(tab) = self.tabs.get_mut(self.selected_tab)
            && let Some(session) = &mut tab.session
        {
            // Area is already the raw terminal content region for this tab.
            // Keep PTY size aligned 1:1 with the rendered content area.
            let rows = area.height.max(1);
            let cols = area.width.max(1);
            if tab.last_pty_size == Some((rows, cols)) {
                return;
            }
            let resize_started_at = Instant::now();

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

            tab.last_pty_size = Some((rows, cols));
            if debug_enabled!() {
                log_debug!("Resized PTY/parser to {}x{} in {:?}", cols, rows, resize_started_at.elapsed());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{collect_osc52_sequences, find_osc_end};

    #[test]
    fn find_osc_end_supports_bel_and_st() {
        assert_eq!(find_osc_end(b"\x1b]52;c;abc\x07"), Some(11));
        assert_eq!(find_osc_end(b"\x1b]52;c;abc\x1b\\"), Some(12));
    }

    #[test]
    fn collect_osc52_sequences_handles_chunked_input_iteratively() {
        let mut osc_buf = Vec::new();
        let first = collect_osc52_sequences(b"\x1b]52;c;Zm9v", &mut osc_buf);
        assert!(first.is_empty());
        assert!(!osc_buf.is_empty());

        let second = collect_osc52_sequences(b"YmFy\x07after", &mut osc_buf);
        assert_eq!(second.len(), 1);
        assert_eq!(second[0], b"\x1b]52;c;Zm9vYmFy\x07".to_vec());
        assert!(osc_buf.is_empty());
    }
}
