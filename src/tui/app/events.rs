//! Event loop and top-level event routing.

use crate::tui::SessionManager;
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use ratatui::DefaultTerminal;
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
    if app.pass_prompt.is_some() {
        return AppAction::Tab(TabAction::HandleKey);
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

fn should_mark_ui_dirty_for_key(app: &SessionManager, key: &KeyEvent) -> bool {
    let terminal_view_active = !app.focus_on_manager && !app.tabs.is_empty() && app.selected_tab < app.tabs.len();
    let terminal_search_active = app.current_tab_search().map(|search_state| search_state.active).unwrap_or(false);
    let direct_terminal_input = terminal_view_active && !terminal_search_active && app.quick_connect.is_none() && app.pass_prompt.is_none() && !app.search_mode;

    // Forwarded terminal typing/paste characters don't need eager UI invalidation.
    // PTY output updates render_epoch and drives redraws.
    if direct_terminal_input && key.modifiers.is_empty() && matches!(key.code, KeyCode::Char(_) | KeyCode::Enter | KeyCode::Tab) {
        return false;
    }

    true
}

pub(crate) fn run_app(terminal: &mut DefaultTerminal, app: &mut SessionManager) -> io::Result<()> {
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
                    if should_mark_ui_dirty_for_key(app, &key) {
                        app.mark_ui_dirty();
                    }
                    match resolve_action(app) {
                        AppAction::Exit => app.should_exit = true,
                        AppAction::Host(_) | AppAction::Tab(_) | AppAction::Terminal(_) | AppAction::QuickConnect(_) => app.handle_key(key)?,
                    }
                }
                Event::Paste(text) => {
                    app.mark_ui_dirty();
                    app.handle_paste(text)?;
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
