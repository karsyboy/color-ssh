use super::{AnsiPalette, color_name_to_index, display_width, parse_alacritty_theme, parse_hex_color, resolve_color_token, truncate_to_display_width};
use ratatui::style::Color;

fn palette() -> AnsiPalette {
    AnsiPalette {
        black: Color::Rgb(0x01, 0x01, 0x01),
        red: Color::Rgb(0x02, 0x02, 0x02),
        green: Color::Rgb(0x03, 0x03, 0x03),
        yellow: Color::Rgb(0x04, 0x04, 0x04),
        blue: Color::Rgb(0x05, 0x05, 0x05),
        magenta: Color::Rgb(0x06, 0x06, 0x06),
        cyan: Color::Rgb(0x07, 0x07, 0x07),
        white: Color::Rgb(0x08, 0x08, 0x08),
        bright_black: Color::Rgb(0x09, 0x09, 0x09),
        bright_red: Color::Rgb(0x0a, 0x0a, 0x0a),
        bright_green: Color::Rgb(0x0b, 0x0b, 0x0b),
        bright_yellow: Color::Rgb(0x0c, 0x0c, 0x0c),
        bright_blue: Color::Rgb(0x0d, 0x0d, 0x0d),
        bright_magenta: Color::Rgb(0x0e, 0x0e, 0x0e),
        bright_cyan: Color::Rgb(0x0f, 0x0f, 0x0f),
        bright_white: Color::Rgb(0x10, 0x10, 0x10),
    }
}

#[test]
fn parses_hex_color_tokens() {
    assert_eq!(parse_hex_color("#a1b2c3"), Some(Color::Rgb(0xa1, 0xb2, 0xc3)));
    assert_eq!(parse_hex_color("a1b2c3"), Some(Color::Rgb(0xa1, 0xb2, 0xc3)));
    assert_eq!(parse_hex_color("#zzz999"), None);
    assert_eq!(parse_hex_color("#12345"), None);
}

#[test]
fn resolves_palette_color_names() {
    let palette = palette();
    assert_eq!(resolve_color_token("cyan", &palette), Some(palette.cyan));
    assert_eq!(resolve_color_token("bright_cyan", &palette), Some(palette.bright_cyan));
    assert_eq!(resolve_color_token("dark_gray", &palette), Some(palette.bright_black));
    assert_eq!(resolve_color_token("unknown", &palette), None);
}

#[test]
fn maps_color_names_to_ansi_indices() {
    assert_eq!(color_name_to_index("black"), Some(0));
    assert_eq!(color_name_to_index("white"), Some(7));
    assert_eq!(color_name_to_index("invalid"), None);
}

#[test]
fn parses_alacritty_theme_palette_and_surfaces() {
    let theme = parse_alacritty_theme(
        r##"
[colors.primary]
foreground = "#e1e3e4"
background = "#101420"

[colors.selection]
text = "#c0c0c0"
background = "#1f2440"

[colors.normal]
black = "#0a0e1e"
red = "#ff0000"
green = "#00ff00"
yellow = "#ffff00"
blue = "#0000ff"
magenta = "#ff00ff"
cyan = "#00e8c6"
white = "#aaaaaa"

[colors.bright]
black = "#222222"
red = "#ff5555"
green = "#55ff55"
yellow = "#ffff55"
blue = "#5555ff"
magenta = "#ff55ff"
cyan = "#55ffff"
white = "#ffffff"
"##,
    )
    .expect("parse alacritty theme");

    assert_eq!(theme.theme_name, "Alacritty Theme");
    assert_eq!(theme.ansi.black, Color::Rgb(0x0a, 0x0e, 0x1e));
    assert_eq!(theme.ansi.red, Color::Rgb(0xff, 0x00, 0x00));
    assert_eq!(theme.ansi.cyan, Color::Rgb(0x00, 0xe8, 0xc6));
    assert_eq!(theme.ansi.bright_red, Color::Rgb(0xff, 0x55, 0x55));
    assert_eq!(theme.ansi.bright_white, Color::Rgb(0xff, 0xff, 0xff));
    assert_eq!(theme.tab_inactive_bg, Color::Rgb(0x10, 0x14, 0x20));
    assert_eq!(theme.tab_active_bg, Color::Rgb(0x22, 0x22, 0x22));
    assert_eq!(theme.selection_fg, Color::Rgb(0xc0, 0xc0, 0xc0));
    assert_eq!(theme.selection_bg, Color::Rgb(0x1f, 0x24, 0x40));
    assert_eq!(theme.ansi.white, Color::Rgb(0xe1, 0xe3, 0xe4));
}

#[test]
fn ignores_unsupported_or_malformed_alacritty_lines() {
    let theme = parse_alacritty_theme(
        r##"
not-a-setting
[colors.normal]
cursor-color = "#ff00ff"
red = "#ff0000"
bad-entry
"##,
    )
    .expect("parse alacritty theme");

    assert_eq!(theme.ansi.red, Color::Rgb(0xff, 0x00, 0x00));
}

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
