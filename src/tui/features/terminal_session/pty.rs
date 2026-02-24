//! SSH session spawning and PTY management.

use crate::auth::pass::{self, PassPromptStatus};
use crate::ssh_config::SshHost;
use crate::tui::terminal_emulator::Parser;
use crate::tui::{HostTab, PassPromptAction, SessionManager, SshSession, TerminalSearchState};
use crate::{debug_enabled, log_debug, log_error};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{self, Read};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

fn modifier_parameter(modifiers: KeyModifiers) -> u8 {
    let mut param = 1u8;
    if modifiers.contains(KeyModifiers::SHIFT) {
        param = param.saturating_add(1);
    }
    if modifiers.contains(KeyModifiers::ALT) {
        param = param.saturating_add(2);
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        param = param.saturating_add(4);
    }
    param
}

fn prefix_with_escape(mut bytes: Vec<u8>) -> Vec<u8> {
    let mut prefixed = Vec::with_capacity(bytes.len() + 1);
    prefixed.push(0x1b);
    prefixed.append(&mut bytes);
    prefixed
}

fn encode_csi_cursor_key(final_byte: u8, modifiers: KeyModifiers) -> Vec<u8> {
    let base = vec![0x1b, b'[', final_byte];
    if modifiers.is_empty() {
        return base;
    }
    if modifiers == KeyModifiers::ALT {
        return prefix_with_escape(base);
    }

    let final_char = final_byte as char;
    format!("\x1b[1;{}{}", modifier_parameter(modifiers), final_char).into_bytes()
}

fn encode_csi_tilde_key(code: u8, modifiers: KeyModifiers) -> Vec<u8> {
    if modifiers.is_empty() {
        return format!("\x1b[{}~", code).into_bytes();
    }
    if modifiers == KeyModifiers::ALT {
        return prefix_with_escape(format!("\x1b[{}~", code).into_bytes());
    }

    format!("\x1b[{};{}~", code, modifier_parameter(modifiers)).into_bytes()
}

pub(crate) fn encode_key_event_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    let modifiers = key.modifiers & (KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL);

    let bytes = match key.code {
        KeyCode::Char(ch) => {
            let mut out = if modifiers.contains(KeyModifiers::CONTROL) {
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
            };

            if modifiers.contains(KeyModifiers::ALT) {
                out = prefix_with_escape(out);
            }
            out
        }
        KeyCode::Enter => {
            let out = vec![b'\r'];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Backspace => {
            let out = vec![127];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Tab => {
            let out = vec![b'\t'];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Esc => {
            let out = vec![27];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Up => encode_csi_cursor_key(b'A', modifiers),
        KeyCode::Down => encode_csi_cursor_key(b'B', modifiers),
        KeyCode::Right => encode_csi_cursor_key(b'C', modifiers),
        KeyCode::Left => encode_csi_cursor_key(b'D', modifiers),
        KeyCode::Home => encode_csi_cursor_key(b'H', modifiers),
        KeyCode::End => encode_csi_cursor_key(b'F', modifiers),
        KeyCode::PageUp => encode_csi_tilde_key(5, modifiers),
        KeyCode::PageDown => encode_csi_tilde_key(6, modifiers),
        KeyCode::Delete => encode_csi_tilde_key(3, modifiers),
        KeyCode::Insert => encode_csi_tilde_key(2, modifiers),
        _ => return None,
    };

    Some(bytes)
}

impl SessionManager {
    // PTY sizing / config helpers.
    fn initial_pty_size(&self) -> (u16, u16) {
        let rows = self.tab_content_area.height.max(1);
        let cols = self.tab_content_area.width.max(1);
        (rows, cols)
    }

    fn resolved_history_buffer_for_host(&self, host: &SshHost) -> usize {
        crate::config::history_buffer_for_profile(host.profile.as_deref()).unwrap_or(self.history_buffer)
    }

    fn resolve_host_pass_password(&mut self, host: &SshHost, action: PassPromptAction) -> Option<(Option<String>, Option<String>)> {
        let Some(pass_key) = host.pass_key.as_deref() else {
            return Some((None, None));
        };

        match pass::resolve_pass_key_for_tui(pass_key, &mut self.pass_cache) {
            PassPromptStatus::Ready(password) => Some((Some(password), None)),
            PassPromptStatus::PromptRequired => {
                self.open_pass_prompt(pass_key.to_string(), action);
                None
            }
            PassPromptStatus::Fallback(reason) => {
                log_debug!("Pass auto-login unavailable for host {}: {:?}", host.name, reason);
                Some((None, Some(pass::fallback_notice(reason))))
            }
        }
    }

    // Host selection -> tab opening.
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

    // Tab/session creation.
    fn open_host_tab(&mut self, host: SshHost, force_ssh_logging: bool) {
        log_debug!("Opening tab for host: {}", host.name);
        let action = PassPromptAction::OpenHostTab {
            host: host.clone(),
            force_ssh_logging,
        };
        let Some((pass_password, pass_fallback_notice)) = self.resolve_host_pass_password(&host, action) else {
            return;
        };
        self.open_host_tab_with_auth(host, force_ssh_logging, pass_password, pass_fallback_notice);
    }

    fn open_host_tab_with_auth(&mut self, host: SshHost, force_ssh_logging: bool, pass_password: Option<String>, pass_fallback_notice: Option<String>) {
        // Generate unique tab title with suffix for duplicate hosts
        let existing_count = self.tabs.iter().filter(|tab| tab.host.name == host.name).count();
        let tab_title = if existing_count == 0 {
            host.name.clone()
        } else {
            format!("{}_{}", host.name, existing_count)
        };
        let history_buffer = self.resolved_history_buffer_for_host(&host);
        log_debug!("Using history buffer {} for tab '{}' (profile: {:?})", history_buffer, tab_title, host.profile);
        let (initial_rows, initial_cols) = self.initial_pty_size();

        // Spawn SSH session
        let session = match Self::spawn_ssh_session(
            &host,
            &tab_title,
            history_buffer,
            force_ssh_logging,
            initial_rows,
            initial_cols,
            pass_password,
            pass_fallback_notice,
        ) {
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

    pub(crate) fn complete_pass_prompt_action(&mut self, action: PassPromptAction, pass_password: Option<String>, pass_fallback_notice: Option<String>) {
        match action {
            PassPromptAction::OpenHostTab { host, force_ssh_logging } => {
                self.open_host_tab_with_auth(host, force_ssh_logging, pass_password, pass_fallback_notice);
            }
            PassPromptAction::ReconnectTab { tab_index } => {
                self.reconnect_session_with_auth(tab_index, pass_password, pass_fallback_notice);
            }
        }
    }

    // Spawn and wire a PTY-backed cossh process.
    /// Spawn an SSH session in a PTY
    fn spawn_ssh_session(
        host: &SshHost,
        tab_title: &str,
        history_buffer: usize,
        force_ssh_logging: bool,
        initial_rows: u16,
        initial_cols: u16,
        pass_password: Option<String>,
        pass_fallback_notice: Option<String>,
    ) -> io::Result<SshSession> {
        let pty_system = native_pty_system();
        let rows = initial_rows.max(1);
        let cols = initial_cols.max(1);

        // Create a new PTY with current content area size to avoid startup
        // geometry mismatches in full-screen terminal apps.
        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| io::Error::other(err.to_string()))?;

        // Build cossh command to get syntax highlighting
        let cossh_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("cossh"));

        let mut cmd = if pass_password.is_some() {
            let mut pass_cmd = CommandBuilder::new("sshpass");
            pass_cmd.arg("-e");
            pass_cmd.arg(&cossh_path);
            pass_cmd
        } else {
            CommandBuilder::new(&cossh_path)
        };

        if force_ssh_logging {
            cmd.arg("-l");
        }

        cmd.arg(&host.name);
        cmd.env("COSSH_SESSION_NAME", tab_title);
        cmd.env("COSSH_SKIP_PASS_RESOLVE", "1");
        if let Some(password) = pass_password.as_ref() {
            cmd.env("SSHPASS", password);
        }

        // Pass profile if specified in .ssh/config via #_Profile
        if let Some(profile) = &host.profile {
            cmd.arg("-P");
            cmd.arg(profile);
        }

        let pass_info = if pass_password.is_some() { " (via pass)" } else { "" };
        let profile_info = host.profile.as_ref().map_or(String::new(), |profile| format!(" [profile: {}]", profile));
        let logging_info = if force_ssh_logging { " [ssh-logging]" } else { "" };
        log_debug!(
            "Spawning cossh command: cossh {}{}{}{} (session: {})",
            host.name,
            pass_info,
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
        let parser = Arc::new(Mutex::new(Parser::new_with_pty_writer(rows, cols, history_buffer, writer.clone())));
        let parser_clone = parser.clone();
        let exited = Arc::new(Mutex::new(false));
        let exited_clone = exited.clone();
        let pty_master = Arc::new(Mutex::new(pty_pair.master));
        let render_epoch = Arc::new(AtomicU64::new(0));
        let render_epoch_clone = render_epoch.clone();

        if let Some(notice) = pass_fallback_notice
            && let Ok(mut parser) = parser.lock()
        {
            let message = format!("\r\n[color-ssh] {}\r\n", notice);
            parser.process(message.as_bytes());
            render_epoch.fetch_add(1, Ordering::Relaxed);
        }

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

    // Session recovery.
    /// Reconnect a disconnected session in the current tab
    pub(crate) fn reconnect_session(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let tab_index = self.selected_tab;
        let host = self.tabs[tab_index].host.clone();

        log_debug!("Reconnecting session for host: {}", host.name);

        let action = PassPromptAction::ReconnectTab { tab_index };
        let Some((pass_password, pass_fallback_notice)) = self.resolve_host_pass_password(&host, action) else {
            return;
        };
        self.reconnect_session_with_auth(tab_index, pass_password, pass_fallback_notice);
    }

    fn reconnect_session_with_auth(&mut self, tab_index: usize, pass_password: Option<String>, pass_fallback_notice: Option<String>) {
        if self.tabs.is_empty() || tab_index >= self.tabs.len() {
            return;
        }

        let tab = &self.tabs[tab_index];
        let host = tab.host.clone();
        let tab_title = tab.title.clone();
        let force_ssh_logging = tab.force_ssh_logging;
        let history_buffer = self.resolved_history_buffer_for_host(&host);
        let (initial_rows, initial_cols) = tab.last_pty_size.unwrap_or_else(|| self.initial_pty_size());

        log_debug!(
            "Using history buffer {} for reconnect tab '{}' (profile: {:?})",
            history_buffer,
            tab_title,
            host.profile
        );
        match Self::spawn_ssh_session(
            &host,
            &tab_title,
            history_buffer,
            force_ssh_logging,
            initial_rows,
            initial_cols,
            pass_password,
            pass_fallback_notice,
        ) {
            Ok(session) => {
                let tab = &mut self.tabs[tab_index];
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

    // Live PTY resize from current viewport.
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
#[path = "../../../test/tui/features/terminal_session/pty.rs"]
mod tests;
