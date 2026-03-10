//! Terminal-search highlight range helpers.

use std::collections::HashMap;

pub(crate) type SearchRowRanges = HashMap<i64, Vec<(u16, u16)>>;
pub(crate) type CurrentSearchRange = Option<(i64, u16, u16)>;

pub(crate) fn build_search_row_ranges_from_matches(matches: &[(i64, u16, u16)], current: usize) -> (SearchRowRanges, CurrentSearchRange) {
    if matches.is_empty() {
        return (HashMap::new(), None);
    }

    let current_match = matches.get(current).copied();

    let mut row_ranges: HashMap<i64, Vec<(u16, u16)>> = HashMap::new();
    for (row, start_col, end_col) in matches {
        row_ranges.entry(*row).or_default().push((*start_col, *end_col));
    }
    for ranges in row_ranges.values_mut() {
        ranges.sort_unstable_by_key(|(start_col, _)| *start_col);
    }

    (row_ranges, current_match)
}
