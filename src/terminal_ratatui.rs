//! Ratatui adapter for terminal viewport snapshots.
//!
//! `terminal_core` remains renderer-neutral. This module is the current
//! ratatui-specific bridge that paints backend-neutral viewport data into a
//! ratatui buffer. A future GUI renderer should consume the same viewport model
//! directly instead of re-reading terminal internals. It is responsible only
//! for presentation: it combines canonical cell styles with additive overlay
//! styles and never writes back into the PTY or terminal engine.

use crate::terminal_core::highlight_overlay::{HighlightCellRange, HighlightOverlayStyle};
use crate::terminal_core::{AnsiColor, TerminalCellSnapshot, TerminalViewport};
use alacritty_terminal::vte::ansi::NamedColor;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

const RELOAD_NOTICE_TOAST_MAX_WIDTH: u16 = 60;
const RELOAD_NOTICE_TOAST_MARGIN: u16 = 1;

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

pub(crate) fn render_reload_notice_toast(frame: &mut Frame, area: Rect, message: &str) {
    let toast_area = reload_notice_toast_area(area, message);
    if toast_area.width == 0 || toast_area.height == 0 {
        return;
    }

    let block_style = Style::default().fg(Color::Indexed(6)).bg(Color::Indexed(8));
    let text_style = Style::default().fg(Color::Indexed(15)).bg(Color::Indexed(8));
    let block = Block::default().borders(Borders::ALL).border_style(block_style).style(block_style);
    let inner = block.inner(toast_area);

    frame.render_widget(Clear, toast_area);
    frame.render_widget(block, toast_area);
    frame.render_widget(Paragraph::new(message.to_string()).style(text_style).wrap(Wrap { trim: false }), inner);
}

fn reload_notice_toast_area(area: Rect, message: &str) -> Rect {
    if area.width < 3 || area.height < 3 || message.is_empty() {
        return Rect::default();
    }

    let available_width = area.width.saturating_sub(RELOAD_NOTICE_TOAST_MARGIN.saturating_mul(2));
    let available_height = area.height.saturating_sub(RELOAD_NOTICE_TOAST_MARGIN.saturating_mul(2));
    if available_width < 3 || available_height < 3 {
        return Rect::default();
    }

    let max_outer_width = available_width.clamp(3, RELOAD_NOTICE_TOAST_MAX_WIDTH);
    let desired_outer_width = message.lines().map(UnicodeWidthStr::width).max().unwrap_or(1).saturating_add(2) as u16;
    let toast_width = desired_outer_width.clamp(3, max_outer_width);
    let inner_width = toast_width.saturating_sub(2).max(1) as usize;
    let wrapped_line_count = message
        .lines()
        .map(|line| UnicodeWidthStr::width(line).max(1).div_ceil(inner_width).max(1))
        .sum::<usize>()
        .max(1) as u16;
    let toast_height = wrapped_line_count.saturating_add(2).min(available_height).max(3);
    let x = area.x + area.width.saturating_sub(toast_width + RELOAD_NOTICE_TOAST_MARGIN);
    let y = area.y + area.height.saturating_sub(toast_height + RELOAD_NOTICE_TOAST_MARGIN);

    Rect::new(x, y, toast_width, toast_height)
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

pub(crate) fn apply_overlay_ranges(base_style: Style, row_ranges: Option<&[HighlightCellRange]>, styles: &[HighlightOverlayStyle], col: u16) -> Style {
    let Some(row_ranges) = row_ranges else {
        return base_style;
    };

    let range_idx = row_ranges.partition_point(|range| range.end_col <= col);
    let Some(range) = row_ranges.get(range_idx) else {
        return base_style;
    };
    if col < range.start_col || col >= range.end_col {
        return base_style;
    }

    let Some(overlay_style) = styles.get(range.style_index()) else {
        return base_style;
    };

    apply_overlay_style(base_style, overlay_style)
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

#[cfg(test)]
#[path = "test/terminal_ratatui.rs"]
mod tests;
