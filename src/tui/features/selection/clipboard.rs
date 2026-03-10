//! Text selection and clipboard support
//!
//! Uses OSC 52 escape sequences via crossterm for clipboard operations.
//! This works in most modern terminals: Konsole, Kitty, Alacritty, Wezterm, foot, etc.

use super::extract::current_selection;
use crate::tui::AppState;
use crossterm::clipboard::CopyToClipboard;
use crossterm::execute;
use std::io::{Write, stdout};

/// Copy text to system clipboard using OSC 52 escape sequence
fn copy_to_clipboard(text: &str) {
    let _ = execute!(stdout(), CopyToClipboard::to_clipboard_from(text));
    let _ = stdout().flush();
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

        copy_to_clipboard(&text);
    }
}
