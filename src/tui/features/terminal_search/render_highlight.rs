//! Terminal-search highlight range helpers.

#[cfg(test)]
use crate::tui::TerminalSearchState;
use std::collections::HashMap;

pub(crate) type SearchRowRanges = HashMap<i64, Vec<(u16, u16)>>;
pub(crate) type CurrentSearchRange = Option<(i64, u16, u16)>;

pub(crate) fn build_search_row_ranges_from_matches(matches: &[(i64, u16, usize)], current: usize) -> (SearchRowRanges, CurrentSearchRange) {
    if matches.is_empty() {
        return (HashMap::new(), None);
    }

    let current_match = matches.get(current).map(|(row, col, len)| {
        let end_col = col.saturating_add(*len as u16);
        (*row, *col, end_col)
    });

    let mut row_ranges: HashMap<i64, Vec<(u16, u16)>> = HashMap::new();
    for (row, col, len) in matches {
        let end_col = col.saturating_add(*len as u16);
        row_ranges.entry(*row).or_default().push((*col, end_col));
    }
    for ranges in row_ranges.values_mut() {
        ranges.sort_unstable_by_key(|(start_col, _)| *start_col);
    }

    (row_ranges, current_match)
}

#[cfg(test)]
pub(crate) fn build_search_row_ranges(search: Option<&TerminalSearchState>) -> (SearchRowRanges, CurrentSearchRange) {
    let Some(search_state) = search else {
        return (HashMap::new(), None);
    };
    if !search_state.active || search_state.matches.is_empty() {
        return (HashMap::new(), None);
    }

    build_search_row_ranges_from_matches(&search_state.matches, search_state.current)
}

#[cfg(test)]
#[path = "../../../test/tui/features/terminal_search/render_highlight.rs"]
mod tests;
