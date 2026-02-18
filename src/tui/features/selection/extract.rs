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

pub(crate) fn extract_selection_text(parser: &mut Parser, start: (i64, u16), end: (i64, u16)) -> String {
    let (visible_rows, cols) = parser.screen().size();
    let abs_start = start.0;
    let abs_end = end.0;
    parser.with_scrollback_restored(|parser| {
        let mut result = String::new();

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

            let col_start = if abs_r == abs_start { start.1 } else { 0 };
            let col_end = if abs_r == abs_end { end.1 } else { cols.saturating_sub(1) };
            let col_end = col_end.min(cols.saturating_sub(1));

            if let Some(row) = parser.row_snapshot(sr) {
                result.push_str(row.slice_columns(col_start, col_end).trim_end());
            }

            if abs_r < abs_end {
                result.push('\n');
            }
        }

        result
    })
}

#[cfg(test)]
mod tests {
    use super::extract_selection_text;
    use crate::tui::terminal_emulator::Parser;

    #[test]
    fn extract_selection_text_restores_scrollback_view() {
        let mut parser = Parser::new(2, 5, 50);
        parser.process(b"11111\r\n22222\r\n33333\r\n");
        parser.set_scrollback(1);
        let before_cell = parser.screen().cell(0, 0).map(|cell| cell.contents()).unwrap_or_default();

        let _ = extract_selection_text(&mut parser, (0, 0), (0, 2));
        let after_cell = parser.screen().cell(0, 0).map(|cell| cell.contents()).unwrap_or_default();

        assert_eq!(before_cell, after_cell);
    }
}
