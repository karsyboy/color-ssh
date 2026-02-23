//! Terminal-search highlight range helpers.

use crate::tui::TerminalSearchState;
use std::collections::HashMap;

pub(crate) type SearchRowRanges = HashMap<i64, Vec<(u16, u16)>>;
pub(crate) type CurrentSearchRange = Option<(i64, u16, u16)>;

pub(crate) fn build_search_row_ranges(search: Option<&TerminalSearchState>) -> (SearchRowRanges, CurrentSearchRange) {
    let Some(search_state) = search else {
        return (HashMap::new(), None);
    };
    if !search_state.active || search_state.matches.is_empty() {
        return (HashMap::new(), None);
    }

    let current_match = search_state.matches.get(search_state.current).map(|(row, col, len)| {
        let end_col = col.saturating_add(*len as u16);
        (*row, *col, end_col)
    });

    let mut row_ranges: HashMap<i64, Vec<(u16, u16)>> = HashMap::new();
    for (row, col, len) in &search_state.matches {
        let end_col = col.saturating_add(*len as u16);
        row_ranges.entry(*row).or_default().push((*col, end_col));
    }

    (row_ranges, current_match)
}

#[cfg(test)]
#[path = "../../../test/tui/features/terminal_search/render_highlight.rs"]
mod tests;
