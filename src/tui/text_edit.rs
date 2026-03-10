//! Shared Unicode-safe text editing and rendering helpers.

use ratatui::{style::Style, text::Span};

pub(crate) type TextSelection = Option<(usize, usize)>;

pub(crate) fn char_len(text: &str) -> usize {
    text.chars().count()
}

pub(crate) fn clamp_cursor(text: &str, cursor: &mut usize) {
    *cursor = (*cursor).min(char_len(text));
}

pub(crate) fn normalized_selection(text: &str, selection: TextSelection) -> TextSelection {
    let (start, end) = selection?;
    let len = char_len(text);
    let start = start.min(len);
    let end = end.min(len);
    if start == end {
        None
    } else if start < end {
        Some((start, end))
    } else {
        Some((end, start))
    }
}

pub(crate) fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }

    let max = char_len(text);
    let clamped = char_index.min(max);
    if clamped == max {
        return text.len();
    }

    text.char_indices().nth(clamped).map_or(text.len(), |(byte_index, _)| byte_index)
}

pub(crate) fn delete_selection(text: &mut String, cursor: &mut usize, selection: &mut TextSelection) -> bool {
    let Some((start, end)) = normalized_selection(text, *selection) else {
        *selection = None;
        return false;
    };

    let start_byte = byte_index_for_char(text, start);
    let end_byte = byte_index_for_char(text, end);
    text.replace_range(start_byte..end_byte, "");
    *cursor = start;
    *selection = None;
    true
}

fn push_if_non_empty<'a>(spans: &mut Vec<Span<'a>>, text: &'a str, style: Style) {
    if !text.is_empty() {
        spans.push(Span::styled(text, style));
    }
}

pub(crate) fn build_edit_value_spans<'a>(
    text: &'a str,
    cursor: usize,
    selection: TextSelection,
    value_style: Style,
    cursor_style: Style,
    selection_style: Style,
) -> Vec<Span<'a>> {
    let mut spans = Vec::new();
    let len = char_len(text);
    let cursor = cursor.min(len);

    if let Some((start_raw, end_raw)) = selection {
        let start = start_raw.min(len);
        let end = end_raw.min(len);
        let (start, end) = if start <= end { (start, end) } else { (end, start) };

        if start < end {
            let start_byte = byte_index_for_char(text, start);
            let end_byte = byte_index_for_char(text, end);
            push_if_non_empty(&mut spans, &text[..start_byte], value_style);
            push_if_non_empty(&mut spans, &text[start_byte..end_byte], selection_style);
            push_if_non_empty(&mut spans, &text[end_byte..], value_style);
            return spans;
        }
    }

    if len == 0 {
        spans.push(Span::styled(" ", cursor_style));
        return spans;
    }

    if cursor < len {
        let cursor_start = byte_index_for_char(text, cursor);
        let cursor_end = byte_index_for_char(text, cursor + 1);
        push_if_non_empty(&mut spans, &text[..cursor_start], value_style);
        push_if_non_empty(&mut spans, &text[cursor_start..cursor_end], cursor_style);
        push_if_non_empty(&mut spans, &text[cursor_end..], value_style);
    } else {
        spans.push(Span::styled(text, value_style));
        spans.push(Span::styled(" ", cursor_style));
    }

    spans
}
