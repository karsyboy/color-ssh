//! Shared UI helpers that map directly to terminal ANSI colors.
//!
//! This intentionally avoids loading any custom color theme files.
//! The TUI now uses the active terminal theme for ANSI color rendering.

use ratatui::style::Color;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const ANSI_BLACK: u8 = 0;
const ANSI_RED: u8 = 1;
const ANSI_GREEN: u8 = 2;
const ANSI_YELLOW: u8 = 3;
const ANSI_MAGENTA: u8 = 5;
const ANSI_CYAN: u8 = 6;
const ANSI_WHITE: u8 = 7;
const ANSI_BRIGHT_BLACK: u8 = 8;
const ANSI_BRIGHT_RED: u8 = 9;
const ANSI_BRIGHT_CYAN: u8 = 14;
const ANSI_BRIGHT_WHITE: u8 = 15;

#[inline]
const fn ansi(index: u8) -> Color {
    Color::Indexed(index)
}

pub(crate) fn ansi_black() -> Color {
    ansi(ANSI_BLACK)
}

pub(crate) fn ansi_red() -> Color {
    ansi(ANSI_RED)
}

pub(crate) fn ansi_green() -> Color {
    ansi(ANSI_GREEN)
}

pub(crate) fn ansi_yellow() -> Color {
    ansi(ANSI_YELLOW)
}

pub(crate) fn ansi_magenta() -> Color {
    ansi(ANSI_MAGENTA)
}

pub(crate) fn ansi_cyan() -> Color {
    ansi(ANSI_CYAN)
}

pub(crate) fn ansi_white() -> Color {
    ansi(ANSI_WHITE)
}

pub(crate) fn ansi_bright_black() -> Color {
    ansi(ANSI_BRIGHT_BLACK)
}

pub(crate) fn ansi_bright_red() -> Color {
    ansi(ANSI_BRIGHT_RED)
}

pub(crate) fn ansi_bright_cyan() -> Color {
    ansi(ANSI_BRIGHT_CYAN)
}

pub(crate) fn ansi_bright_white() -> Color {
    ansi(ANSI_BRIGHT_WHITE)
}

pub(crate) fn tab_active_bg() -> Color {
    ansi_bright_black()
}

pub(crate) fn tab_inactive_bg() -> Color {
    Color::Reset
}

pub(crate) fn selection_fg() -> Color {
    ansi_black()
}

pub(crate) fn selection_bg() -> Color {
    ansi_cyan()
}

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
#[path = "../../test/tui/ui/theme.rs"]
mod tests;
