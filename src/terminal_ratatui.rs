//! Ratatui adapter for terminal viewport snapshots.
//!
//! `terminal_core` remains renderer-neutral. This module is the current
//! ratatui-specific bridge that paints backend-neutral viewport data into a
//! ratatui buffer. A future GUI renderer should consume the same viewport model
//! directly instead of re-reading terminal internals.

use crate::terminal_core::highlight_overlay::HighlightOverlayStyle;
use crate::terminal_core::{AnsiColor, TerminalCellSnapshot, TerminalViewport};
use alacritty_terminal::vte::ansi::NamedColor;
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
};

pub(crate) fn paint_terminal_viewport<F>(
    buffer: &mut Buffer,
    area: Rect,
    viewport: &TerminalViewport,
    show_cursor: bool,
    mut style_for_cell: F,
) -> Option<Position>
where
    F: FnMut(i64, u16, &TerminalCellSnapshot, bool, Style) -> Style,
{
    clear_area(buffer, area);

    let mut cursor_position = None;
    let cursor = viewport.cursor();
    let (viewport_rows, viewport_cols) = viewport.size();
    let mut glyph_scratch = String::new();

    for (visible_row, row) in viewport.rows().iter().enumerate() {
        let visible_row = visible_row as u16;
        if visible_row >= area.height || visible_row >= viewport_rows {
            break;
        }

        for (col_idx, cell) in row.cells().iter().enumerate() {
            let col = col_idx as u16;
            if col >= area.width || col >= viewport_cols {
                break;
            }

            let is_cursor = show_cursor && cursor.is_some_and(|cursor| cursor.row() == visible_row && cursor.col() == col);
            let style = style_for_cell(row.absolute_row(), col, cell, is_cursor, base_style_for_cell(cell));
            let buf_cell = &mut buffer[(area.x + col, area.y + visible_row)];
            buf_cell.set_symbol(cell.glyph().as_str(&mut glyph_scratch));
            buf_cell.set_style(style);

            if is_cursor {
                cursor_position = Some(Position::new(area.x + col, area.y + visible_row));
            }
        }
    }

    cursor_position
}

fn clear_area(buffer: &mut Buffer, area: Rect) {
    for row in area.y..area.y.saturating_add(area.height) {
        for col in area.x..area.x.saturating_add(area.width) {
            let cell = &mut buffer[(col, row)];
            cell.set_symbol(" ");
            cell.set_style(Style::default());
        }
    }
}

fn base_style_for_cell(cell: &TerminalCellSnapshot) -> Style {
    let mut fg_color = to_ratatui_color(cell.fg_color());
    let mut bg_color = to_ratatui_background_color(cell.bg_color());

    if cell.inverse() {
        std::mem::swap(&mut fg_color, &mut bg_color);
        if fg_color == Color::Reset {
            fg_color = Color::Black;
        }
        if bg_color == Color::Reset {
            bg_color = Color::Indexed(15);
        }
    }

    let mut style = Style::default();
    if fg_color != Color::Reset {
        style = style.fg(fg_color);
    }
    if bg_color != Color::Reset {
        style = style.bg(bg_color);
    }
    if cell.bold() {
        style = style.add_modifier(Modifier::BOLD);
    }
    if cell.italic() {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if cell.underline() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

pub(crate) fn apply_overlay_style(mut base_style: Style, overlay_style: &HighlightOverlayStyle) -> Style {
    if let Some(fg_color) = overlay_style.fg_color() {
        let fg_color = to_ratatui_color(fg_color);
        if fg_color != Color::Reset {
            base_style = base_style.fg(fg_color);
        }
    }

    if let Some(bg_color) = overlay_style.bg_color() {
        let bg_color = to_ratatui_background_color(bg_color);
        if bg_color != Color::Reset {
            base_style = base_style.bg(bg_color);
        }
    }

    if overlay_style.bold() {
        base_style = base_style.add_modifier(Modifier::BOLD);
    }
    if overlay_style.italic() {
        base_style = base_style.add_modifier(Modifier::ITALIC);
    }
    if overlay_style.underline() {
        base_style = base_style.add_modifier(Modifier::UNDERLINED);
    }

    base_style
}

fn to_ratatui_color(color: AnsiColor) -> Color {
    match color {
        AnsiColor::Named(named) => named_color_to_ansi_index(named).map(Color::Indexed).unwrap_or(Color::Reset),
        AnsiColor::Indexed(idx) => Color::Indexed(idx),
        AnsiColor::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

fn to_ratatui_background_color(color: AnsiColor) -> Color {
    match color {
        AnsiColor::Named(named) => named_color_to_ansi_index(named).map(Color::Indexed).unwrap_or(Color::Reset),
        AnsiColor::Indexed(idx) => Color::Indexed(idx),
        AnsiColor::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
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
