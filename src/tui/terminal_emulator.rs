//! Lightweight terminal-emulator wrapper backed by `alacritty_terminal`.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell as TermCell, Flags};
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
pub(crate) use alacritty_terminal::vte::ansi::Color as AnsiColor;
use alacritty_terminal::vte::ansi::Processor;

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

#[derive(Clone, Copy)]
struct TermDimensions {
    rows: usize,
    cols: usize,
    history: usize,
}

impl TermDimensions {
    fn new(rows: u16, cols: u16, history: usize) -> Self {
        Self {
            rows: rows.max(1) as usize,
            cols: cols.max(1) as usize,
            history,
        }
    }
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.rows.saturating_add(self.history)
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

pub(crate) struct Parser {
    term: Term<VoidListener>,
    processor: Processor,
    dimensions: TermDimensions,
}

impl Parser {
    pub(crate) fn new(rows: u16, cols: u16, history: usize) -> Self {
        let dimensions = TermDimensions::new(rows, cols, history);
        let config = TermConfig {
            scrolling_history: history,
            ..TermConfig::default()
        };
        let term = Term::new(config, &dimensions, VoidListener);
        Self {
            term,
            processor: Processor::new(),
            dimensions,
        }
    }

    pub(crate) fn process(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    pub(crate) fn set_size(&mut self, rows: u16, cols: u16) {
        self.dimensions = TermDimensions::new(rows, cols, self.dimensions.history);
        self.term.resize(self.dimensions);
    }

    pub(crate) fn set_scrollback(&mut self, scrollback: usize) {
        let max_scrollback = self.max_scrollback();
        let target = scrollback.min(max_scrollback) as i32;
        let current = self.term.grid().display_offset() as i32;
        let delta = target - current;
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(delta));
        }
    }

    pub(crate) fn screen(&self) -> Screen<'_> {
        Screen { parser: self }
    }

    fn max_scrollback(&self) -> usize {
        self.term.grid().history_size()
    }
}

pub(crate) struct Screen<'a> {
    parser: &'a Parser,
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

    pub(crate) fn mouse_protocol_encoding(&self) -> MouseProtocolEncoding {
        let mode = self.parser.term.mode();
        if mode.contains(TermMode::SGR_MOUSE) {
            MouseProtocolEncoding::Sgr
        } else {
            MouseProtocolEncoding::Default
        }
    }

    pub(crate) fn mouse_protocol_mode(&self) -> MouseProtocolMode {
        let mode = self.parser.term.mode();
        if mode.contains(TermMode::MOUSE_MOTION) {
            MouseProtocolMode::AnyMotion
        } else if mode.contains(TermMode::MOUSE_DRAG) {
            MouseProtocolMode::ButtonMotion
        } else if mode.contains(TermMode::MOUSE_REPORT_CLICK) {
            MouseProtocolMode::Press
        } else {
            MouseProtocolMode::None
        }
    }
}

pub(crate) struct CellRef<'a> {
    cell: &'a TermCell,
}

impl<'a> CellRef<'a> {
    pub(crate) fn has_contents(&self) -> bool {
        !self.cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) && self.cell.c != ' '
    }

    pub(crate) fn contents(&self) -> String {
        if self.cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            return " ".to_string();
        }

        let mut out = String::new();
        out.push(self.cell.c);
        if let Some(zerowidth) = self.cell.zerowidth() {
            for c in zerowidth {
                out.push(*c);
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
