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
use alacritty_terminal::vte::ansi::NamedColor;
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

/// Backend-neutral snapshot of the currently visible terminal viewport.
///
/// Wrapped lines stay split at terminal row boundaries so renderers can honor
/// the emulator's layout rather than attempting to reflow text themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalViewport {
    size: (u16, u16),
    cursor: Option<TerminalCursorSnapshot>,
    rows: Vec<TerminalViewportRow>,
}

impl TerminalViewport {
    pub(crate) fn size(&self) -> (u16, u16) {
        self.size
    }

    pub(crate) fn cursor(&self) -> Option<TerminalCursorSnapshot> {
        self.cursor
    }

    pub(crate) fn rows(&self) -> &[TerminalViewportRow] {
        &self.rows
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalCursorSnapshot {
    row: u16,
    col: u16,
}

impl TerminalCursorSnapshot {
    pub(crate) fn new(row: u16, col: u16) -> Self {
        Self { row, col }
    }

    pub(crate) fn row(&self) -> u16 {
        self.row
    }

    pub(crate) fn col(&self) -> u16 {
        self.col
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalViewportRow {
    absolute_row: i64,
    cells: Vec<TerminalCellSnapshot>,
}

impl TerminalViewportRow {
    pub(crate) fn absolute_row(&self) -> i64 {
        self.absolute_row
    }

    pub(crate) fn cells(&self) -> &[TerminalCellSnapshot] {
        &self.cells
    }

    pub(crate) fn display_text(&self) -> String {
        let mut line = String::new();
        let mut scratch = String::new();
        for cell in &self.cells {
            line.push_str(cell.glyph().as_str(&mut scratch));
        }
        line
    }
}

/// Owned cell content extracted from the emulator.
///
/// Most cells avoid allocation by storing either a blank marker or a single
/// primary character. A `Cluster` is only allocated when combining characters
/// need to be preserved together for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminalGlyph {
    Blank,
    Char(char),
    Cluster(String),
}

impl TerminalGlyph {
    pub(crate) fn as_str<'a>(&'a self, scratch: &'a mut String) -> &'a str {
        match self {
            Self::Blank => " ",
            Self::Char(ch) => {
                scratch.clear();
                scratch.push(*ch);
                scratch.as_str()
            }
            Self::Cluster(cluster) => cluster.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalCellStyle {
    fg_color: AnsiColor,
    bg_color: AnsiColor,
    bold: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

impl Default for TerminalCellStyle {
    fn default() -> Self {
        Self {
            fg_color: AnsiColor::Named(NamedColor::Foreground),
            bg_color: AnsiColor::Named(NamedColor::Background),
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
        }
    }
}

impl TerminalCellStyle {
    pub(crate) fn fg_color(&self) -> AnsiColor {
        self.fg_color
    }

    pub(crate) fn bg_color(&self) -> AnsiColor {
        self.bg_color
    }

    pub(crate) fn bold(&self) -> bool {
        self.bold
    }

    pub(crate) fn italic(&self) -> bool {
        self.italic
    }

    pub(crate) fn underline(&self) -> bool {
        self.underline
    }

    pub(crate) fn inverse(&self) -> bool {
        self.inverse
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalCellSnapshot {
    glyph: TerminalGlyph,
    style: TerminalCellStyle,
}

impl TerminalCellSnapshot {
    pub(crate) fn glyph(&self) -> &TerminalGlyph {
        &self.glyph
    }

    pub(crate) fn fg_color(&self) -> AnsiColor {
        self.style.fg_color()
    }

    pub(crate) fn bg_color(&self) -> AnsiColor {
        self.style.bg_color()
    }

    pub(crate) fn bold(&self) -> bool {
        self.style.bold()
    }

    pub(crate) fn italic(&self) -> bool {
        self.style.italic()
    }

    pub(crate) fn underline(&self) -> bool {
        self.style.underline()
    }

    pub(crate) fn inverse(&self) -> bool {
        self.style.inverse()
    }

    fn blank() -> Self {
        Self {
            glyph: TerminalGlyph::Blank,
            style: TerminalCellStyle::default(),
        }
    }
}

pub(crate) struct TerminalViewModel<'a> {
    pub(super) engine: &'a TerminalEngine,
}

impl<'a> TerminalViewModel<'a> {
    fn current_display_scrollback(&self) -> usize {
        self.engine.term.grid().display_offset()
    }

    fn resolved_display_scrollback(&self, display_scrollback: usize) -> usize {
        display_scrollback.min(self.scrollback())
    }

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

    /// Whether bracketed paste is enabled by the remote application.
    pub(crate) fn bracketed_paste_enabled(&self) -> bool {
        self.engine.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Whether the terminal is currently rendering into the alternate screen.
    pub(crate) fn is_alternate_screen(&self) -> bool {
        self.engine.term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Snapshot the visible viewport into backend-neutral rows, cells, and
    /// cursor metadata for renderers.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn viewport_snapshot(&self, max_rows: u16, max_cols: u16) -> TerminalViewport {
        self.viewport_snapshot_at_scrollback(max_rows, max_cols, self.current_display_scrollback())
    }

    /// Snapshot the visible viewport for an explicit scrollback offset without
    /// mutating the live terminal engine state.
    pub(crate) fn viewport_snapshot_at_scrollback(&self, max_rows: u16, max_cols: u16, display_scrollback: usize) -> TerminalViewport {
        let (vt_rows, vt_cols) = self.size();
        let render_rows = max_rows.min(vt_rows);
        let render_cols = max_cols.min(vt_cols);
        let display_offset = self.resolved_display_scrollback(display_scrollback).min(i32::MAX as usize) as i32;
        let cursor = (!self.cursor_hidden())
            .then(|| TerminalCursorSnapshot::new(self.cursor_position().0, self.cursor_position().1))
            .filter(|cursor| cursor.row < render_rows && cursor.col < render_cols);

        let mut rows = Vec::with_capacity(render_rows as usize);
        for row in 0..render_rows {
            let mut cells = Vec::with_capacity(render_cols as usize);
            for col in 0..render_cols {
                let snapshot = self
                    .cell_at_scrollback(row, col, display_offset as usize)
                    .map(|cell| cell.snapshot())
                    .unwrap_or_else(TerminalCellSnapshot::blank);
                cells.push(snapshot);
            }

            rows.push(TerminalViewportRow {
                absolute_row: row as i64 - i64::from(display_offset),
                cells,
            });
        }

        TerminalViewport {
            size: (render_rows, render_cols),
            cursor,
            rows,
        }
    }

    /// Convert a visible row index into an absolute terminal line index.
    #[allow(dead_code)]
    pub(crate) fn absolute_row(&self, row: u16) -> Option<i64> {
        self.absolute_row_at_scrollback(row, self.current_display_scrollback())
    }

    /// Convert a visible row index into an absolute terminal line index for an
    /// explicit scrollback offset.
    pub(crate) fn absolute_row_at_scrollback(&self, row: u16, display_scrollback: usize) -> Option<i64> {
        let grid = self.engine.term.grid();
        if row as usize >= grid.screen_lines() {
            return None;
        }

        let display_offset = self.resolved_display_scrollback(display_scrollback).min(i32::MAX as usize) as i32;
        let line = Line(row as i32 - display_offset);
        (line >= grid.topmost_line() && line <= grid.bottommost_line()).then_some(line.0 as i64)
    }

    /// Return renderer-facing cells for the visible grid.
    pub(crate) fn cell(&self, row: u16, col: u16) -> Option<TerminalCellView<'_>> {
        self.cell_at_scrollback(row, col, self.current_display_scrollback())
    }

    /// Return renderer-facing cells for an explicit scrollback offset.
    pub(crate) fn cell_at_scrollback(&self, row: u16, col: u16, display_scrollback: usize) -> Option<TerminalCellView<'_>> {
        let grid = self.engine.term.grid();
        if row as usize >= grid.screen_lines() || col as usize >= grid.columns() {
            return None;
        }

        let display_offset = self.resolved_display_scrollback(display_scrollback).min(i32::MAX as usize) as i32;
        let line = Line(row as i32 - display_offset);
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

    /// Return logical buffer row identities in top-to-bottom order.
    ///
    /// These identities are stable only for the lifetime of the current grid
    /// storage and are intended for local change detection, not persistence.
    pub(crate) fn buffer_row_storage_ids(&self) -> Vec<usize> {
        let grid = self.engine.term.grid();
        let top = grid.topmost_line().0;
        let bottom = grid.bottommost_line().0;
        let mut row_ids = Vec::with_capacity((bottom - top + 1).max(0) as usize);

        for row in top..=bottom {
            row_ids.push(std::ptr::from_ref(&grid[Line(row)]).cast::<()>() as usize);
        }

        row_ids
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

    fn glyph(&self) -> TerminalGlyph {
        if self.cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            return TerminalGlyph::Blank;
        }
        if !Self::is_renderable_primary_char(self.cell.c) {
            return TerminalGlyph::Blank;
        }

        let mut cluster = None;
        if let Some(zerowidth) = self.cell.zerowidth() {
            let mut text = String::new();
            text.push(self.cell.c);
            for c in zerowidth {
                if Self::is_renderable_zero_width(*c) {
                    text.push(*c);
                }
            }
            if text.chars().count() > 1 {
                cluster = Some(text);
            }
        }

        match cluster {
            Some(text) => TerminalGlyph::Cluster(text),
            None => TerminalGlyph::Char(self.cell.c),
        }
    }

    fn style(&self) -> TerminalCellStyle {
        TerminalCellStyle {
            fg_color: self.cell.fg,
            bg_color: self.cell.bg,
            bold: self.bold(),
            italic: self.italic(),
            underline: self.underline(),
            inverse: self.inverse(),
        }
    }

    fn snapshot(&self) -> TerminalCellSnapshot {
        TerminalCellSnapshot {
            glyph: self.glyph(),
            style: self.style(),
        }
    }

    #[cfg(test)]
    pub(crate) fn contents(&self) -> String {
        let mut scratch = String::new();
        self.symbol(&mut scratch).to_owned()
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
