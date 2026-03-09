//! Renderer-facing terminal data extraction.
//!
//! `TerminalViewModel` is intentionally read-only. Renderers should consume
//! terminal state through this view instead of reaching into PTY streams or the
//! underlying `alacritty_terminal::Term` directly.

use super::color::AnsiColor;
use super::engine::TerminalEngine;
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

pub(crate) struct TerminalViewModel<'a> {
    pub(super) engine: &'a TerminalEngine,
}

impl<'a> TerminalViewModel<'a> {
    /// Return the visible terminal size in rows and columns.
    pub(crate) fn size(&self) -> (u16, u16) {
        let grid = self.engine.term.grid();
        (grid.screen_lines() as u16, grid.columns() as u16)
    }

    /// Return the current maximum scrollback depth.
    pub(crate) fn scrollback(&self) -> usize {
        self.engine.max_scrollback()
    }

    /// Return the current cursor position in visible-grid coordinates.
    pub(crate) fn cursor_position(&self) -> (u16, u16) {
        let point = self.engine.term.grid().cursor.point;
        (point.line.0.max(0) as u16, point.column.0 as u16)
    }

    /// Whether the cursor should be hidden for the active terminal mode.
    pub(crate) fn cursor_hidden(&self) -> bool {
        !self.engine.term.mode().contains(TermMode::SHOW_CURSOR)
    }

    /// Transitional alias retained while existing TUI code still calls `hide_cursor`.
    pub(crate) fn hide_cursor(&self) -> bool {
        self.cursor_hidden()
    }

    /// Whether bracketed paste is enabled by the remote application.
    pub(crate) fn bracketed_paste_enabled(&self) -> bool {
        self.engine.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Convert a visible row index into an absolute terminal line index.
    #[allow(dead_code)]
    pub(crate) fn absolute_row(&self, row: u16) -> Option<i64> {
        let grid = self.engine.term.grid();
        if row as usize >= grid.screen_lines() {
            return None;
        }

        let line = Line(row as i32 - grid.display_offset() as i32);
        (line >= grid.topmost_line() && line <= grid.bottommost_line()).then_some(line.0 as i64)
    }

    /// Return renderer-facing cells for the visible grid.
    pub(crate) fn cell(&self, row: u16, col: u16) -> Option<TerminalCellView<'_>> {
        let grid = self.engine.term.grid();
        if row as usize >= grid.screen_lines() || col as usize >= grid.columns() {
            return None;
        }

        let line = Line(row as i32 - grid.display_offset() as i32);
        if line < grid.topmost_line() || line > grid.bottommost_line() {
            return None;
        }

        let column = Column(col as usize);
        Some(TerminalCellView { cell: &grid[line][column] })
    }

    /// Extract a visible row as display text using terminal-cell semantics.
    #[allow(dead_code)]
    pub(crate) fn visible_row_text(&self, row: u16) -> Option<String> {
        let (_, cols) = self.size();
        let _ = self.absolute_row(row)?;

        let mut line = String::new();
        let mut scratch = String::new();
        for col in 0..cols {
            let Some(cell) = self.cell(row, col) else {
                continue;
            };
            line.push_str(cell.symbol(&mut scratch));
        }
        Some(line)
    }

    /// Return the visible rows as `(absolute_line, text)` tuples.
    ///
    /// This is the extraction point future overlay engines and GUI adapters
    /// should consume instead of parsing raw PTY output again.
    #[allow(dead_code)]
    pub(crate) fn visible_row_texts(&self) -> Vec<(i64, String)> {
        let (rows, _) = self.size();
        let mut visible_rows = Vec::with_capacity(rows as usize);
        for row in 0..rows {
            if let (Some(absolute_row), Some(text)) = (self.absolute_row(row), self.visible_row_text(row)) {
                visible_rows.push((absolute_row, text));
            }
        }
        visible_rows
    }

    /// Return the active mouse reporting mode as seen by the terminal.
    pub(crate) fn mouse_protocol(&self) -> (MouseProtocolMode, MouseProtocolEncoding) {
        let mode = self.engine.term.mode();
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

    /// Extract text for an arbitrary terminal-coordinate selection.
    pub(crate) fn selection_text(&self, start: (i64, u16), end: (i64, u16)) -> String {
        self.engine.selection_text(start, end)
    }
}

pub(crate) struct TerminalCellView<'a> {
    cell: &'a TermCell,
}

impl<'a> TerminalCellView<'a> {
    fn is_renderable_primary_char(ch: char) -> bool {
        ch != ' ' && !ch.is_control()
    }

    fn is_renderable_zero_width(ch: char) -> bool {
        !ch.is_control() && ch.width().unwrap_or(0) == 0
    }

    #[cfg(test)]
    pub(crate) fn has_contents(&self) -> bool {
        !self.cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) && Self::is_renderable_primary_char(self.cell.c)
    }

    /// Return the display symbol for a terminal cell.
    pub(crate) fn symbol<'b>(&self, scratch: &'b mut String) -> &'b str {
        if self.cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            return " ";
        }
        if !Self::is_renderable_primary_char(self.cell.c) {
            return " ";
        }

        scratch.clear();
        scratch.push(self.cell.c);
        if let Some(zerowidth) = self.cell.zerowidth() {
            for c in zerowidth {
                if Self::is_renderable_zero_width(*c) {
                    scratch.push(*c);
                }
            }
        }
        scratch.as_str()
    }

    #[cfg(test)]
    pub(crate) fn contents(&self) -> String {
        let mut scratch = String::new();
        self.symbol(&mut scratch).to_owned()
    }

    pub(crate) fn fg_color(&self) -> AnsiColor {
        self.cell.fg
    }

    /// Transitional alias retained while existing TUI code still calls `fgcolor`.
    pub(crate) fn fgcolor(&self) -> AnsiColor {
        self.fg_color()
    }

    pub(crate) fn bg_color(&self) -> AnsiColor {
        self.cell.bg
    }

    /// Transitional alias retained while existing TUI code still calls `bgcolor`.
    pub(crate) fn bgcolor(&self) -> AnsiColor {
        self.bg_color()
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
