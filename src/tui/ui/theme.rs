//! Shared UI helpers and session-manager theme loading.

use crate::{log_info, log_warn};
use once_cell::sync::OnceCell;
use ratatui::style::Color;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const THEME_FILE_NAME: &str = "cossh-theme.toml";

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnsiPalette {
    pub(crate) black: Color,
    pub(crate) red: Color,
    pub(crate) green: Color,
    pub(crate) yellow: Color,
    pub(crate) blue: Color,
    pub(crate) magenta: Color,
    pub(crate) cyan: Color,
    pub(crate) white: Color,
    pub(crate) bright_black: Color,
    pub(crate) bright_red: Color,
    pub(crate) bright_green: Color,
    pub(crate) bright_yellow: Color,
    pub(crate) bright_blue: Color,
    pub(crate) bright_magenta: Color,
    pub(crate) bright_cyan: Color,
    pub(crate) bright_white: Color,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionTheme {
    pub(crate) theme_name: String,
    pub(crate) ansi: AnsiPalette,
    pub(crate) tab_active_bg: Color,
    pub(crate) tab_inactive_bg: Color,
    pub(crate) selection_fg: Color,
    pub(crate) selection_bg: Color,
}

impl Default for SessionTheme {
    fn default() -> Self {
        // One Dark ANSI palette (matches an AnsiColor theme preset).
        let ansi = AnsiPalette {
            black: Color::Rgb(0x28, 0x2c, 0x34),
            red: Color::Rgb(0xe0, 0x6c, 0x75),
            green: Color::Rgb(0x98, 0xc3, 0x79),
            yellow: Color::Rgb(0xe5, 0xc0, 0x7b),
            blue: Color::Rgb(0x61, 0xaf, 0xef),
            magenta: Color::Rgb(0xc6, 0x78, 0xdd),
            cyan: Color::Rgb(0x56, 0xb6, 0xc2),
            white: Color::Rgb(0xab, 0xb2, 0xbf),
            bright_black: Color::Rgb(0x5c, 0x63, 0x70),
            bright_red: Color::Rgb(0xe0, 0x6c, 0x75),
            bright_green: Color::Rgb(0x98, 0xc3, 0x79),
            bright_yellow: Color::Rgb(0xe5, 0xc0, 0x7b),
            bright_blue: Color::Rgb(0x61, 0xaf, 0xef),
            bright_magenta: Color::Rgb(0xc6, 0x78, 0xdd),
            bright_cyan: Color::Rgb(0x56, 0xb6, 0xc2),
            bright_white: Color::Rgb(0xff, 0xff, 0xff),
        };

        Self {
            theme_name: "One Dark".to_string(),
            ansi,
            tab_active_bg: Color::Rgb(0x44, 0x44, 0x44),
            tab_inactive_bg: Color::Rgb(0x30, 0x30, 0x30),
            selection_fg: Color::Rgb(0xff, 0xff, 0xff),
            selection_bg: Color::Rgb(0x61, 0xaf, 0xef),
        }
    }
}

#[derive(Debug, Default)]
struct AlacrittyThemeConfig {
    theme_name: Option<String>,
    foreground: Option<String>,
    selection_text: Option<String>,
    background: Option<String>,
    selection_background: Option<String>,
    normal_palette: [Option<String>; 8],
    bright_palette: [Option<String>; 8],
}

static SESSION_THEME: OnceCell<SessionTheme> = OnceCell::new();

pub(crate) fn session_theme() -> &'static SessionTheme {
    SESSION_THEME.get_or_init(load_theme_or_default)
}

pub(crate) fn ansi_black() -> Color {
    session_theme().ansi.black
}

pub(crate) fn ansi_red() -> Color {
    session_theme().ansi.red
}

pub(crate) fn ansi_green() -> Color {
    session_theme().ansi.green
}

pub(crate) fn ansi_yellow() -> Color {
    session_theme().ansi.yellow
}

pub(crate) fn ansi_magenta() -> Color {
    session_theme().ansi.magenta
}

pub(crate) fn ansi_cyan() -> Color {
    session_theme().ansi.cyan
}

pub(crate) fn ansi_white() -> Color {
    session_theme().ansi.white
}

pub(crate) fn ansi_bright_black() -> Color {
    session_theme().ansi.bright_black
}

pub(crate) fn ansi_bright_red() -> Color {
    session_theme().ansi.bright_red
}

pub(crate) fn ansi_bright_cyan() -> Color {
    session_theme().ansi.bright_cyan
}

pub(crate) fn ansi_bright_white() -> Color {
    session_theme().ansi.bright_white
}

pub(crate) fn tab_active_bg() -> Color {
    session_theme().tab_active_bg
}

pub(crate) fn tab_inactive_bg() -> Color {
    session_theme().tab_inactive_bg
}

pub(crate) fn selection_fg() -> Color {
    session_theme().selection_fg
}

pub(crate) fn selection_bg() -> Color {
    session_theme().selection_bg
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

fn load_theme_or_default() -> SessionTheme {
    match load_theme_file() {
        Ok(theme) => {
            log_info!("Loaded TUI theme '{}' from ~/.color-ssh/{}", theme.theme_name, THEME_FILE_NAME);
            theme
        }
        Err(err) => {
            log_warn!("Failed to load TUI theme ({}), using built-in defaults", err);
            SessionTheme::default()
        }
    }
}

fn load_theme_file() -> io::Result<SessionTheme> {
    let theme_path = theme_file_path()?;
    ensure_theme_file_exists(&theme_path)?;
    let theme_content = fs::read_to_string(&theme_path)?;
    parse_alacritty_theme(&theme_content)
}

#[derive(Debug, Clone, Copy, Default)]
enum AlacrittySection {
    #[default]
    Other,
    ColorsPrimary,
    ColorsSelection,
    ColorsNormal,
    ColorsBright,
}

fn parse_alacritty_theme(content: &str) -> io::Result<SessionTheme> {
    let mut parsed = AlacrittyThemeConfig::default();
    let mut current_section = AlacrittySection::Other;

    for (line_idx, raw_line) in content.lines().enumerate() {
        let line_number = line_idx + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            current_section = match line {
                "[colors.primary]" => AlacrittySection::ColorsPrimary,
                "[colors.selection]" => AlacrittySection::ColorsSelection,
                "[colors.normal]" => AlacrittySection::ColorsNormal,
                "[colors.bright]" => AlacrittySection::ColorsBright,
                _ => AlacrittySection::Other,
            };
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            log_warn!("Ignoring malformed theme line {}: '{}'", line_number, line);
            continue;
        };

        let key = raw_key.trim();
        let value = normalize_value(raw_value);
        if value.is_empty() {
            continue;
        }

        match current_section {
            AlacrittySection::ColorsPrimary => match key {
                "foreground" => parsed.foreground = Some(value),
                "background" => parsed.background = Some(value),
                _ => {}
            },
            AlacrittySection::ColorsSelection => match key {
                "background" => parsed.selection_background = Some(value),
                "text" => parsed.selection_text = Some(value),
                _ => {}
            },
            AlacrittySection::ColorsNormal => {
                if let Some(index) = color_name_to_index(key) {
                    parsed.normal_palette[index] = Some(value);
                }
            }
            AlacrittySection::ColorsBright => {
                if let Some(index) = color_name_to_index(key) {
                    parsed.bright_palette[index] = Some(value);
                }
            }
            AlacrittySection::Other => {
                if matches!(key, "theme" | "theme_name" | "name") {
                    parsed.theme_name = Some(value);
                }
            }
        }
    }

    let mut theme = SessionTheme::default();
    if let Some(theme_name) = parsed.theme_name.as_deref()
        && !theme_name.trim().is_empty()
    {
        theme.theme_name = theme_name.trim().to_string();
    } else {
        theme.theme_name = "Alacritty Theme".to_string();
    }

    for (index, color_value) in parsed.normal_palette.iter().enumerate() {
        let Some(color_value) = color_value else {
            continue;
        };
        match resolve_color_token(color_value, &theme.ansi) {
            Some(color) => set_ansi_by_index(&mut theme.ansi, index, color),
            None => {
                log_warn!("Ignoring invalid normal color '{}' at index {}", color_value, index);
            }
        }
    }

    for (index, color_value) in parsed.bright_palette.iter().enumerate() {
        let Some(color_value) = color_value else {
            continue;
        };
        match resolve_color_token(color_value, &theme.ansi) {
            Some(color) => set_ansi_by_index(&mut theme.ansi, index + 8, color),
            None => {
                log_warn!("Ignoring invalid bright color '{}' at index {}", color_value, index);
            }
        }
    }

    if let Some(background) = parsed.background.as_deref()
        && let Some(color) = resolve_color_token(background, &theme.ansi)
    {
        theme.tab_inactive_bg = color;
    }

    if let Some(selection_text) = parsed.selection_text.as_deref()
        && let Some(color) = resolve_color_token(selection_text, &theme.ansi)
    {
        theme.selection_fg = color;
    }

    if let Some(selection_background) = parsed.selection_background.as_deref()
        && let Some(color) = resolve_color_token(selection_background, &theme.ansi)
    {
        theme.selection_bg = color;
    }

    theme.tab_active_bg = theme.ansi.bright_black;

    // Keep tab contrast readable when a theme sets identical values.
    if theme.tab_active_bg == theme.tab_inactive_bg {
        theme.tab_active_bg = theme.selection_bg;
    }

    // Foreground is optional in Alacritty themes; when present use it as ANSI white fallback.
    if let Some(foreground) = parsed.foreground.as_deref()
        && let Some(color) = resolve_color_token(foreground, &theme.ansi)
    {
        theme.ansi.white = color;
    }

    Ok(theme)
}

fn normalize_value(raw_value: &str) -> String {
    let value = raw_value.trim();
    if value.len() >= 2 && ((value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\''))) {
        value[1..value.len() - 1].trim().to_string()
    } else {
        value.to_string()
    }
}

fn color_name_to_index(name: &str) -> Option<usize> {
    match name.trim() {
        "black" => Some(0),
        "red" => Some(1),
        "green" => Some(2),
        "yellow" => Some(3),
        "blue" => Some(4),
        "magenta" => Some(5),
        "cyan" => Some(6),
        "white" => Some(7),
        _ => None,
    }
}

fn set_ansi_by_index(palette: &mut AnsiPalette, index: usize, color: Color) {
    match index {
        0 => palette.black = color,
        1 => palette.red = color,
        2 => palette.green = color,
        3 => palette.yellow = color,
        4 => palette.blue = color,
        5 => palette.magenta = color,
        6 => palette.cyan = color,
        7 => palette.white = color,
        8 => palette.bright_black = color,
        9 => palette.bright_red = color,
        10 => palette.bright_green = color,
        11 => palette.bright_yellow = color,
        12 => palette.bright_blue = color,
        13 => palette.bright_magenta = color,
        14 => palette.bright_cyan = color,
        15 => palette.bright_white = color,
        _ => {}
    }
}

fn theme_file_path() -> io::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Unable to resolve home directory"))?;
    Ok(home.join(".color-ssh").join(THEME_FILE_NAME))
}

fn ensure_theme_file_exists(path: &Path) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }

    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Theme path does not have a parent directory"))?;

    fs::create_dir_all(parent)?;
    let template = include_str!("../../../templates/cossh-theme.toml");
    fs::write(path, template)?;
    log_info!("Created default TUI theme file at {:?}", path);
    Ok(())
}

fn resolve_color_token(value: &str, palette: &AnsiPalette) -> Option<Color> {
    if let Some(hex_color) = parse_hex_color(value) {
        return Some(hex_color);
    }

    let token = value.trim().to_ascii_lowercase();
    match token.as_str() {
        "black" => Some(palette.black),
        "red" => Some(palette.red),
        "green" => Some(palette.green),
        "yellow" => Some(palette.yellow),
        "blue" => Some(palette.blue),
        "magenta" => Some(palette.magenta),
        "cyan" => Some(palette.cyan),
        "white" | "gray" | "grey" => Some(palette.white),
        "bright_black" | "dark_gray" | "dark_grey" => Some(palette.bright_black),
        "bright_red" => Some(palette.bright_red),
        "bright_green" => Some(palette.bright_green),
        "bright_yellow" => Some(palette.bright_yellow),
        "bright_blue" => Some(palette.bright_blue),
        "bright_magenta" => Some(palette.bright_magenta),
        "bright_cyan" => Some(palette.bright_cyan),
        "bright_white" | "light_gray" | "light_grey" => Some(palette.bright_white),
        _ => None,
    }
}

fn parse_hex_color(value: &str) -> Option<Color> {
    let normalized = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if normalized.len() != 6 || !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }

    let r = u8::from_str_radix(&normalized[0..2], 16).ok()?;
    let g = u8::from_str_radix(&normalized[2..4], 16).ok()?;
    let b = u8::from_str_radix(&normalized[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

#[cfg(test)]
mod tests {
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
}
