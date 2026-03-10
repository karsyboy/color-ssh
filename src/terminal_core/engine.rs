//! Canonical terminal emulation state.
//!
//! `TerminalEngine` is the source of truth for embedded interactive sessions.
//! It owns the `alacritty_terminal::Term`, the VTE processor, resize handling,
//! scrollback state, and search/selection helpers that operate on terminal
//! coordinates instead of renderer-specific abstractions.

use super::event_listener::TerminalEventListener;
use super::host::TerminalHostCallbacks;
use super::types::{TerminalInputWriter, TerminalSearchMatch};
use super::view::{TerminalTextSpan, TerminalViewModel};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Line, Point};
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::vte::ansi::Processor;
use regex::Regex;

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
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn new(rows: u16, cols: u16, history: usize) -> Self {
        Self::new_with_listener(rows, cols, history, TerminalEventListener::new(rows, cols, None))
    }

    /// Create a terminal engine with an explicit remote clipboard policy.
    #[allow(dead_code)]
    pub(crate) fn new_with_remote_clipboard_policy(
        rows: u16,
        cols: u16,
        history: usize,
        allow_remote_clipboard_write: bool,
        remote_clipboard_max_bytes: usize,
    ) -> Self {
        Self::new_with_listener(
            rows,
            cols,
            history,
            TerminalEventListener::new_with_remote_clipboard_policy(rows, cols, None, allow_remote_clipboard_write, remote_clipboard_max_bytes),
        )
    }

    /// Create a terminal engine that can answer PTY-originated writes.
    #[allow(dead_code)]
    pub(crate) fn new_with_input_writer(rows: u16, cols: u16, history: usize, input_writer: TerminalInputWriter) -> Self {
        Self::new_with_listener(rows, cols, history, TerminalEventListener::new(rows, cols, Some(input_writer)))
    }

    /// Create a terminal engine that can answer PTY-originated writes and host-owned callbacks.
    pub(crate) fn new_with_input_writer_and_host(
        rows: u16,
        cols: u16,
        history: usize,
        input_writer: TerminalInputWriter,
        host_callbacks: TerminalHostCallbacks,
    ) -> Self {
        Self::new_with_listener(
            rows,
            cols,
            history,
            TerminalEventListener::new_with_host(rows, cols, Some(input_writer), host_callbacks),
        )
    }

    /// Create a terminal engine that can answer PTY-originated writes with an explicit remote clipboard policy.
    #[allow(dead_code)]
    pub(crate) fn new_with_input_writer_and_remote_clipboard_policy(
        rows: u16,
        cols: u16,
        history: usize,
        input_writer: TerminalInputWriter,
        allow_remote_clipboard_write: bool,
        remote_clipboard_max_bytes: usize,
    ) -> Self {
        Self::new_with_input_writer_and_host_and_remote_clipboard_policy(
            rows,
            cols,
            history,
            input_writer,
            TerminalHostCallbacks::default(),
            allow_remote_clipboard_write,
            remote_clipboard_max_bytes,
        )
    }

    /// Create a terminal engine that can answer PTY-originated writes, host-owned callbacks, and an explicit remote clipboard policy.
    pub(crate) fn new_with_input_writer_and_host_and_remote_clipboard_policy(
        rows: u16,
        cols: u16,
        history: usize,
        input_writer: TerminalInputWriter,
        host_callbacks: TerminalHostCallbacks,
        allow_remote_clipboard_write: bool,
        remote_clipboard_max_bytes: usize,
    ) -> Self {
        Self::new_with_listener(
            rows,
            cols,
            history,
            TerminalEventListener::new_with_host_and_remote_clipboard_policy(
                rows,
                cols,
                Some(input_writer),
                host_callbacks,
                allow_remote_clipboard_write,
                remote_clipboard_max_bytes,
            ),
        )
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
    #[cfg_attr(not(test), allow(dead_code))]
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
        let regex = match Regex::new(&pattern) {
            Ok(regex) => regex,
            Err(_) => return Vec::new(),
        };

        let mut matches = Vec::new();
        let view = self.view_model();

        for absolute_row in self.term.topmost_line().0..=self.term.bottommost_line().0 {
            let Some((row_text, spans)) = view.search_text_for_absolute_row(absolute_row as i64) else {
                continue;
            };

            for row_match in regex.find_iter(&row_text) {
                let Some(start_span) = text_span_for_byte(&spans, row_match.start()) else {
                    continue;
                };
                let Some(end_span) = text_span_for_byte(&spans, row_match.end().saturating_sub(1)) else {
                    continue;
                };

                matches.push((absolute_row as i64, start_span.start_col(), end_span.end_col()));
            }
        }

        matches
    }
}

fn text_span_for_byte(spans: &[TerminalTextSpan], byte_offset: usize) -> Option<TerminalTextSpan> {
    let span_idx = spans.partition_point(|span| span.end_byte() <= byte_offset);
    let span = spans.get(span_idx).copied()?;
    (byte_offset >= span.start_byte() && byte_offset < span.end_byte()).then_some(span)
}
