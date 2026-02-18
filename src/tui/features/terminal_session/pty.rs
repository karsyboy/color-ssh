//! SSH session spawning and PTY management

use crate::ssh_config::SshHost;
use crate::tui::terminal_emulator::Parser;
use crate::tui::{HostTab, SessionManager, SshSession, TerminalSearchState};
use crate::{debug_enabled, log_debug, log_error};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{self, Read};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

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
    fn resolved_history_buffer_for_host(&self, host: &SshHost) -> usize {
        crate::config::history_buffer_for_profile(host.profile.as_deref()).unwrap_or(self.history_buffer)
    }

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
        let history_buffer = self.resolved_history_buffer_for_host(&host);
        log_debug!("Using history buffer {} for tab '{}' (profile: {:?})", history_buffer, tab_title, host.profile);

        // Spawn SSH session
        let session = match Self::spawn_ssh_session(&host, &tab_title, history_buffer, force_ssh_logging) {
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
        let writer = Arc::new(Mutex::new(writer));

        // Create terminal parser/emulator and hook alacritty event callbacks to the PTY writer.
        let parser = Arc::new(Mutex::new(Parser::new_with_pty_writer(40, 120, history_buffer, writer.clone())));
        let parser_clone = parser.clone();
        let exited = Arc::new(Mutex::new(false));
        let exited_clone = exited.clone();
        let pty_master = Arc::new(Mutex::new(pty_pair.master));
        let render_epoch = Arc::new(AtomicU64::new(0));
        let render_epoch_clone = render_epoch.clone();

        // Spawn a thread to read from PTY
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
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

                        if let Ok(mut parser) = parser_clone.lock() {
                            // Process PTY bytes through terminal emulator.
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
            render_epoch,
        })
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
        let history_buffer = self.resolved_history_buffer_for_host(&host);
        log_debug!(
            "Using history buffer {} for reconnect tab '{}' (profile: {:?})",
            history_buffer,
            tab_title,
            host.profile
        );
        match Self::spawn_ssh_session(&host, &tab_title, history_buffer, force_ssh_logging) {
            Ok(session) => {
                let tab = &mut self.tabs[self.selected_tab];
                tab.session = Some(session);
                tab.scroll_offset = 0;
                tab.terminal_search.matches.clear();
                tab.terminal_search.current = 0;
                tab.last_pty_size = None;
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

            // Let the terminal emulator handle resize semantics directly.
            if let Ok(mut parser) = session.parser.lock() {
                parser.set_size(rows, cols);
            }
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
    fn encode_key_event_bytes_ctrl_bracket_variants() {
        let open = KeyEvent::new(KeyCode::Char('['), KeyModifiers::CONTROL);
        let backslash = KeyEvent::new(KeyCode::Char('\\'), KeyModifiers::CONTROL);
        let close = KeyEvent::new(KeyCode::Char(']'), KeyModifiers::CONTROL);
        let at = KeyEvent::new(KeyCode::Char('@'), KeyModifiers::CONTROL);

        assert_eq!(encode_key_event_bytes(open), Some(vec![27]));
        assert_eq!(encode_key_event_bytes(backslash), Some(vec![28]));
        assert_eq!(encode_key_event_bytes(close), Some(vec![29]));
        assert_eq!(encode_key_event_bytes(at), Some(vec![0]));
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

    #[test]
    fn encode_key_event_bytes_alt_arrow_prefixes_escape() {
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::ALT);
        assert_eq!(encode_key_event_bytes(key), Some(b"\x1b\x1b[A".to_vec()));
    }
}
