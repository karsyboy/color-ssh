//! Selection extraction helpers.

use crate::tui::terminal_emulator::Parser;

/// Check if a cell at `(row, col)` is within the current text selection.
pub(crate) fn is_cell_in_selection(row: i64, col: u16, start: Option<(i64, u16)>, end: Option<(i64, u16)>) -> bool {
    let (start, end) = match (start, end) {
        (Some(selection_start), Some(selection_end)) => {
            // Normalize so start <= end in reading order.
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
        col >= start.1 && col <= end.1
    } else if row == start.0 {
        col >= start.1
    } else if row == end.0 {
        col <= end.1
    } else {
        true
    }
}

pub(crate) fn extract_selection_text(parser: &Parser, start: (i64, u16), end: (i64, u16)) -> String {
    parser.selection_text(start, end)
}

#[cfg(test)]
#[path = "../../../test/tui/features/selection/extract.rs"]
mod tests;
