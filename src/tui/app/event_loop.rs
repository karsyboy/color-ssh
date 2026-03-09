//! Event loop and top-level event routing.

use super::action::{AppAction, resolve_action, should_mark_ui_dirty_for_key};
use crate::tui::AppState;
use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;
use std::{io, time::Duration};

pub(crate) fn run_app(terminal: &mut DefaultTerminal, app: &mut AppState) -> io::Result<()> {
    const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
    const RENDER_HEARTBEAT: Duration = Duration::from_millis(250);
    const VAULT_STATUS_MODAL_POLL_INTERVAL: Duration = Duration::from_secs(1);

    loop {
        app.apply_vault_status_notifications();
        app.apply_config_reload_notifications();
        app.expire_reload_notice_toast();
        app.refresh_vault_status_if_stale(VAULT_STATUS_MODAL_POLL_INTERVAL);
        app.refresh_active_terminal_search_if_needed();

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
                        AppAction::Host | AppAction::Tab | AppAction::TerminalSearch | AppAction::QuickConnect => app.handle_key(key)?,
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
