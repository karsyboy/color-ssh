//! Text selection and clipboard support
//!
//! Uses OSC 52 escape sequences via crossterm for clipboard operations.
//! This works in most modern terminals: Konsole, Kitty, Alacritty, Wezterm, foot, etc.

use super::extract::current_selection;
use crate::terminal_core::TerminalClipboardTarget;
use crate::terminal_host::copy_to_clipboard;
use crate::tui::AppState;

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

        let tab = &self.tabs[self.selected_tab];
        let session = match &tab.session {
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
}
