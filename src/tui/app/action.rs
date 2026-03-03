use crate::tui::AppState;
use crossterm::event::{KeyCode, KeyEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppAction {
    Host,
    Tab,
    TerminalSearch,
    QuickConnect,
    Exit,
}

pub(crate) fn resolve_action(app: &AppState) -> AppAction {
    if app.should_exit {
        return AppAction::Exit;
    }
    if app.vault_unlock.is_some() || app.vault_status_modal.is_some() {
        return AppAction::Tab;
    }
    if app.quick_connect.is_some() {
        return AppAction::QuickConnect;
    }
    if app.search_mode {
        return AppAction::Host;
    }
    if !app.focus_on_manager && app.current_tab_search().map(|search_state| search_state.active).unwrap_or(false) {
        return AppAction::TerminalSearch;
    }
    if !app.focus_on_manager && !app.tabs.is_empty() && app.selected_tab < app.tabs.len() {
        return AppAction::Tab;
    }
    AppAction::Host
}

pub(crate) fn should_mark_ui_dirty_for_key(app: &AppState, key: &KeyEvent) -> bool {
    let terminal_view_active = !app.focus_on_manager && !app.tabs.is_empty() && app.selected_tab < app.tabs.len();
    let terminal_search_active = app.current_tab_search().map(|search_state| search_state.active).unwrap_or(false);
    let direct_terminal_input = terminal_view_active
        && !terminal_search_active
        && app.quick_connect.is_none()
        && app.vault_unlock.is_none()
        && app.vault_status_modal.is_none()
        && !app.search_mode;

    if direct_terminal_input && key.modifiers.is_empty() && matches!(key.code, KeyCode::Char(_) | KeyCode::Enter | KeyCode::Tab) {
        return false;
    }

    true
}
