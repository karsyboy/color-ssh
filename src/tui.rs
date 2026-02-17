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

use crate::{log_debug, log_error};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, process::Command, time::Duration};

/// Run the interactive session manager.
pub fn run_session_manager() -> io::Result<()> {
    log_debug!("Starting interactive session manager");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = SessionManager::new()?;
    let result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        log_error!("Session manager error: {}", err);
        eprintln!("Error: {}", err);
        return Err(err);
    }

    if let Some(request) = app.selected_host_to_connect {
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
    loop {
        app.check_clear_pending();
        terminal.draw(|frame| app.draw(frame))?;

        if app.should_exit {
            break;
        }

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => app.handle_key(key)?,
                Event::Mouse(mouse) => app.handle_mouse(mouse)?,
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    Ok(())
}
