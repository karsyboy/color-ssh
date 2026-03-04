use super::color::AnsiColor;
use super::parser::Parser;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::{Cell as TermCell, Flags};
use unicode_width::UnicodeWidthChar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseProtocolEncoding {
    Default,
    Sgr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseProtocolMode {
    None,
    Press,
    ButtonMotion,
    AnyMotion,
}

pub(crate) struct Screen<'a> {
    pub(super) parser: &'a Parser,
}

impl<'a> Screen<'a> {
    pub(crate) fn size(&self) -> (u16, u16) {
        let grid = self.parser.term.grid();
        (grid.screen_lines() as u16, grid.columns() as u16)
    }

    pub(crate) fn scrollback(&self) -> usize {
        self.parser.max_scrollback()
    }

    pub(crate) fn cursor_position(&self) -> (u16, u16) {
        let point = self.parser.term.grid().cursor.point;
        (point.line.0.max(0) as u16, point.column.0 as u16)
    }

    pub(crate) fn hide_cursor(&self) -> bool {
        !self.parser.term.mode().contains(TermMode::SHOW_CURSOR)
    }

    pub(crate) fn bracketed_paste_enabled(&self) -> bool {
        self.parser.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    pub(crate) fn cell(&self, row: u16, col: u16) -> Option<CellRef<'_>> {
        let grid = self.parser.term.grid();
        if row as usize >= grid.screen_lines() || col as usize >= grid.columns() {
            return None;
        }

        let line = Line(row as i32 - grid.display_offset() as i32);
        if line < grid.topmost_line() || line > grid.bottommost_line() {
            return None;
        }

        let column = Column(col as usize);
        Some(CellRef { cell: &grid[line][column] })
    }

    pub(crate) fn mouse_protocol(&self) -> (MouseProtocolMode, MouseProtocolEncoding) {
        let mode = self.parser.term.mode();
        let mouse_mode = if mode.contains(TermMode::MOUSE_MOTION) {
            MouseProtocolMode::AnyMotion
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            MouseProtocolMode::ButtonMotion
        } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
            MouseProtocolMode::Press
        } else {
            MouseProtocolMode::None
        };

        let encoding = if mode.contains(TermMode::SGR_MOUSE) {
            MouseProtocolEncoding::Sgr
        } else {
            MouseProtocolEncoding::Default
        };

        (mouse_mode, encoding)
    }
}

pub(crate) struct CellRef<'a> {
    cell: &'a TermCell,
}

impl<'a> CellRef<'a> {
    fn is_renderable_primary_char(ch: char) -> bool {
        ch != ' ' && !ch.is_control()
    }

    fn is_renderable_zero_width(ch: char) -> bool {
        !ch.is_control() && ch.width().unwrap_or(0) == 0
    }

    pub(crate) fn has_contents(&self) -> bool {
        !self.cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) && Self::is_renderable_primary_char(self.cell.c)
    }

    pub(crate) fn contents(&self) -> String {
        if self.cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            return " ".to_string();
        }
        if !Self::is_renderable_primary_char(self.cell.c) {
            return " ".to_string();
        }

        let mut out = String::new();
        out.push(self.cell.c);
        if let Some(zerowidth) = self.cell.zerowidth() {
            for c in zerowidth {
                if Self::is_renderable_zero_width(*c) {
                    out.push(*c);
                }
            }
        }
        out
    }

    pub(crate) fn fgcolor(&self) -> AnsiColor {
        self.cell.fg
    }

    pub(crate) fn bgcolor(&self) -> AnsiColor {
        self.cell.bg
    }

    pub(crate) fn bold(&self) -> bool {
        self.cell.flags.intersects(Flags::BOLD | Flags::DIM_BOLD)
    }

    pub(crate) fn italic(&self) -> bool {
        self.cell.flags.contains(Flags::ITALIC)
    }

    pub(crate) fn underline(&self) -> bool {
        self.cell.flags.intersects(Flags::ALL_UNDERLINES)
    }

    pub(crate) fn inverse(&self) -> bool {
        self.cell.flags.contains(Flags::INVERSE)
    }
}
