//! Event loop and top-level event routing.

use crate::tui::SessionManager;
use crate::{debug_enabled, log_debug};
use crossterm::event::{self, Event};
use ratatui::Terminal;
use std::{
    io,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppAction {
    Host(HostAction),
    Tab(TabAction),
    Terminal(TerminalAction),
    QuickConnect(QuickConnectAction),
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostAction {
    HandleKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabAction {
    HandleKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalAction {
    HandleSearchKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuickConnectAction {
    HandleKey,
}

fn resolve_action(app: &SessionManager) -> AppAction {
    if app.should_exit {
        return AppAction::Exit;
    }
    if app.quick_connect.is_some() {
        return AppAction::QuickConnect(QuickConnectAction::HandleKey);
    }
    if app.search_mode {
        return AppAction::Host(HostAction::HandleKey);
    }
    if !app.focus_on_manager && app.current_tab_search().map(|search_state| search_state.active).unwrap_or(false) {
        return AppAction::Terminal(TerminalAction::HandleSearchKey);
    }
    if !app.focus_on_manager && !app.tabs.is_empty() && app.selected_tab < app.tabs.len() {
        return AppAction::Tab(TabAction::HandleKey);
    }
    AppAction::Host(HostAction::HandleKey)
}

pub(crate) fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut SessionManager) -> io::Result<()> {
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
                    match resolve_action(app) {
                        AppAction::Exit => app.should_exit = true,
                        _ if key.code == crossterm::event::KeyCode::Char('q') && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                            app.should_exit = true;
                        }
                        AppAction::Host(_) | AppAction::Tab(_) | AppAction::Terminal(_) | AppAction::QuickConnect(_) => app.handle_key(key)?,
                    }
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
