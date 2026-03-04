use alacritty_terminal::vte::ansi::{NamedColor, Rgb};
use ratatui::style::Color as UiColor;

pub(crate) use alacritty_terminal::vte::ansi::Color as AnsiColor;

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

pub(super) fn ansi_index_to_rgb(index: u8) -> Rgb {
    match index {
        0 => Rgb { r: 0, g: 0, b: 0 },
        1 => Rgb { r: 205, g: 0, b: 0 },
        2 => Rgb { r: 0, g: 205, b: 0 },
        3 => Rgb { r: 205, g: 205, b: 0 },
        4 => Rgb { r: 0, g: 0, b: 238 },
        5 => Rgb { r: 205, g: 0, b: 205 },
        6 => Rgb { r: 0, g: 205, b: 205 },
        7 => Rgb { r: 229, g: 229, b: 229 },
        8 => Rgb { r: 127, g: 127, b: 127 },
        9 => Rgb { r: 255, g: 0, b: 0 },
        10 => Rgb { r: 0, g: 255, b: 0 },
        11 => Rgb { r: 255, g: 255, b: 0 },
        12 => Rgb { r: 92, g: 92, b: 255 },
        13 => Rgb { r: 255, g: 0, b: 255 },
        14 => Rgb { r: 0, g: 255, b: 255 },
        _ => Rgb { r: 255, g: 255, b: 255 },
    }
}
