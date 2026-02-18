//! Event loop and top-level event routing.

use crate::tui::SessionManager;
use crossterm::event::{self, Event};
use ratatui::Terminal;
use std::{io, time::Duration};

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
            terminal.draw(|frame| app.draw(frame))?;
            app.mark_drawn();
            // This is for troubleshooting render times
            // let render_started_at = std::time::Instant::now();
            // if crate::debug_enabled!() {
            //     crate::log_debug!("TUI frame rendered in {:?}", render_started_at.elapsed());
            // }
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
                        AppAction::Host(_) | AppAction::Tab(_) | AppAction::Terminal(_) | AppAction::QuickConnect(_) => app.handle_key(key)?,
                    }
                }
                Event::Mouse(mouse) => {
                    app.mark_ui_dirty();
                    app.handle_mouse(mouse)?;
                }
                Event::Resize(_, _) => {
                    if let Ok((width, height)) = crossterm::terminal::size() {
                        app.handle_terminal_resize(width, height);
                    }
                    app.mark_ui_dirty();
                }
                _ => {}
            }
        }
    }

    Ok(())
}
