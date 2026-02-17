//! Text selection and clipboard support
//!
//! Uses OSC 52 escape sequences via crossterm for clipboard operations.
//! This works in most modern terminals: Konsole, Kitty, Alacritty, Wezterm, foot, etc.

use super::SessionManager;
use crossterm::clipboard::CopyToClipboard;
use crossterm::execute;
use std::io::{Write, stdout};
use vt100::Parser;

/// Check if a cell at (row, col) is within the current text selection
pub fn is_cell_in_selection(row: i64, col: u16, start: Option<(i64, u16)>, end: Option<(i64, u16)>) -> bool {
    let (start, end) = match (start, end) {
        (Some(selection_start), Some(selection_end)) => {
            // Normalize so start <= end in reading order
            if selection_start.0 < selection_end.0 || (selection_start.0 == selection_end.0 && selection_start.1 <= selection_end.1) {
                (selection_start, selection_end)
            } else {
                (selection_end, selection_start)
            }
        }
        _ => return false,
    };

    if row < start.0 || row > end.0 {
        return false;
    }
    if start.0 == end.0 {
        // Single row: selected from start.1 to end.1
        col >= start.1 && col <= end.1
    } else if row == start.0 {
        // First row: from start.1 to end of line
        col >= start.1
    } else if row == end.0 {
        // Last row: from beginning to end.1
        col <= end.1
    } else {
        // Middle rows: entirely selected
        true
    }
}

/// Copy text to system clipboard using OSC 52 escape sequence
fn copy_to_clipboard(text: &str) {
    let _ = execute!(stdout(), CopyToClipboard::to_clipboard_from(text));
    let _ = stdout().flush();
}

fn extract_selection_text(parser: &mut Parser, start: (i64, u16), end: (i64, u16), restore_scrollback: usize) -> String {
    let (visible_rows, cols) = parser.screen().size();
    let mut result = String::new();

    let abs_start = start.0;
    let abs_end = end.0;

    for abs_r in abs_start..=abs_end {
        let sb = if abs_r < 0 { (-abs_r) as usize } else { 0 };
        let sr = (abs_r + sb as i64) as u16;

        if sr >= visible_rows {
            if abs_r < abs_end {
                result.push('\n');
            }
            continue;
        }

        parser.set_scrollback(sb);
        let screen = parser.screen();

        let col_start = if abs_r == abs_start { start.1 } else { 0 };
        let col_end = if abs_r == abs_end { end.1 } else { cols.saturating_sub(1) };

        let mut line = String::new();
        for col in col_start..=col_end.min(cols.saturating_sub(1)) {
            if let Some(cell) = screen.cell(sr, col) {
                if cell.has_contents() {
                    line.push_str(&cell.contents());
                } else {
                    line.push(' ');
                }
            }
        }
        let trimmed = line.trim_end();
        result.push_str(trimmed);
        if abs_r < abs_end {
            result.push('\n');
        }
    }

    parser.set_scrollback(restore_scrollback);
    result
}

impl SessionManager {
    /// Copy the current text selection to clipboard
    pub(super) fn copy_selection_to_clipboard(&self) {
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
        let restore_scrollback = tab.scroll_offset;

        let text = if let Ok(mut parser) = session.parser.lock() {
            extract_selection_text(&mut parser, start, end, restore_scrollback)
        } else {
            return;
        };

        if text.is_empty() {
            return;
        }

        copy_to_clipboard(&text);
    }
}

#[cfg(test)]
mod tests {
    use super::extract_selection_text;
    use vt100::Parser;

    #[test]
    fn extract_selection_text_restores_scrollback_view() {
        let mut parser = Parser::new(2, 5, 50);
        parser.process(b"11111\r\n22222\r\n33333\r\n");
        parser.set_scrollback(1);
        let before_cell = parser.screen().cell(0, 0).map(|cell| cell.contents()).unwrap_or_default();

        let _ = extract_selection_text(&mut parser, (0, 0), (0, 2), 1);
        let after_cell = parser.screen().cell(0, 0).map(|cell| cell.contents()).unwrap_or_default();

        assert_eq!(before_cell, after_cell);
    }
}
