//! Lightweight terminal-emulator wrapper backed by `alacritty_terminal`.

use crate::tui::ui::theme;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Line, Point};
use alacritty_terminal::term::cell::{Cell as TermCell, Flags};
use alacritty_terminal::term::search::RegexSearch;
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{NamedColor, Processor, Rgb};
use crossterm::clipboard::CopyToClipboard;
use crossterm::execute;
use ratatui::style::Color as UiColor;
use std::io::{Write, stdout};
use std::sync::{Arc, Mutex};
use unicode_width::UnicodeWidthChar;

pub(crate) use alacritty_terminal::vte::ansi::Color as AnsiColor;

pub(crate) type PtyWriter = Arc<Mutex<Box<dyn Write + Send>>>;

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

#[derive(Default)]
struct ParserEventState {
    pty_writer: Option<PtyWriter>,
    rows: u16,
    cols: u16,
}

#[derive(Clone)]
struct ParserEventListener {
    state: Arc<Mutex<ParserEventState>>,
}

impl ParserEventListener {
    // Construction / shared state access.
    fn new(rows: u16, cols: u16, pty_writer: Option<PtyWriter>) -> Self {
        let state = ParserEventState {
            pty_writer,
            rows: rows.max(1),
            cols: cols.max(1),
        };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    fn set_size(&self, rows: u16, cols: u16) {
        if let Ok(mut state) = self.state.lock() {
            state.rows = rows.max(1);
            state.cols = cols.max(1);
        }
    }

    fn size(&self) -> (u16, u16) {
        if let Ok(state) = self.state.lock() {
            (state.rows, state.cols)
        } else {
            (1, 1)
        }
    }

    // PTY + clipboard side effects requested by terminal events.
    fn write_pty(&self, bytes: &[u8]) {
        if let Ok(state) = self.state.lock()
            && let Some(pty_writer) = &state.pty_writer
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

    // ANSI 16-color lookup for color queries.
    fn color_index_rgb(index: usize) -> Rgb {
        let color = if index <= 15 {
            ansi_index_to_theme_color(index as u8)
        } else {
            ansi_index_to_theme_color(7)
        };
        ui_color_to_rgb(color).unwrap_or(Rgb { r: 229, g: 229, b: 229 })
    }
}

impl EventListener for ParserEventListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => self.write_pty(text.as_bytes()),
            Event::ClipboardStore(_, text) => Self::copy_to_clipboard(&text),
            Event::ClipboardLoad(_, formatter) => {
                let response = formatter("");
                self.write_pty(response.as_bytes());
            }
            Event::TextAreaSizeRequest(formatter) => {
                let (rows, cols) = self.size();
                let response = formatter(WindowSize {
                    num_lines: rows,
                    num_cols: cols,
                    cell_width: 0,
                    cell_height: 0,
                });
                self.write_pty(response.as_bytes());
            }
            Event::ColorRequest(index, formatter) => {
                let response = formatter(Self::color_index_rgb(index));
                self.write_pty(response.as_bytes());
            }
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
    // Construction.
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
    event_listener: ParserEventListener,
}

impl Parser {
    // Construction.
    #[cfg(test)]
    pub(crate) fn new(rows: u16, cols: u16, history: usize) -> Self {
        Self::new_with_listener(rows, cols, history, ParserEventListener::new(rows, cols, None))
    }

    pub(crate) fn new_with_pty_writer(rows: u16, cols: u16, history: usize, pty_writer: PtyWriter) -> Self {
        Self::new_with_listener(rows, cols, history, ParserEventListener::new(rows, cols, Some(pty_writer)))
    }

    fn new_with_listener(rows: u16, cols: u16, history: usize, event_listener: ParserEventListener) -> Self {
        let dimensions = TermDimensions::new(rows, cols, history);
        let config = TermConfig {
            scrolling_history: history,
            ..TermConfig::default()
        };
        let term = Term::new(config, &dimensions, event_listener.clone());
        Self {
            term,
            processor: Processor::new(),
            dimensions,
            event_listener,
        }
    }

    // Input + viewport updates.
    pub(crate) fn process(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    pub(crate) fn set_size(&mut self, rows: u16, cols: u16) {
        self.dimensions = TermDimensions::new(rows, cols, self.dimensions.history);
        self.event_listener.set_size(rows, cols);
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

    // Read-only screen access.
    pub(crate) fn screen(&self) -> Screen<'_> {
        Screen { parser: self }
    }

    pub(crate) fn max_scrollback(&self) -> usize {
        self.term.grid().history_size()
    }

    // Selection + search helpers.
    pub(crate) fn selection_text(&self, start: (i64, u16), end: (i64, u16)) -> String {
        let mut start_point = Point::new(Line(start.0 as i32), Column(start.1 as usize)).grid_clamp(&self.term, Boundary::Grid);
        let mut end_point = Point::new(Line(end.0 as i32), Column(end.1 as usize)).grid_clamp(&self.term, Boundary::Grid);

        if start_point > end_point {
            std::mem::swap(&mut start_point, &mut end_point);
        }

        self.term.bounds_to_string(start_point, end_point)
    }

    pub(crate) fn search_literal_matches(&self, query: &str) -> Vec<(i64, u16, usize)> {
        if query.is_empty() {
            return Vec::new();
        }

        let pattern = format!("(?i:{})", regex::escape(query));
        let mut regex = match RegexSearch::new(&pattern) {
            Ok(regex) => regex,
            Err(_) => return Vec::new(),
        };

        let top = self.term.topmost_line();
        let bottom = self.term.bottommost_line();
        let last_col = self.term.last_column();
        let search_end = Point::new(bottom, last_col);
        let mut search_start = Point::new(top, Column(0));
        let query_char_count = query.chars().count();
        let mut matches = Vec::new();

        while search_start <= search_end {
            let Some(range) = self.term.regex_search_right(&mut regex, search_start, search_end) else {
                break;
            };

            let start_point = *range.start();
            let end_point = *range.end();

            if start_point.line == end_point.line {
                matches.push((start_point.line.0 as i64, start_point.column.0 as u16, query_char_count));
            }

            if end_point >= search_end {
                break;
            }

            search_start = if end_point.column < last_col {
                Point::new(end_point.line, end_point.column + 1)
            } else {
                Point::new(end_point.line + 1, Column(0))
            };
        }

        matches
    }
}

pub(crate) struct Screen<'a> {
    parser: &'a Parser,
}

impl<'a> Screen<'a> {
    // Basic screen metadata.
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

    // Cell lookups.
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

    // Active mouse-reporting mode from terminal state.
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

    // Text content.
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

    // Style accessors.
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
    // Foreground path: honor theme palette.
    match color {
        AnsiColor::Named(named) => match named {
            NamedColor::Black | NamedColor::DimBlack => ansi_index_to_theme_color(0),
            NamedColor::Red | NamedColor::DimRed => ansi_index_to_theme_color(1),
            NamedColor::Green | NamedColor::DimGreen => ansi_index_to_theme_color(2),
            NamedColor::Yellow | NamedColor::DimYellow => ansi_index_to_theme_color(3),
            NamedColor::Blue | NamedColor::DimBlue => ansi_index_to_theme_color(4),
            NamedColor::Magenta | NamedColor::DimMagenta => ansi_index_to_theme_color(5),
            NamedColor::Cyan | NamedColor::DimCyan => ansi_index_to_theme_color(6),
            NamedColor::White | NamedColor::DimWhite => ansi_index_to_theme_color(7),
            NamedColor::BrightBlack | NamedColor::DimForeground => ansi_index_to_theme_color(8),
            NamedColor::BrightRed => ansi_index_to_theme_color(9),
            NamedColor::BrightGreen => ansi_index_to_theme_color(10),
            NamedColor::BrightYellow => ansi_index_to_theme_color(11),
            NamedColor::BrightBlue => ansi_index_to_theme_color(12),
            NamedColor::BrightMagenta => ansi_index_to_theme_color(13),
            NamedColor::BrightCyan => ansi_index_to_theme_color(14),
            NamedColor::BrightWhite | NamedColor::BrightForeground => ansi_index_to_theme_color(15),
            NamedColor::Foreground => theme::ansi_white(),
            NamedColor::Background | NamedColor::Cursor => UiColor::Reset,
        },
        AnsiColor::Indexed(idx) => match idx {
            0..=15 => ansi_index_to_theme_color(idx),
            _ => UiColor::Indexed(idx),
        },
        AnsiColor::Spec(rgb) => UiColor::Rgb(rgb.r, rgb.g, rgb.b),
    }
}

pub(crate) fn to_ratatui_background_color(color: AnsiColor) -> UiColor {
    // Background path: keep legacy/default ANSI mapping.
    match color {
        AnsiColor::Named(named) => match named {
            NamedColor::Black | NamedColor::DimBlack => UiColor::Black,
            NamedColor::Red | NamedColor::DimRed => UiColor::Red,
            NamedColor::Green | NamedColor::DimGreen => UiColor::Green,
            NamedColor::Yellow | NamedColor::DimYellow => UiColor::Yellow,
            NamedColor::Blue | NamedColor::DimBlue => UiColor::Blue,
            NamedColor::Magenta | NamedColor::DimMagenta => UiColor::Magenta,
            NamedColor::Cyan | NamedColor::DimCyan => UiColor::Cyan,
            NamedColor::White | NamedColor::DimWhite => UiColor::Gray,
            NamedColor::BrightBlack | NamedColor::DimForeground => UiColor::DarkGray,
            NamedColor::BrightRed => UiColor::LightRed,
            NamedColor::BrightGreen => UiColor::LightGreen,
            NamedColor::BrightYellow => UiColor::LightYellow,
            NamedColor::BrightBlue => UiColor::LightBlue,
            NamedColor::BrightMagenta => UiColor::LightMagenta,
            NamedColor::BrightCyan => UiColor::LightCyan,
            NamedColor::BrightWhite | NamedColor::BrightForeground => UiColor::White,
            NamedColor::Foreground | NamedColor::Background | NamedColor::Cursor => UiColor::Reset,
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

fn ansi_index_to_theme_color(index: u8) -> UiColor {
    let palette = theme::session_theme().ansi;
    match index {
        0 => palette.black,
        1 => palette.red,
        2 => palette.green,
        3 => palette.yellow,
        4 => palette.blue,
        5 => palette.magenta,
        6 => palette.cyan,
        7 => palette.white,
        8 => palette.bright_black,
        9 => palette.bright_red,
        10 => palette.bright_green,
        11 => palette.bright_yellow,
        12 => palette.bright_blue,
        13 => palette.bright_magenta,
        14 => palette.bright_cyan,
        _ => palette.bright_white,
    }
}

fn ui_color_to_rgb(color: UiColor) -> Option<Rgb> {
    if let UiColor::Rgb(r, g, b) = color { Some(Rgb { r, g, b }) } else { None }
}

#[cfg(test)]
#[path = "../test/tui/terminal_emulator.rs"]
mod tests;
