use super::io::{spawn_output_reader, spawn_process_exit_watcher};
use crate::auth::agent;
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::process;
use crate::tui::terminal_emulator::Parser;
use crate::tui::{AppState, HostTab, ManagedChild, ManagedSession, TerminalSearchState, VaultUnlockAction};
use crate::{command_path, debug_enabled, log_debug, log_error};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{self, Read};
use std::process::Stdio;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

struct SessionLaunchOptions {
    force_ssh_logging: bool,
    initial_rows: u16,
    initial_cols: u16,
    pass_entry_override: Option<String>,
    pass_fallback_notice: Option<String>,
}

fn auto_login_notice(host: &InventoryHost, detail: impl Into<String>) -> String {
    let detail = detail.into();
    match &host.protocol {
        ConnectionProtocol::Ssh => format!("{detail}; continuing with the standard SSH password prompt."),
        ConnectionProtocol::Rdp => format!("{detail}; RDP launch requires vault-backed credentials."),
        ConnectionProtocol::Other(protocol) => format!("{detail}; protocol '{}' is not supported for launch.", protocol),
    }
}

impl AppState {
    fn initial_pty_size(&self) -> (u16, u16) {
        let rows = self.tab_content_area.height.max(1);
        let cols = self.tab_content_area.width.max(1);
        (rows, cols)
    }

    fn resolved_history_buffer_for_host(&self, host: &InventoryHost) -> usize {
        crate::config::history_buffer_for_profile(host.profile.as_deref()).unwrap_or(self.history_buffer)
    }

    fn resolve_host_pass_password(&mut self, host: &InventoryHost, action: VaultUnlockAction) -> Option<(Option<String>, Option<String>)> {
        let auth_settings = crate::config::auth_settings();
        if !auth_settings.tui_password_autologin {
            let notice = match &host.protocol {
                ConnectionProtocol::Ssh => None,
                ConnectionProtocol::Rdp => {
                    Some("TUI password auto-login is disabled in auth settings; enable it to launch RDP sessions from the session manager.".to_string())
                }
                ConnectionProtocol::Other(protocol) => Some(format!("Protocol '{}' is not supported for launch.", protocol)),
            };
            return Some((None, notice));
        }

        let Some(pass_key) = host.vault_pass.as_deref() else {
            let notice = match &host.protocol {
                ConnectionProtocol::Ssh => None,
                ConnectionProtocol::Rdp => Some("RDP launch requires a password vault entry; add `vault_pass: <name>` to the inventory host.".to_string()),
                ConnectionProtocol::Other(protocol) => Some(format!("Protocol '{}' is not supported for launch.", protocol)),
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
                Some(match &host.protocol {
                    ConnectionProtocol::Ssh => "Password vault is not initialized. Run `cossh vault init` or `cossh vault add <name>` first.".to_string(),
                    ConnectionProtocol::Rdp => {
                        "RDP launch requires an initialized password vault. Run `cossh vault init` or `cossh vault add <name>` first.".to_string()
                    }
                    ConnectionProtocol::Other(protocol) => format!("Protocol '{}' is not supported for launch.", protocol),
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

    pub(crate) fn select_host_to_connect(&mut self) {
        let Some(host_idx) = self.selected_host_idx() else {
            return;
        };
        let host = self.hosts[host_idx].clone();
        self.open_host_tab(host, false);
    }

    pub(crate) fn open_quick_connect_host(&mut self, user: String, hostname: String, profile: Option<String>, force_ssh_logging: bool) {
        let user = user.trim().to_string();
        let target = if user.is_empty() {
            hostname.clone()
        } else {
            format!("{}@{}", user, hostname)
        };
        let mut host = InventoryHost::new(target);
        host.user = if user.is_empty() { None } else { Some(user) };
        host.host = hostname;
        host.profile = profile;
        self.open_host_tab(host, force_ssh_logging);
    }

    fn open_host_tab(&mut self, host: InventoryHost, force_ssh_logging: bool) {
        log_debug!("Opening {} tab for host: {}", host.protocol.as_str(), host.name);
        let action = VaultUnlockAction::OpenHostTab {
            host: Box::new(host.clone()),
            force_ssh_logging,
        };
        let Some((pass_entry_override, pass_fallback_notice)) = self.resolve_host_pass_password(&host, action) else {
            return;
        };
        self.open_host_tab_with_auth(host, force_ssh_logging, pass_entry_override, pass_fallback_notice);
    }

    fn open_host_tab_with_auth(
        &mut self,
        host: InventoryHost,
        force_ssh_logging: bool,
        pass_entry_override: Option<String>,
        pass_fallback_notice: Option<String>,
    ) {
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

    fn spawn_session(host: &InventoryHost, tab_title: &str, history_buffer: usize, launch_options: SessionLaunchOptions) -> io::Result<ManagedSession> {
        match &host.protocol {
            ConnectionProtocol::Ssh => Self::spawn_ssh_session(host, tab_title, history_buffer, launch_options),
            ConnectionProtocol::Rdp => Self::spawn_rdp_session(host, tab_title, history_buffer, launch_options),
            ConnectionProtocol::Other(protocol) => Err(io::Error::other(format!("unsupported protocol '{}'", protocol))),
        }
    }

    fn spawn_ssh_session(host: &InventoryHost, tab_title: &str, history_buffer: usize, launch_options: SessionLaunchOptions) -> io::Result<ManagedSession> {
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

    fn spawn_rdp_session(host: &InventoryHost, _tab_title: &str, history_buffer: usize, launch_options: SessionLaunchOptions) -> io::Result<ManagedSession> {
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
