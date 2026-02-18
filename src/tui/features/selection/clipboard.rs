//! Text selection and clipboard support
//!
//! Uses OSC 52 escape sequences via crossterm for clipboard operations.
//! This works in most modern terminals: Konsole, Kitty, Alacritty, Wezterm, foot, etc.

use super::extract::extract_selection_text;
use crate::tui::SessionManager;
use crossterm::clipboard::CopyToClipboard;
use crossterm::execute;
use std::io::{Write, stdout};

/// Copy text to system clipboard using OSC 52 escape sequence
fn copy_to_clipboard(text: &str) {
    let _ = execute!(stdout(), CopyToClipboard::to_clipboard_from(text));
    let _ = stdout().flush();
}

impl SessionManager {
    /// Copy the current text selection to clipboard
    pub(crate) fn copy_selection_to_clipboard(&self) {
        let (start, end) = match (self.selection_start, self.selection_end) {
            (Some(selection_start), Some(selection_end)) => {
                // Normalize so start <= end in reading order
                if selection_start.0 < selection_end.0 || (selection_start.0 == selection_end.0 && selection_start.1 <= selection_end.1) {
                    (selection_start, selection_end)
                } else {
                    (selection_end, selection_start)
                }
            }
            _ => return,
        };

        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let tab = &self.tabs[self.selected_tab];
        let session = match &tab.session {
            Some(session) => session,
            None => return,
        };

        let text = if let Ok(mut parser) = session.parser.lock() {
            extract_selection_text(&mut parser, start, end)
        } else {
            return;
        };

        if text.is_empty() {
            return;
        }

        copy_to_clipboard(&text);
    }
}
