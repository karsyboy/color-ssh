//! Shared UI helpers.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(crate) fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub(crate) fn truncate_to_display_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let mut output = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if char_width > 0 && used + char_width > max_width {
            break;
        }
        output.push(ch);
        used += char_width;
    }

    output
}

#[cfg(test)]
mod tests {
    use super::{display_width, truncate_to_display_width};

    #[test]
    fn calculates_display_width_for_wide_glyphs() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("a界b"), 4);
    }

    #[test]
    fn truncates_by_display_width_instead_of_char_count() {
        assert_eq!(truncate_to_display_width("a界b", 1), "a");
        assert_eq!(truncate_to_display_width("a界b", 3), "a界");
        assert_eq!(truncate_to_display_width("a界b", 4), "a界b");
    }
}
