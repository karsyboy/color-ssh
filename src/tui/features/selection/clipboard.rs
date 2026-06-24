//! Text selection and clipboard support
//!
//! Uses OSC 52 escape sequences via crossterm for clipboard operations.
//! This works in most modern terminals: Konsole, Kitty, Alacritty, Wezterm, foot, etc.

use super::extract::current_selection;
use crate::terminal::TerminalClipboardTarget;
use crate::terminal::copy_to_clipboard;
use crate::tui::AppState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn is_modal_copy_shortcut(key: &KeyEvent) -> bool {
    matches!(
        key.code,
        KeyCode::Char(ch)
            if ch.eq_ignore_ascii_case(&'c') && (key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT))
    )
}

impl AppState {
    // Selection export.
    /// Copy the current text selection to clipboard
    pub(crate) fn copy_selection_to_clipboard(&self) {
        let Some(selection) = current_selection(self.selection_start, self.selection_end) else {
            return;
        };

        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let session = match self.selected_terminal_tab().and_then(|tab| tab.session.as_ref()) {
            Some(session) => session,
            None => return,
        };

        let text = match session.selection_text_for(selection) {
            Ok(text) => text,
            Err(_) => return,
        };

        if text.is_empty() {
            return;
        }

        copy_to_clipboard(TerminalClipboardTarget::Clipboard, &text);
    }

    pub(crate) fn active_modal_selection_text(&self) -> Option<String> {
        self.rdp_credentials
            .as_ref()
            .and_then(|prompt| prompt.selected_text())
            .or_else(|| self.quick_connect.as_ref().and_then(|form| form.selected_text()))
            .or_else(|| self.folder_rename.as_ref().and_then(|state| state.selected_text()))
            .or_else(|| self.folder_create.as_ref().and_then(|state| state.selected_text()))
            .or_else(|| self.selected_host_editor().and_then(|form| form.selected_text()))
    }

    pub(crate) fn copy_active_modal_selection_to_clipboard(&self) {
        let Some(text) = self.active_modal_selection_text() else {
            return;
        };

        if text.is_empty() {
            return;
        }

        copy_to_clipboard(TerminalClipboardTarget::Clipboard, &text);
    }
}
