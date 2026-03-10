//! Frontend-facing terminal snapshot contract.
//!
//! This module gathers the renderer-neutral terminal state a frontend needs to
//! paint a session without reaching into PTY or emulator internals directly.
//! The ratatui renderer and a future GUI renderer should both consume these
//! snapshot types.

use super::highlight_overlay::{HighlightOverlay, HighlightOverlayContext, HighlightOverlayEngine, HighlightOverlayViewport};
use super::view::{TerminalCursorSnapshot, TerminalViewModel};
use super::{MouseProtocolEncoding, MouseProtocolMode, TerminalViewport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct TerminalGridPoint {
    absolute_row: i64,
    column: u16,
}

impl TerminalGridPoint {
    pub(crate) fn new(absolute_row: i64, column: u16) -> Self {
        Self { absolute_row, column }
    }

    pub(crate) fn absolute_row(self) -> i64 {
        self.absolute_row
    }

    pub(crate) fn column(self) -> u16 {
        self.column
    }
}

impl From<(i64, u16)> for TerminalGridPoint {
    fn from((absolute_row, column): (i64, u16)) -> Self {
        Self::new(absolute_row, column)
    }
}

impl From<TerminalGridPoint> for (i64, u16) {
    fn from(point: TerminalGridPoint) -> Self {
        (point.absolute_row, point.column)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalSelection {
    start: TerminalGridPoint,
    end: TerminalGridPoint,
}

impl TerminalSelection {
    pub(crate) fn new(start: impl Into<TerminalGridPoint>, end: impl Into<TerminalGridPoint>) -> Self {
        Self {
            start: start.into(),
            end: end.into(),
        }
    }

    pub(crate) fn ordered(self) -> Self {
        if self.start <= self.end {
            self
        } else {
            Self {
                start: self.end,
                end: self.start,
            }
        }
    }

    pub(crate) fn start(self) -> TerminalGridPoint {
        self.ordered().start
    }

    pub(crate) fn end(self) -> TerminalGridPoint {
        self.ordered().end
    }

    pub(crate) fn contains_cell(self, absolute_row: i64, column: u16) -> bool {
        let ordered = self.ordered();
        let start = ordered.start;
        let end = ordered.end;

        if absolute_row < start.absolute_row() || absolute_row > end.absolute_row() {
            return false;
        }

        if start.absolute_row() == end.absolute_row() {
            column >= start.column() && column <= end.column()
        } else if absolute_row == start.absolute_row() {
            column >= start.column()
        } else if absolute_row == end.absolute_row() {
            column <= end.column()
        } else {
            true
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalCursorState {
    position: TerminalCursorSnapshot,
    hidden: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
impl TerminalCursorState {
    pub(crate) fn position(self) -> TerminalCursorSnapshot {
        self.position
    }

    pub(crate) fn hidden(self) -> bool {
        self.hidden
    }

    pub(crate) fn viewport_position(self, viewport_size: (u16, u16)) -> Option<TerminalCursorSnapshot> {
        (!self.hidden && self.position.row() < viewport_size.0 && self.position.col() < viewport_size.1).then_some(self.position)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalMouseProtocolState {
    mode: MouseProtocolMode,
    encoding: MouseProtocolEncoding,
}

#[cfg_attr(not(test), allow(dead_code))]
impl TerminalMouseProtocolState {
    #[allow(dead_code)]
    pub(crate) fn mode(self) -> MouseProtocolMode {
        self.mode
    }

    #[allow(dead_code)]
    pub(crate) fn encoding(self) -> MouseProtocolEncoding {
        self.encoding
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalScrollbackState {
    display_offset: usize,
    max_offset: usize,
}

impl TerminalScrollbackState {
    pub(crate) fn display_offset(self) -> usize {
        self.display_offset
    }

    pub(crate) fn max_offset(self) -> usize {
        self.max_offset
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalFrontendSnapshot {
    viewport: TerminalViewport,
    cursor: TerminalCursorState,
    scrollback: TerminalScrollbackState,
    alternate_screen: bool,
    mouse_protocol: TerminalMouseProtocolState,
}

#[cfg_attr(not(test), allow(dead_code))]
impl TerminalFrontendSnapshot {
    pub(crate) fn viewport(&self) -> &TerminalViewport {
        &self.viewport
    }

    pub(crate) fn into_viewport(self) -> TerminalViewport {
        self.viewport
    }

    pub(crate) fn cursor(&self) -> TerminalCursorState {
        self.cursor
    }

    pub(crate) fn visible_cursor(&self) -> Option<TerminalCursorSnapshot> {
        self.cursor.viewport_position(self.viewport.size())
    }

    pub(crate) fn scrollback(&self) -> TerminalScrollbackState {
        self.scrollback
    }

    pub(crate) fn is_alternate_screen(&self) -> bool {
        self.alternate_screen
    }

    #[allow(dead_code)]
    pub(crate) fn mouse_protocol(&self) -> TerminalMouseProtocolState {
        self.mouse_protocol
    }

    pub(crate) fn build_highlight_overlay(&self, overlay_engine: &mut HighlightOverlayEngine, render_epoch: u64) -> HighlightOverlay {
        overlay_engine.build_visible_overlay(
            &HighlightOverlayViewport::new(&self.viewport, self.alternate_screen, self.mouse_protocol.mode, self.cursor.hidden),
            HighlightOverlayContext {
                render_epoch,
                display_scrollback: self.scrollback.display_offset,
            },
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalSessionSnapshot {
    render_epoch: u64,
    frontend: TerminalFrontendSnapshot,
}

#[cfg_attr(not(test), allow(dead_code))]
impl TerminalSessionSnapshot {
    pub(super) fn new(render_epoch: u64, frontend: TerminalFrontendSnapshot) -> Self {
        Self { render_epoch, frontend }
    }

    #[allow(dead_code)]
    pub(crate) fn render_epoch(&self) -> u64 {
        self.render_epoch
    }

    #[allow(dead_code)]
    pub(crate) fn frontend(&self) -> &TerminalFrontendSnapshot {
        &self.frontend
    }

    pub(crate) fn viewport(&self) -> &TerminalViewport {
        self.frontend.viewport()
    }

    pub(crate) fn scrollback(&self) -> TerminalScrollbackState {
        self.frontend.scrollback()
    }

    pub(crate) fn build_highlight_overlay(&self, overlay_engine: &mut HighlightOverlayEngine) -> HighlightOverlay {
        self.frontend.build_highlight_overlay(overlay_engine, self.render_epoch)
    }
}

impl<'a> TerminalViewModel<'a> {
    /// Snapshot the renderer-facing terminal state a frontend needs to paint a frame.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn frontend_snapshot(&self, max_rows: u16, max_cols: u16) -> TerminalFrontendSnapshot {
        self.frontend_snapshot_at_scrollback(max_rows, max_cols, self.engine.term.grid().display_offset())
    }

    /// Snapshot the renderer-facing terminal state for an explicit scrollback
    /// offset without mutating the live engine state.
    pub(crate) fn frontend_snapshot_at_scrollback(&self, max_rows: u16, max_cols: u16, display_scrollback: usize) -> TerminalFrontendSnapshot {
        let display_scrollback = display_scrollback.min(self.scrollback());
        let viewport = self.viewport_snapshot_at_scrollback(max_rows, max_cols, display_scrollback);
        let cursor_position = self.cursor_position();
        let cursor = TerminalCursorState {
            position: TerminalCursorSnapshot::new(cursor_position.0, cursor_position.1),
            hidden: self.cursor_hidden(),
        };
        let (mouse_mode, mouse_encoding) = self.mouse_protocol();

        TerminalFrontendSnapshot {
            viewport,
            cursor,
            scrollback: TerminalScrollbackState {
                display_offset: display_scrollback,
                max_offset: self.scrollback(),
            },
            alternate_screen: self.is_alternate_screen(),
            mouse_protocol: TerminalMouseProtocolState {
                mode: mouse_mode,
                encoding: mouse_encoding,
            },
        }
    }

    /// Extract text for a typed terminal-coordinate selection.
    pub(crate) fn selection_text_for(&self, selection: TerminalSelection) -> String {
        let selection = selection.ordered();
        self.selection_text(selection.start().into(), selection.end().into())
    }
}
