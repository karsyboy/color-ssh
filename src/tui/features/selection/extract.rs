//! Selection extraction helpers.

use crate::terminal_core::TerminalSelection;

pub(crate) fn current_selection(start: Option<(i64, u16)>, end: Option<(i64, u16)>) -> Option<TerminalSelection> {
    Some(TerminalSelection::new(start?, end?).ordered())
}

/// Check if a cell at `(row, col)` is within the current text selection.
pub(crate) fn is_cell_in_selection(row: i64, col: u16, selection: Option<TerminalSelection>) -> bool {
    selection.is_some_and(|selection| selection.contains_cell(row, col))
}
