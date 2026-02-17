//! Interactive TUI-based SSH host selector.

mod input;
mod mouse;
mod render;
mod search;
mod selection;
mod ssh_session;
mod state;
mod status_bar;

pub(super) use state::{
    ConnectRequest, HostTab, HostTreeRow, HostTreeRowKind, QuickConnectField, QuickConnectState, SessionManager, SshSession, TerminalSearchState,
};

use crate::{debug_enabled, log_debug, log_error};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, Default)]
struct TerminalModeGuard {
    active: bool,
}

impl TerminalModeGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        Ok(Self { active: true })
    }

    fn cleanup(&mut self) {
        if !self.active {
            return;
        }

        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
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

    let mut app = SessionManager::new()?;
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

        let cossh_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("cossh"));
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

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut SessionManager) -> io::Result<()> {
    const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
    const RENDER_HEARTBEAT: Duration = Duration::from_millis(250);

    loop {
        if app.should_draw(RENDER_HEARTBEAT) {
            app.check_clear_pending();
            let render_started_at = Instant::now();
            terminal.draw(|frame| app.draw(frame))?;
            app.mark_drawn();
            if debug_enabled!() {
                log_debug!("TUI frame rendered in {:?}", render_started_at.elapsed());
            }
        }

        if app.should_exit {
            break;
        }

        if event::poll(EVENT_POLL_INTERVAL)? {
            match event::read()? {
                Event::Key(key) => {
                    app.mark_ui_dirty();
                    app.handle_key(key)?;
                }
                Event::Mouse(mouse) => {
                    app.mark_ui_dirty();
                    app.handle_mouse(mouse)?;
                }
                Event::Resize(_, _) => {
                    app.mark_ui_dirty();
                }
                _ => {}
            }
        }
    }

    Ok(())
}
