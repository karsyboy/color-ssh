//! Canonical terminal emulation state.
//!
//! `TerminalEngine` is the source of truth for embedded interactive sessions.
//! It owns the `alacritty_terminal::Term`, the VTE processor, resize handling,
//! scrollback state, and search/selection helpers that operate on terminal
//! coordinates instead of renderer-specific abstractions.

use super::event_listener::TerminalEventListener;
use super::types::{TerminalInputWriter, TerminalSearchMatch};
use super::view::TerminalViewModel;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Line, Point};
use alacritty_terminal::term::search::RegexSearch;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::vte::ansi::Processor;

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

pub(crate) struct TerminalEngine {
    pub(super) term: Term<TerminalEventListener>,
    processor: Processor,
    dimensions: TermDimensions,
    event_listener: TerminalEventListener,
}

impl TerminalEngine {
    /// Create a terminal engine without a host input channel.
    pub(crate) fn new(rows: u16, cols: u16, history: usize) -> Self {
        Self::new_with_listener(rows, cols, history, TerminalEventListener::new(rows, cols, None))
    }

    /// Create a terminal engine that can answer PTY-originated writes.
    pub(crate) fn new_with_input_writer(rows: u16, cols: u16, history: usize, input_writer: TerminalInputWriter) -> Self {
        Self::new_with_listener(rows, cols, history, TerminalEventListener::new(rows, cols, Some(input_writer)))
    }

    fn new_with_listener(rows: u16, cols: u16, history: usize, event_listener: TerminalEventListener) -> Self {
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

    /// Apply raw terminal output bytes to the canonical terminal state.
    pub(crate) fn process_output(&mut self, bytes: &[u8]) {
        self.processor.advance(&mut self.term, bytes);
    }

    /// Resize the engine's visible surface and notify the terminal listener.
    pub(crate) fn resize_surface(&mut self, rows: u16, cols: u16) {
        self.dimensions = TermDimensions::new(rows, cols, self.dimensions.history);
        self.event_listener.resize_surface(rows, cols);
        self.term.resize(self.dimensions);
    }

    /// Set the display offset within scrollback.
    pub(crate) fn set_display_scrollback(&mut self, scrollback: usize) {
        let max_scrollback = self.max_scrollback();
        let target = scrollback.min(max_scrollback) as i32;
        let current = self.term.grid().display_offset() as i32;
        let delta = target - current;
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(delta));
        }
    }

    /// Build a renderer-facing view model over the current terminal state.
    pub(crate) fn view_model(&self) -> TerminalViewModel<'_> {
        TerminalViewModel { engine: self }
    }

    /// Transitional alias retained while existing TUI code still calls `screen`.
    pub(crate) fn screen(&self) -> TerminalViewModel<'_> {
        self.view_model()
    }

    /// Return the maximum available scrollback depth.
    pub(crate) fn max_scrollback(&self) -> usize {
        self.term.grid().history_size()
    }

    /// Extract text for an arbitrary terminal-coordinate selection.
    pub(crate) fn selection_text(&self, start: (i64, u16), end: (i64, u16)) -> String {
        let mut start_point = Point::new(Line(start.0 as i32), Column(start.1 as usize)).grid_clamp(&self.term, Boundary::Grid);
        let mut end_point = Point::new(Line(end.0 as i32), Column(end.1 as usize)).grid_clamp(&self.term, Boundary::Grid);

        if start_point > end_point {
            std::mem::swap(&mut start_point, &mut end_point);
        }

        self.term.bounds_to_string(start_point, end_point)
    }

    /// Search the full terminal buffer using literal text semantics.
    pub(crate) fn search_literal_matches(&self, query: &str) -> Vec<TerminalSearchMatch> {
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
