//! Ratatui color adapter for the embedded terminal core.
//!
//! This file stays TUI-specific on purpose. `terminal_core` exposes terminal
//! colors in a renderer-neutral form and the TUI maps them into ratatui styles
//! here. A future GUI can add its own adapter without changing the engine.

use crate::terminal_core::AnsiColor;
use alacritty_terminal::vte::ansi::NamedColor;
use ratatui::style::Color as UiColor;

pub(crate) fn to_ratatui_color(color: AnsiColor) -> UiColor {
    match color {
        AnsiColor::Named(named) => named_color_to_ansi_index(named).map(UiColor::Indexed).unwrap_or(UiColor::Reset),
        AnsiColor::Indexed(idx) => UiColor::Indexed(idx),
        AnsiColor::Spec(rgb) => UiColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

pub(crate) fn to_ratatui_background_color(color: AnsiColor) -> UiColor {
    match color {
        AnsiColor::Named(named) => named_color_to_ansi_index(named).map(UiColor::Indexed).unwrap_or(UiColor::Reset),
        AnsiColor::Indexed(idx) => UiColor::Indexed(idx),
        AnsiColor::Spec(rgb) => UiColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

fn named_color_to_ansi_index(named: NamedColor) -> Option<u8> {
    match named {
        NamedColor::Black | NamedColor::DimBlack => Some(0),
        NamedColor::Red | NamedColor::DimRed => Some(1),
        NamedColor::Green | NamedColor::DimGreen => Some(2),
        NamedColor::Yellow | NamedColor::DimYellow => Some(3),
        NamedColor::Blue | NamedColor::DimBlue => Some(4),
        NamedColor::Magenta | NamedColor::DimMagenta => Some(5),
        NamedColor::Cyan | NamedColor::DimCyan => Some(6),
        NamedColor::White | NamedColor::DimWhite => Some(7),
        NamedColor::BrightBlack | NamedColor::DimForeground => Some(8),
        NamedColor::BrightRed => Some(9),
        NamedColor::BrightGreen => Some(10),
        NamedColor::BrightYellow => Some(11),
        NamedColor::BrightBlue => Some(12),
        NamedColor::BrightMagenta => Some(13),
        NamedColor::BrightCyan => Some(14),
        NamedColor::BrightWhite | NamedColor::BrightForeground => Some(15),
        NamedColor::Foreground | NamedColor::Background | NamedColor::Cursor => None,
    }
}
