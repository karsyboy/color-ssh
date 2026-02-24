//! TUI bootstrap and teardown lifecycle.

use super::events::run_app;
use crate::tui::AppState;
use crate::{command_path, log_debug, log_error};
use crossterm::{
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, process::Command};

#[derive(Debug, Default)]
struct TerminalModeGuard {
    active: bool,
}

impl TerminalModeGuard {
    // Terminal mode lifecycle.
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
        Ok(Self { active: true })
    }

    fn cleanup(&mut self) {
        if !self.active {
            return;
        }

        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture, DisableBracketedPaste);
        self.active = false;
    }
}

impl Drop for TerminalModeGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Run the interactive session manager.
pub fn run_session_manager() -> io::Result<()> {
    log_debug!("Starting interactive session manager");

    let mut mode_guard = TerminalModeGuard::enter()?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = AppState::new()?;
    let result = run_app(&mut terminal, &mut app);
    let selected_request = app.selected_host_to_connect.take();
    let show_cursor_result = terminal.show_cursor();

    mode_guard.cleanup();

    if let Err(err) = result {
        log_error!("Session manager error: {}", err);
        eprintln!("Error: {}", err);
        return Err(err);
    }

    if let Err(err) = show_cursor_result {
        log_error!("Failed to restore cursor visibility: {}", err);
        return Err(err);
    }

    if let Some(request) = selected_request {
        log_debug!("Connecting to host: {}", request.target);

        let cossh_path = command_path::cossh_path()?;
        let mut cmd = Command::new(cossh_path);

        if request.force_ssh_logging {
            cmd.arg("-l");
        }
        if let Some(profile) = request.profile {
            cmd.arg("-P").arg(profile);
        }

        let status = cmd.arg(request.target).status()?;

        if !status.success() {
            log_error!("SSH connection failed with code: {:?}", status.code());
        }
    }

    log_debug!("Session manager exited");
    Ok(())
}
