use super::io::{spawn_output_reader, spawn_process_exit_watcher};
use crate::auth::agent;
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::process;
use crate::terminal_core::{TerminalChild, TerminalEngine, TerminalSession};
use crate::tui::{AppState, HostTab, TerminalSearchState, VaultUnlockAction};
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
    disable_vault_autologin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HostPassResolution {
    pass_entry_override: Option<String>,
    pass_fallback_notice: Option<String>,
    disable_vault_autologin: bool,
}

fn auto_login_notice(host: &InventoryHost, detail: impl Into<String>) -> String {
    let detail = detail.into();
    match &host.protocol {
        ConnectionProtocol::Ssh => format!("{detail}; continuing with the standard SSH password prompt."),
        ConnectionProtocol::Rdp => format!("{detail}; continuing with the FreeRDP password prompt."),
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

    fn resolve_host_pass_password_with_autologin(
        &mut self,
        host: &InventoryHost,
        action: VaultUnlockAction,
        tui_password_autologin: bool,
    ) -> Option<HostPassResolution> {
        if !tui_password_autologin {
            return Some(match &host.protocol {
                ConnectionProtocol::Ssh | ConnectionProtocol::Rdp => HostPassResolution {
                    pass_entry_override: None,
                    pass_fallback_notice: None,
                    disable_vault_autologin: true,
                },
                ConnectionProtocol::Other(protocol) => HostPassResolution {
                    pass_entry_override: None,
                    pass_fallback_notice: Some(format!("Protocol '{}' is not supported for launch.", protocol)),
                    disable_vault_autologin: true,
                },
            });
        }

        let Some(pass_key) = host.vault_pass.as_deref() else {
            return Some(match &host.protocol {
                ConnectionProtocol::Ssh | ConnectionProtocol::Rdp => HostPassResolution {
                    pass_entry_override: None,
                    pass_fallback_notice: None,
                    disable_vault_autologin: false,
                },
                ConnectionProtocol::Other(protocol) => HostPassResolution {
                    pass_entry_override: None,
                    pass_fallback_notice: Some(format!("Protocol '{}' is not supported for launch.", protocol)),
                    disable_vault_autologin: false,
                },
            });
        };

        let client = match agent::AgentClient::new() {
            Ok(client) => client,
            Err(err) => {
                return Some(HostPassResolution {
                    pass_entry_override: None,
                    pass_fallback_notice: Some(auto_login_notice(
                        host,
                        format!("Password auto-login is unavailable because the password vault agent could not be started ({err})"),
                    )),
                    disable_vault_autologin: true,
                });
            }
        };

        match client.entry_status(pass_key) {
            Ok(entry_status) => {
                let exists = entry_status.exists;
                let unlocked = entry_status.status.unlocked;
                self.set_vault_status(entry_status.status);
                if exists && unlocked {
                    return Some(HostPassResolution {
                        pass_entry_override: Some(pass_key.to_string()),
                        pass_fallback_notice: None,
                        disable_vault_autologin: false,
                    });
                }
                if !exists {
                    return Some(HostPassResolution {
                        pass_entry_override: None,
                        pass_fallback_notice: Some(auto_login_notice(
                            host,
                            format!("Password auto-login is unavailable because vault entry '{}' was not found", pass_key),
                        )),
                        disable_vault_autologin: true,
                    });
                }
                self.open_vault_unlock(pass_key.to_string(), action);
                None
            }
            Err(agent::AgentError::VaultNotInitialized) => Some(HostPassResolution {
                pass_entry_override: None,
                pass_fallback_notice: Some(match &host.protocol {
                    ConnectionProtocol::Ssh => "Password vault is not initialized. Run `cossh vault init` or `cossh vault add <name>` first.".to_string(),
                    ConnectionProtocol::Rdp => auto_login_notice(
                        host,
                        "the password vault is not initialized. Run `cossh vault init` or `cossh vault add <name>` first",
                    ),
                    ConnectionProtocol::Other(protocol) => format!("Protocol '{}' is not supported for launch.", protocol),
                }),
                disable_vault_autologin: true,
            }),
            Err(err) => {
                log_debug!("Password auto-login unavailable for host {}: {}", host.name, err);
                Some(HostPassResolution {
                    pass_entry_override: None,
                    pass_fallback_notice: Some(auto_login_notice(
                        host,
                        format!("Password auto-login is unavailable because the password vault could not be queried ({err})"),
                    )),
                    disable_vault_autologin: true,
                })
            }
        }
    }

    fn resolve_host_pass_password(&mut self, host: &InventoryHost, action: VaultUnlockAction) -> Option<HostPassResolution> {
        let auth_settings = crate::config::auth_settings();
        self.resolve_host_pass_password_with_autologin(host, action, auth_settings.tui_password_autologin)
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
        let Some(auth_resolution) = self.resolve_host_pass_password(&host, action) else {
            return;
        };
        self.open_host_tab_with_auth(
            host,
            force_ssh_logging,
            auth_resolution.pass_entry_override,
            auth_resolution.pass_fallback_notice,
            auth_resolution.disable_vault_autologin,
        );
    }

    fn open_host_tab_with_auth(
        &mut self,
        host: InventoryHost,
        force_ssh_logging: bool,
        pass_entry_override: Option<String>,
        pass_fallback_notice: Option<String>,
        disable_vault_autologin: bool,
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
            disable_vault_autologin,
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
        let disable_vault_autologin = pass_fallback_notice.is_some();
        match action {
            VaultUnlockAction::UnlockVault => {}
            VaultUnlockAction::OpenHostTab { host, force_ssh_logging } => {
                self.open_host_tab_with_auth(*host, force_ssh_logging, pass_entry_override, pass_fallback_notice, disable_vault_autologin);
            }
            VaultUnlockAction::ReconnectTab { tab_index } => {
                self.reconnect_session_with_auth(tab_index, pass_entry_override, pass_fallback_notice, disable_vault_autologin);
            }
        }
    }

    fn spawn_session(host: &InventoryHost, tab_title: &str, history_buffer: usize, launch_options: SessionLaunchOptions) -> io::Result<TerminalSession> {
        match &host.protocol {
            ConnectionProtocol::Ssh => Self::spawn_ssh_session(host, tab_title, history_buffer, launch_options),
            ConnectionProtocol::Rdp => Self::spawn_rdp_session(host, tab_title, history_buffer, launch_options),
            ConnectionProtocol::Other(protocol) => Err(io::Error::other(format!("unsupported protocol '{}'", protocol))),
        }
    }

    fn spawn_ssh_session(host: &InventoryHost, tab_title: &str, history_buffer: usize, launch_options: SessionLaunchOptions) -> io::Result<TerminalSession> {
        let pty_system = native_pty_system();
        let SessionLaunchOptions {
            force_ssh_logging,
            initial_rows,
            initial_cols,
            pass_entry_override,
            pass_fallback_notice,
            disable_vault_autologin,
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
        if disable_vault_autologin {
            cmd.env(process::DISABLE_VAULT_AUTOLOGIN_ENV, "1");
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
        let vault_info = if disable_vault_autologin { " [no-vault-autologin]" } else { "" };
        log_debug!(
            "Spawning cossh command: cossh ssh {}{}{}{}{} (session: {})",
            host.name,
            pass_info,
            profile_info,
            logging_info,
            vault_info,
            tab_title
        );

        let child = Arc::new(Mutex::new(pty_pair.slave.spawn_command(cmd).map_err(|err| io::Error::other(err.to_string()))?));

        let mut reader = pty_pair.master.try_clone_reader().map_err(|err| io::Error::other(err.to_string()))?;
        let writer = pty_pair.master.take_writer().map_err(|err| io::Error::other(err.to_string()))?;
        let writer = Arc::new(Mutex::new(writer));

        let engine = Arc::new(Mutex::new(TerminalEngine::new_with_input_writer(rows, cols, history_buffer, writer.clone())));
        let engine_clone = engine.clone();
        let exited = Arc::new(Mutex::new(false));
        let exited_clone = exited.clone();
        let pty_master = Arc::new(Mutex::new(pty_pair.master));
        let render_epoch = Arc::new(AtomicU64::new(0));
        let render_epoch_clone = render_epoch.clone();

        if let Some(notice) = pass_fallback_notice
            && let Ok(mut engine) = engine.lock()
        {
            let message = format!("\r\n[color-ssh] {}\r\n", notice);
            engine.process_output(message.as_bytes());
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
                        if let Ok(mut engine) = engine_clone.lock() {
                            engine.process_output(&buf[..bytes_read]);
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

        Ok(TerminalSession::new(
            Some(pty_master),
            Some(writer),
            TerminalChild::Pty(child),
            engine,
            exited,
            render_epoch,
        ))
    }

    fn spawn_rdp_session(host: &InventoryHost, _tab_title: &str, history_buffer: usize, launch_options: SessionLaunchOptions) -> io::Result<TerminalSession> {
        let SessionLaunchOptions {
            initial_rows,
            initial_cols,
            pass_entry_override,
            pass_fallback_notice,
            disable_vault_autologin,
            ..
        } = launch_options;
        let mut launch_host = host.clone();
        let pass_entry_override = if disable_vault_autologin { None } else { pass_entry_override };
        if disable_vault_autologin {
            launch_host.vault_pass = None;
        }
        let mut command_spec = process::build_rdp_command_for_host(&launch_host, pass_entry_override.as_deref())?;
        let launch_notice = pass_fallback_notice.or(command_spec.fallback_notice.take());

        if command_spec.stdin_payload.is_none() {
            let pty_system = native_pty_system();
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

            let program_path = command_path::resolve_known_command_path(&command_spec.program)?;
            let mut cmd = CommandBuilder::new(&program_path);
            for arg in &command_spec.args {
                cmd.arg(arg);
            }
            for (key, value) in &command_spec.env {
                cmd.env(key, value);
            }

            let child = Arc::new(Mutex::new(pty_pair.slave.spawn_command(cmd).map_err(|err| io::Error::other(err.to_string()))?));
            let mut reader = pty_pair.master.try_clone_reader().map_err(|err| io::Error::other(err.to_string()))?;
            let writer = pty_pair.master.take_writer().map_err(|err| io::Error::other(err.to_string()))?;
            let writer = Arc::new(Mutex::new(writer));

            let engine = Arc::new(Mutex::new(TerminalEngine::new_with_input_writer(rows, cols, history_buffer, writer.clone())));
            let engine_clone = engine.clone();
            let exited = Arc::new(Mutex::new(false));
            let exited_clone = exited.clone();
            let pty_master = Arc::new(Mutex::new(pty_pair.master));
            let render_epoch = Arc::new(AtomicU64::new(0));
            let render_epoch_clone = render_epoch.clone();

            if let Some(notice) = launch_notice
                && let Ok(mut engine) = engine.lock()
            {
                let message = format!("\r\n[color-ssh] {}\r\n", notice);
                engine.process_output(message.as_bytes());
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
                            if let Ok(mut engine) = engine_clone.lock() {
                                engine.process_output(&buf[..bytes_read]);
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

            return Ok(TerminalSession::new(
                Some(pty_master),
                Some(writer),
                TerminalChild::Pty(child),
                engine,
                exited,
                render_epoch,
            ));
        }

        let mut child = process::spawn_command(command_spec, Stdio::piped(), Stdio::piped())?;
        let stdout = child.stdout.take().ok_or_else(|| io::Error::other("failed to capture FreeRDP stdout"))?;
        let stderr = child.stderr.take().ok_or_else(|| io::Error::other("failed to capture FreeRDP stderr"))?;

        let rows = initial_rows.max(1);
        let cols = initial_cols.max(1);
        let engine = Arc::new(Mutex::new(TerminalEngine::new(rows, cols, history_buffer)));
        let exited = Arc::new(Mutex::new(false));
        let render_epoch = Arc::new(AtomicU64::new(0));
        let child = Arc::new(Mutex::new(child));

        spawn_output_reader("freerdp stdout", stdout, engine.clone(), render_epoch.clone());
        spawn_output_reader("freerdp stderr", stderr, engine.clone(), render_epoch.clone());
        spawn_process_exit_watcher(child.clone(), exited.clone());

        if let Some(notice) = launch_notice
            && let Ok(mut engine) = engine.lock()
        {
            let message = format!("\r\n[color-ssh] {}\r\n", notice);
            engine.process_output(message.as_bytes());
            render_epoch.fetch_add(1, Ordering::Relaxed);
        }

        Ok(TerminalSession::new(None, None, TerminalChild::Process(child), engine, exited, render_epoch))
    }

    pub(crate) fn reconnect_session(&mut self) {
        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let tab_index = self.selected_tab;
        let host = self.tabs[tab_index].host.clone();

        log_debug!("Reconnecting session for host: {}", host.name);

        let action = VaultUnlockAction::ReconnectTab { tab_index };
        let Some(auth_resolution) = self.resolve_host_pass_password(&host, action) else {
            return;
        };
        self.reconnect_session_with_auth(
            tab_index,
            auth_resolution.pass_entry_override,
            auth_resolution.pass_fallback_notice,
            auth_resolution.disable_vault_autologin,
        );
    }

    fn reconnect_session_with_auth(
        &mut self,
        tab_index: usize,
        pass_entry_override: Option<String>,
        pass_fallback_notice: Option<String>,
        disable_vault_autologin: bool,
    ) {
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
            disable_vault_autologin,
        };

        match Self::spawn_session(&host, &tab_title, history_buffer, session_launch_options) {
            Ok(session) => {
                let tab = &mut self.tabs[tab_index];
                tab.session = Some(session);
                tab.session_error = None;
                tab.scroll_offset = 0;
                tab.terminal_search.matches.clear();
                tab.terminal_search.current = 0;
                tab.terminal_search.highlight_row_ranges.clear();
                tab.terminal_search.current_highlight_range = None;
                tab.terminal_search.last_search_query.clear();
                tab.terminal_search.last_scanned_render_epoch = 0;
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

            session.resize(rows, cols);

            tab.last_pty_size = Some((rows, cols));
            if debug_enabled!() {
                log_debug!("Resized session/parser to {}x{} in {:?}", cols, rows, resize_started_at.elapsed());
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../test/tui/terminal_launch.rs"]
mod tests;
