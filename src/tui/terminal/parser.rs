use super::events::ParserEventListener;
use super::screen::Screen;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Line, Point};
use alacritty_terminal::term::search::RegexSearch;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::vte::ansi::Processor;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub(crate) type PtyWriter = Arc<Mutex<Box<dyn Write + Send>>>;

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
    pub(super) term: Term<ParserEventListener>,
    processor: Processor,
    dimensions: TermDimensions,
    event_listener: ParserEventListener,
}

impl Parser {
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

    pub(crate) fn screen(&self) -> Screen<'_> {
        Screen { parser: self }
    }

    pub(crate) fn max_scrollback(&self) -> usize {
        self.term.grid().history_size()
    }

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
