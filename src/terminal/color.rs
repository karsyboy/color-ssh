//! Terminal-color primitives shared by the engine and view model.

use alacritty_terminal::vte::ansi::Rgb;

pub(crate) use alacritty_terminal::vte::ansi::Color as AnsiColor;

/// Map an ANSI 16-color palette index to RGB.
///
/// The embedded engine uses this when responding to terminal color queries from
/// remote applications without coupling the core to any specific renderer.
pub(crate) fn ansi_index_to_rgb(index: u8) -> Rgb {
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
