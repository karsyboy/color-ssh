//! SSH session spawning and PTY management

use super::osc52::forward_osc52;
use super::terminal_queries::respond_to_terminal_queries;
use crate::ssh_config::SshHost;
use crate::tui::{HostTab, SessionManager, SshSession, TerminalSearchCache, TerminalSearchState};
use crate::{debug_enabled, log_debug, log_error};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::collections::VecDeque;
use std::io::{self, Read};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;
use vt100::Parser;

const PARSER_REPLAY_BUFFER_MAX_BYTES: usize = 8 * 1024 * 1024;

fn append_replay_bytes(replay_log: &Arc<Mutex<VecDeque<u8>>>, data: &[u8]) {
    let Ok(mut replay) = replay_log.lock() else {
        return;
    };

    replay.extend(data.iter().copied());
    let overflow = replay.len().saturating_sub(PARSER_REPLAY_BUFFER_MAX_BYTES);
    if overflow > 0 {
        replay.drain(..overflow);
    }
}

fn reset_replay_bytes(replay_log: &Arc<Mutex<VecDeque<u8>>>, data: &[u8]) {
    let Ok(mut replay) = replay_log.lock() else {
        return;
    };

    replay.clear();
    replay.extend(data.iter().copied());
    let overflow = replay.len().saturating_sub(PARSER_REPLAY_BUFFER_MAX_BYTES);
    if overflow > 0 {
        replay.drain(..overflow);
    }
}

fn rebuild_parser_from_replay(session: &SshSession, rows: u16, cols: u16, history_buffer: usize) {
    let replay_snapshot = match session.replay_log.lock() {
        Ok(replay) => replay.iter().copied().collect::<Vec<u8>>(),
        Err(_) => Vec::new(),
    };

    if let Ok(mut parser) = session.parser.lock() {
        let mut rebuilt = Parser::new(rows, cols, history_buffer);
        if !replay_snapshot.is_empty() {
            rebuilt.process(&replay_snapshot);
        }
        *parser = rebuilt;
    }
}

pub(crate) fn encode_key_event_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    let mut bytes = match key.code {
        KeyCode::Char(ch) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let control_byte = match ch {
                    '@' | ' ' => 0,
                    'a'..='z' => (ch as u8) - b'a' + 1,
                    'A'..='Z' => (ch as u8) - b'A' + 1,
                    '[' => 27,
                    '\\' => 28,
                    ']' => 29,
                    '^' => 30,
                    '_' => 31,
                    '?' => 127,
                    _ => ch as u8,
                };
                vec![control_byte]
            } else {
                ch.to_string().into_bytes()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![127],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![27],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        _ => return None,
    };

    if key.modifiers.contains(KeyModifiers::ALT) {
        let mut meta_prefixed = Vec::with_capacity(bytes.len() + 1);
        meta_prefixed.push(0x1b);
        meta_prefixed.append(&mut bytes);
        return Some(meta_prefixed);
    }

    Some(bytes)
}

impl SessionManager {
    /// Select a host to open in a new tab
    pub(crate) fn select_host_to_connect(&mut self) {
        let Some(host_idx) = self.selected_host_idx() else {
            return;
        };
        let host = self.hosts[host_idx].clone();
        self.open_host_tab(host, false);
    }

    /// Open a quick-connect host in a new tab.
    pub(crate) fn open_quick_connect_host(&mut self, user: String, hostname: String, profile: Option<String>, force_ssh_logging: bool) {
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
            terminal_search_cache: TerminalSearchCache::default(),
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
        let replay_log = Arc::new(Mutex::new(VecDeque::new()));
        let replay_log_clone = replay_log.clone();

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

                        // Detect clear screen sequences and drop replay history before the clear.
                        // This prevents scrolling back into cleared content after parser rebuild.
                        if let Some(clear_start) = Self::clear_replay_slice_start(data) {
                            reset_replay_bytes(&replay_log_clone, &data[clear_start..]);
                            if let Ok(mut clear) = clear_pending_clone.lock() {
                                *clear = true;
                            }
                        } else {
                            // Keep a bounded replay log so terminal state can be rebuilt on resize.
                            append_replay_bytes(&replay_log_clone, data);
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
            replay_log,
            exited,
            clear_pending,
            render_epoch,
        })
    }

    /// Find the byte index to keep when replay is reset after a clear.
    ///
    /// Supported clear sequences:
    /// - `ESC[2J` clear screen
    /// - `ESC[3J` clear screen + scrollback
    ///
    /// The returned index starts at the earliest contiguous control sequence in
    /// the clear-chain (e.g., keeps `ESC[H ESC[2J ESC[3J` together) so parser
    /// rebuild preserves cursor placement.
    fn clear_replay_slice_start(data: &[u8]) -> Option<usize> {
        let mut last_clear_start: Option<usize> = None;
        let mut prev_end: Option<usize> = None;
        let mut prev_chain_member = false;
        let mut prev_chain_start = 0usize;

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
                if j < data.len() {
                    let final_byte = data[j];
                    let params = std::str::from_utf8(&data[i + 2..j]).unwrap_or("");
                    let is_clear = final_byte == b'J' && (params == "2" || params == "3");
                    let is_home = final_byte == b'H' || final_byte == b'f';
                    let is_chain_member = is_clear || is_home;

                    let chain_start = if is_chain_member && prev_chain_member && prev_end == Some(i) {
                        prev_chain_start
                    } else {
                        i
                    };

                    if is_clear {
                        last_clear_start = Some(chain_start);
                    }

                    prev_end = Some(j + 1);
                    prev_chain_member = is_chain_member;
                    prev_chain_start = chain_start;
                }
                i = j;
            } else {
                i += 1;
            }
        }
        last_clear_start
    }

    /// Check if any tab has a pending clear screen, then reset scroll state and parser history.
    pub(crate) fn check_clear_pending(&mut self) {
        for tab in &mut self.tabs {
            if let Some(session) = &tab.session
                && let Ok(mut clear) = session.clear_pending.lock()
                && *clear
            {
                // Reset scroll offset to show the cleared screen.
                tab.scroll_offset = 0;
                tab.terminal_search.matches.clear();
                tab.terminal_search.current = 0;
                tab.terminal_search_cache = TerminalSearchCache::default();

                // Rebuild from replay to enforce cleared history.
                let (rows, cols) = if let Ok(parser) = session.parser.lock() {
                    parser.screen().size()
                } else {
                    tab.last_pty_size.unwrap_or((40, 120))
                };
                rebuild_parser_from_replay(session, rows.max(1), cols.max(1), self.history_buffer);
                session.render_epoch.fetch_add(1, Ordering::Relaxed);

                *clear = false;
            }
        }
    }

    /// Reconnect a disconnected session in the current tab
    pub(crate) fn reconnect_session(&mut self) {
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
                tab.terminal_search.matches.clear();
                tab.terminal_search.current = 0;
                tab.terminal_search_cache = TerminalSearchCache::default();
                log_debug!("Successfully reconnected to {}", host.name);
            }
            Err(err) => {
                log_error!("Failed to reconnect SSH session: {}", err);
            }
        }
    }

    /// Resize PTY for the current tab based on available area
    pub(crate) fn resize_current_pty(&mut self, area: ratatui::layout::Rect) {
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

            // Rebuild parser from replay log to preserve text when width changes.
            rebuild_parser_from_replay(session, rows, cols, self.history_buffer);
            session.render_epoch.fetch_add(1, Ordering::Relaxed);

            tab.last_pty_size = Some((rows, cols));
            if debug_enabled!() {
                log_debug!("Resized PTY/parser to {}x{} in {:?}", cols, rows, resize_started_at.elapsed());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::encode_key_event_bytes;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn encode_key_event_bytes_ctrl_char() {
        let key = KeyEvent::new(KeyCode::Char('C'), KeyModifiers::CONTROL);
        assert_eq!(encode_key_event_bytes(key), Some(vec![3]));
    }

    #[test]
    fn encode_key_event_bytes_alt_char_prefixes_escape() {
        let key = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT);
        assert_eq!(encode_key_event_bytes(key), Some(vec![0x1b, b'f']));
    }

    #[test]
    fn encode_key_event_bytes_arrow() {
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(encode_key_event_bytes(key), Some(b"\x1b[A".to_vec()));
    }
}
