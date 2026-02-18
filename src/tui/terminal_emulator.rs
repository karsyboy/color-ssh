//! Lightweight terminal-emulator wrapper backed by `alacritty_terminal`.

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell as TermCell, Flags};
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{NamedColor, Processor};
use crossterm::clipboard::CopyToClipboard;
use crossterm::execute;
use ratatui::style::Color as UiColor;
use std::io::{Write, stdout};
use std::sync::{Arc, Mutex};

pub(crate) use alacritty_terminal::vte::ansi::Color as AnsiColor;

pub(crate) type PtyWriter = Arc<Mutex<Box<dyn Write + Send>>>;

#[derive(Debug, Clone, Default)]
pub(crate) struct RowTextSnapshot {
    pub(crate) text: String,
    pub(crate) col_start_byte_offsets: Vec<usize>,
}

impl RowTextSnapshot {
    pub(crate) fn slice_columns(&self, start_col: u16, end_col_inclusive: u16) -> &str {
        if end_col_inclusive < start_col {
            return "";
        }

        let start = self.col_start_byte_offsets.get(start_col as usize).copied().unwrap_or(self.text.len());
        let end_exclusive = self
            .col_start_byte_offsets
            .get(end_col_inclusive.saturating_add(1) as usize)
            .copied()
            .unwrap_or(self.text.len());

        self.text.get(start..end_exclusive).unwrap_or_default()
    }
}

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

#[derive(Clone, Default)]
struct ParserEventListener {
    pty_writer: Option<PtyWriter>,
}

impl ParserEventListener {
    fn with_pty_writer(pty_writer: PtyWriter) -> Self {
        Self { pty_writer: Some(pty_writer) }
    }

    fn write_pty(&self, bytes: &[u8]) {
        if let Some(pty_writer) = &self.pty_writer
            && let Ok(mut guard) = pty_writer.lock()
        {
            let _ = guard.write_all(bytes);
            let _ = guard.flush();
        }
    }

    fn copy_to_clipboard(text: &str) {
        let mut out = stdout();
        let _ = execute!(out, CopyToClipboard::to_clipboard_from(text));
        let _ = out.flush();
    }
}

impl EventListener for ParserEventListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => self.write_pty(text.as_bytes()),
            Event::ClipboardStore(_, text) => Self::copy_to_clipboard(&text),
            _ => {}
        }
    }
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
    term: Term<ParserEventListener>,
    processor: Processor,
    dimensions: TermDimensions,
}

impl Parser {
    #[cfg(test)]
    pub(crate) fn new(rows: u16, cols: u16, history: usize) -> Self {
        Self::new_with_listener(rows, cols, history, ParserEventListener::default())
    }

    pub(crate) fn new_with_pty_writer(rows: u16, cols: u16, history: usize, pty_writer: PtyWriter) -> Self {
        Self::new_with_listener(rows, cols, history, ParserEventListener::with_pty_writer(pty_writer))
    }

    fn new_with_listener(rows: u16, cols: u16, history: usize, event_listener: ParserEventListener) -> Self {
        let dimensions = TermDimensions::new(rows, cols, history);
        let config = TermConfig {
            scrolling_history: history,
            ..TermConfig::default()
        };
        let term = Term::new(config, &dimensions, event_listener);
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

    pub(crate) fn max_scrollback(&self) -> usize {
        self.term.grid().history_size()
    }

    pub(crate) fn current_scrollback(&self) -> usize {
        self.term.grid().display_offset()
    }

    pub(crate) fn with_scrollback_restored<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let restore_scrollback = self.current_scrollback();
        let out = f(self);
        self.set_scrollback(restore_scrollback);
        out
    }

    pub(crate) fn row_snapshot(&self, row: u16) -> Option<RowTextSnapshot> {
        self.row_snapshot_internal(row, false)
    }

    pub(crate) fn row_snapshot_lowercase(&self, row: u16) -> Option<RowTextSnapshot> {
        self.row_snapshot_internal(row, true)
    }

    fn row_snapshot_internal(&self, row: u16, lowercase: bool) -> Option<RowTextSnapshot> {
        let screen = self.screen();
        let (rows, cols) = screen.size();
        if row >= rows {
            return None;
        }

        let mut snapshot = RowTextSnapshot {
            text: String::new(),
            col_start_byte_offsets: Vec::with_capacity(cols as usize),
        };

        for col in 0..cols {
            snapshot.col_start_byte_offsets.push(snapshot.text.len());
            if let Some(cell) = screen.cell(row, col) {
                if cell.has_contents() {
                    let contents = cell.contents();
                    if lowercase {
                        snapshot.text.push_str(&contents.to_lowercase());
                    } else {
                        snapshot.text.push_str(&contents);
                    }
                } else {
                    snapshot.text.push(' ');
                }
            } else {
                snapshot.text.push(' ');
            }
        }

        Some(snapshot)
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

pub(crate) fn to_ratatui_color(color: AnsiColor) -> UiColor {
    match color {
        AnsiColor::Named(named) => match named {
            NamedColor::Black => UiColor::Black,
            NamedColor::Red => UiColor::Red,
            NamedColor::Green => UiColor::Green,
            NamedColor::Yellow => UiColor::Yellow,
            NamedColor::Blue => UiColor::Blue,
            NamedColor::Magenta => UiColor::Magenta,
            NamedColor::Cyan => UiColor::Cyan,
            NamedColor::White => UiColor::Gray,
            NamedColor::BrightBlack => UiColor::DarkGray,
            NamedColor::BrightRed => UiColor::LightRed,
            NamedColor::BrightGreen => UiColor::LightGreen,
            NamedColor::BrightYellow => UiColor::LightYellow,
            NamedColor::BrightBlue => UiColor::LightBlue,
            NamedColor::BrightMagenta => UiColor::LightMagenta,
            NamedColor::BrightCyan => UiColor::LightCyan,
            NamedColor::BrightWhite => UiColor::White,
            NamedColor::DimBlack => UiColor::Black,
            NamedColor::DimRed => UiColor::Red,
            NamedColor::DimGreen => UiColor::Green,
            NamedColor::DimYellow => UiColor::Yellow,
            NamedColor::DimBlue => UiColor::Blue,
            NamedColor::DimMagenta => UiColor::Magenta,
            NamedColor::DimCyan => UiColor::Cyan,
            NamedColor::DimWhite => UiColor::Gray,
            NamedColor::Foreground | NamedColor::Background | NamedColor::Cursor => UiColor::Reset,
            NamedColor::BrightForeground => UiColor::White,
            NamedColor::DimForeground => UiColor::DarkGray,
        },
        AnsiColor::Indexed(idx) => match idx {
            0 => UiColor::Black,
            1 => UiColor::Red,
            2 => UiColor::Green,
            3 => UiColor::Yellow,
            4 => UiColor::Blue,
            5 => UiColor::Magenta,
            6 => UiColor::Cyan,
            7 => UiColor::Gray,
            8 => UiColor::DarkGray,
            9 => UiColor::LightRed,
            10 => UiColor::LightGreen,
            11 => UiColor::LightYellow,
            12 => UiColor::LightBlue,
            13 => UiColor::LightMagenta,
            14 => UiColor::LightCyan,
            15 => UiColor::White,
            _ => UiColor::Indexed(idx),
        },
        AnsiColor::Spec(rgb) => UiColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}
