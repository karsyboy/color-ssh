//! Interactive session spawning and PTY management.

use crate::auth::agent;
use crate::process;
use crate::ssh_config::{ConnectionProtocol, SshHost};
use crate::tui::terminal_emulator::Parser;
use crate::tui::{AppState, HostTab, ManagedChild, ManagedSession, TerminalSearchState, VaultUnlockAction};
use crate::{command_path, debug_enabled, log_debug, log_error};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{self, Read};
use std::process::{Child as ProcessChild, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

struct SessionLaunchOptions {
    force_ssh_logging: bool,
    initial_rows: u16,
    initial_cols: u16,
    pass_entry_override: Option<String>,
    pass_fallback_notice: Option<String>,
}

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

fn auto_login_notice(host: &SshHost, detail: impl Into<String>) -> String {
    let detail = detail.into();
    match host.protocol {
        ConnectionProtocol::Ssh => format!("{detail}; continuing with the standard SSH password prompt."),
        ConnectionProtocol::Rdp => format!("{detail}; RDP launch requires vault-backed credentials."),
    }
}

fn normalize_managed_output_newlines(bytes: &[u8], previous_ended_with_cr: &mut bool) -> Vec<u8> {
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

fn spawn_output_reader<R>(name: &'static str, mut reader: R, parser: Arc<Mutex<Parser>>, render_epoch: Arc<AtomicU64>)
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
                    if let Ok(mut parser) = parser.lock() {
                        parser.process(&normalized);
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

fn spawn_process_exit_watcher(child: Arc<Mutex<ProcessChild>>, exited: Arc<Mutex<bool>>) {
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

impl AppState {
    // PTY sizing / config helpers.
    fn initial_pty_size(&self) -> (u16, u16) {
        let rows = self.tab_content_area.height.max(1);
        let cols = self.tab_content_area.width.max(1);
        (rows, cols)
    }

    fn resolved_history_buffer_for_host(&self, host: &SshHost) -> usize {
        crate::config::history_buffer_for_profile(host.profile.as_deref()).unwrap_or(self.history_buffer)
    }

    fn resolve_host_pass_password(&mut self, host: &SshHost, action: VaultUnlockAction) -> Option<(Option<String>, Option<String>)> {
        let auth_settings = crate::config::auth_settings();
        if !auth_settings.tui_password_autologin {
            let notice = match host.protocol {
                ConnectionProtocol::Ssh => None,
                ConnectionProtocol::Rdp => {
                    Some("TUI password auto-login is disabled in auth settings; enable it to launch RDP sessions from the session manager.".to_string())
                }
            };
            return Some((None, notice));
        }

        let Some(pass_key) = host.pass_key.as_deref() else {
            let notice = match host.protocol {
                ConnectionProtocol::Ssh => None,
                ConnectionProtocol::Rdp => Some("RDP launch requires a password vault entry; add `#_pass <name>` to the host config.".to_string()),
            };
            return Some((None, notice));
        };

        let client = match agent::AgentClient::new() {
            Ok(client) => client,
            Err(err) => {
                return Some((
                    None,
                    Some(auto_login_notice(
                        host,
                        format!("Password auto-login is unavailable because the password vault agent could not be started ({err})"),
                    )),
                ));
            }
        };

        match client.entry_status(pass_key) {
            Ok(entry_status) => {
                let exists = entry_status.exists;
                let unlocked = entry_status.status.unlocked;
                self.set_vault_status(entry_status.status);
                if exists && unlocked {
                    return Some((Some(pass_key.to_string()), None));
                }
                if !exists {
                    return Some((
                        None,
                        Some(auto_login_notice(
                            host,
                            format!("Password auto-login is unavailable because vault entry '{}' was not found", pass_key),
                        )),
                    ));
                }
                self.open_vault_unlock(pass_key.to_string(), action);
                None
            }
            Err(agent::AgentError::VaultNotInitialized) => Some((
                None,
                Some(match host.protocol {
                    ConnectionProtocol::Ssh => "Password vault is not initialized. Run `cossh vault init` or `cossh vault add <name>` first.".to_string(),
                    ConnectionProtocol::Rdp => {
                        "RDP launch requires an initialized password vault. Run `cossh vault init` or `cossh vault add <name>` first.".to_string()
                    }
                }),
            )),
            Err(err) => {
                log_debug!("Password auto-login unavailable for host {}: {}", host.name, err);
                Some((
                    None,
                    Some(auto_login_notice(
                        host,
                        format!("Password auto-login is unavailable because the password vault could not be queried ({err})"),
                    )),
                ))
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
        log_debug!("Opening {:?} tab for host: {}", host.protocol, host.name);
        let action = VaultUnlockAction::OpenHostTab {
            host: Box::new(host.clone()),
            force_ssh_logging,
        };
        let Some((pass_entry_override, pass_fallback_notice)) = self.resolve_host_pass_password(&host, action) else {
            return;
        };
        self.open_host_tab_with_auth(host, force_ssh_logging, pass_entry_override, pass_fallback_notice);
    }

    fn open_host_tab_with_auth(&mut self, host: SshHost, force_ssh_logging: bool, pass_entry_override: Option<String>, pass_fallback_notice: Option<String>) {
        let existing_count = self.tabs.iter().filter(|tab| tab.host.name == host.name).count();
        let tab_title = if existing_count == 0 {
            host.name.clone()
        } else {
            format!("{}_{}", host.name, existing_count)
        };
        let history_buffer = self.resolved_history_buffer_for_host(&host);
        log_debug!("Using history buffer {} for tab '{}' (profile: {:?})", history_buffer, tab_title, host.profile);
        let (initial_rows, initial_cols) = self.initial_pty_size();

        let session_launch_options = SessionLaunchOptions {
            force_ssh_logging,
            initial_rows,
            initial_cols,
            pass_entry_override,
            pass_fallback_notice,
        };

        let (session, session_error) = match Self::spawn_session(&host, &tab_title, history_buffer, session_launch_options) {
            Ok(session) => (Some(session), None),
            Err(err) => {
                let err_message = err.to_string();
                log_error!("Failed to spawn {} session: {}", host.protocol.display_name(), err_message);
                (None, Some(err_message))
            }
        };

        let tab = HostTab {
            title: tab_title,
            host: host.clone(),
            session,
            session_error,
            scroll_offset: 0,
            terminal_search: TerminalSearchState::default(),
            force_ssh_logging,
            last_pty_size: None,
        };

        self.tabs.push(tab);
        self.selected_tab = self.tabs.len() - 1;
        self.focus_on_manager = false;
        self.search_mode = false;
        self.quick_connect = None;

        log_debug!("Created new tab at index {}", self.selected_tab);
    }

    pub(crate) fn complete_vault_unlock_action(
        &mut self,
        action: VaultUnlockAction,
        pass_entry_override: Option<String>,
        pass_fallback_notice: Option<String>,
    ) {
        match action {
            VaultUnlockAction::UnlockVault => {}
            VaultUnlockAction::OpenHostTab { host, force_ssh_logging } => {
                self.open_host_tab_with_auth(*host, force_ssh_logging, pass_entry_override, pass_fallback_notice);
            }
            VaultUnlockAction::ReconnectTab { tab_index } => {
                self.reconnect_session_with_auth(tab_index, pass_entry_override, pass_fallback_notice);
            }
        }
    }

    fn spawn_session(host: &SshHost, tab_title: &str, history_buffer: usize, launch_options: SessionLaunchOptions) -> io::Result<ManagedSession> {
        match host.protocol {
            ConnectionProtocol::Ssh => Self::spawn_ssh_session(host, tab_title, history_buffer, launch_options),
            ConnectionProtocol::Rdp => Self::spawn_rdp_session(host, tab_title, history_buffer, launch_options),
        }
    }

    // Spawn and wire a PTY-backed cossh process.
    fn spawn_ssh_session(host: &SshHost, tab_title: &str, history_buffer: usize, launch_options: SessionLaunchOptions) -> io::Result<ManagedSession> {
        let pty_system = native_pty_system();
        let SessionLaunchOptions {
            force_ssh_logging,
            initial_rows,
            initial_cols,
            pass_entry_override,
            pass_fallback_notice,
        } = launch_options;
        let rows = initial_rows.max(1);
        let cols = initial_cols.max(1);

        let pty_pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| io::Error::other(err.to_string()))?;

        let cossh_path = command_path::cossh_path()?;
        let using_pass_entry = pass_entry_override.is_some();
        let mut cmd = CommandBuilder::new(&cossh_path);

        if force_ssh_logging {
            cmd.arg("-l");
        }

        if let Some(pass_entry_override) = &pass_entry_override {
            cmd.arg("--pass-entry");
            cmd.arg(pass_entry_override);
        }

        if let Some(profile) = &host.profile {
            cmd.arg("-P");
            cmd.arg(profile);
        }

        cmd.arg("ssh");
        cmd.arg(&host.name);
        cmd.env("COSSH_SESSION_NAME", tab_title);

        let pass_info = if using_pass_entry { " (via vault)" } else { "" };
        let profile_info = host.profile.as_ref().map_or(String::new(), |profile| format!(" [profile: {}]", profile));
        let logging_info = if force_ssh_logging { " [ssh-logging]" } else { "" };
        log_debug!(
            "Spawning cossh command: cossh ssh {}{}{}{} (session: {})",
            host.name,
            pass_info,
            profile_info,
            logging_info,
            tab_title
        );

        let child = Arc::new(Mutex::new(pty_pair.slave.spawn_command(cmd).map_err(|err| io::Error::other(err.to_string()))?));

        let mut reader = pty_pair.master.try_clone_reader().map_err(|err| io::Error::other(err.to_string()))?;
        let writer = pty_pair.master.take_writer().map_err(|err| io::Error::other(err.to_string()))?;
        let writer = Arc::new(Mutex::new(writer));

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

        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        if let Ok(mut exited) = exited_clone.lock() {
                            *exited = true;
                        }
                        break;
                    }
                    Ok(bytes_read) => {
                        if let Ok(mut parser) = parser_clone.lock() {
                            parser.process(&buf[..bytes_read]);
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

        Ok(ManagedSession {
            pty_master: Some(pty_master),
            writer: Some(writer),
            child: ManagedChild::Pty(child),
            parser,
            exited,
            render_epoch,
        })
    }

    fn spawn_rdp_session(host: &SshHost, _tab_title: &str, history_buffer: usize, launch_options: SessionLaunchOptions) -> io::Result<ManagedSession> {
        let SessionLaunchOptions {
            initial_rows,
            initial_cols,
            pass_entry_override,
            pass_fallback_notice,
            ..
        } = launch_options;
        if let Some(notice) = pass_fallback_notice {
            return Err(io::Error::other(notice));
        }

        let mut child = process::spawn_command(
            process::build_rdp_command_for_host(host, pass_entry_override.as_deref())?,
            Stdio::piped(),
            Stdio::piped(),
        )?;
        let stdout = child.stdout.take().ok_or_else(|| io::Error::other("failed to capture FreeRDP stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| io::Error::other("failed to capture FreeRDP stderr"))?;

        let rows = initial_rows.max(1);
        let cols = initial_cols.max(1);
        let parser = Arc::new(Mutex::new(Parser::new(rows, cols, history_buffer)));
        let exited = Arc::new(Mutex::new(false));
        let render_epoch = Arc::new(AtomicU64::new(0));
        let child = Arc::new(Mutex::new(child));

        spawn_output_reader("freerdp stdout", stdout, parser.clone(), render_epoch.clone());
        spawn_output_reader("freerdp stderr", stderr, parser.clone(), render_epoch.clone());
        spawn_process_exit_watcher(child.clone(), exited.clone());

        Ok(ManagedSession {
            pty_master: None,
            writer: None,
            child: ManagedChild::Process(child),
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

        let action = VaultUnlockAction::ReconnectTab { tab_index };
        let Some((pass_entry_override, pass_fallback_notice)) = self.resolve_host_pass_password(&host, action) else {
            return;
        };
        self.reconnect_session_with_auth(tab_index, pass_entry_override, pass_fallback_notice);
    }

    fn reconnect_session_with_auth(&mut self, tab_index: usize, pass_entry_override: Option<String>, pass_fallback_notice: Option<String>) {
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
        let session_launch_options = SessionLaunchOptions {
            force_ssh_logging,
            initial_rows,
            initial_cols,
            pass_entry_override,
            pass_fallback_notice,
        };

        match Self::spawn_session(&host, &tab_title, history_buffer, session_launch_options) {
            Ok(session) => {
                let tab = &mut self.tabs[tab_index];
                tab.session = Some(session);
                tab.session_error = None;
                tab.scroll_offset = 0;
                tab.terminal_search.matches.clear();
                tab.terminal_search.current = 0;
                tab.last_pty_size = None;
                log_debug!("Successfully reconnected to {}", host.name);
            }
            Err(err) => {
                let err_message = err.to_string();
                log_error!("Failed to reconnect {} session: {}", host.protocol.display_name(), err_message);
                let tab = &mut self.tabs[tab_index];
                tab.session = None;
                tab.session_error = Some(err_message);
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
            let rows = area.height.max(1);
            let cols = area.width.max(1);
            if tab.last_pty_size == Some((rows, cols)) {
                return;
            }
            let resize_started_at = Instant::now();

            if let Some(pty_master) = session.pty_master.as_ref()
                && let Ok(pty_master) = pty_master.lock()
            {
                let _ = pty_master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }

            if let Ok(mut parser) = session.parser.lock() {
                parser.set_size(rows, cols);
            }
            session.render_epoch.fetch_add(1, Ordering::Relaxed);

            tab.last_pty_size = Some((rows, cols));
            if debug_enabled!() {
                log_debug!("Resized session/parser to {}x{} in {:?}", cols, rows, resize_started_at.elapsed());
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../test/tui/features/terminal_session/pty.rs"]
mod tests;
