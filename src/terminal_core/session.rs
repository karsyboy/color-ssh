//! Session and PTY lifecycle management for embedded terminals.
//!
//! `TerminalSession` owns the process/PTY handles that back one interactive
//! terminal surface. It deliberately does not own rendering concerns; callers
//! consume terminal state through the contained `TerminalEngine`.

use super::{TerminalEngine, TerminalInputWriter, TerminalSelection, TerminalSessionSnapshot};
use crate::log_error;
use portable_pty::{Child as PtyChild, MasterPty, PtySize};
use std::io::{self, Write};
use std::process::Child as ProcessChild;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

pub(crate) enum TerminalChild {
    Pty(Arc<Mutex<Box<dyn PtyChild + Send + Sync>>>),
    Process(Arc<Mutex<ProcessChild>>),
}

pub(crate) struct TerminalSession {
    pty_master: Option<Arc<Mutex<Box<dyn MasterPty + Send>>>>,
    input_writer: Option<TerminalInputWriter>,
    child: TerminalChild,
    engine: Arc<Mutex<TerminalEngine>>,
    exited: Arc<Mutex<bool>>,
    render_epoch: Arc<AtomicU64>,
}

impl TerminalSession {
    /// Build a new session wrapper around already-prepared process and engine state.
    pub(crate) fn new(
        pty_master: Option<Arc<Mutex<Box<dyn MasterPty + Send>>>>,
        input_writer: Option<TerminalInputWriter>,
        child: TerminalChild,
        engine: Arc<Mutex<TerminalEngine>>,
        exited: Arc<Mutex<bool>>,
        render_epoch: Arc<AtomicU64>,
    ) -> Self {
        Self {
            pty_master,
            input_writer,
            child,
            engine,
            exited,
            render_epoch,
        }
    }

    /// Return the canonical terminal engine for this session.
    pub(crate) fn engine(&self) -> &Arc<Mutex<TerminalEngine>> {
        &self.engine
    }

    /// Clone the canonical terminal engine handle for background work.
    pub(crate) fn engine_handle(&self) -> Arc<Mutex<TerminalEngine>> {
        self.engine.clone()
    }

    /// Return the current render epoch value.
    pub(crate) fn render_epoch(&self) -> u64 {
        self.render_epoch.load(Ordering::Relaxed)
    }

    /// Snapshot the frontend-facing terminal state for a renderer frame.
    pub(crate) fn snapshot_for_frontend(&self, max_rows: u16, max_cols: u16, display_scrollback: usize) -> io::Result<TerminalSessionSnapshot> {
        let mut engine = self.engine.lock().map_err(|err| io::Error::other(err.to_string()))?;
        engine.set_display_scrollback(display_scrollback);
        Ok(TerminalSessionSnapshot::new(
            self.render_epoch(),
            engine.view_model().frontend_snapshot(max_rows, max_cols),
        ))
    }

    /// Bump the render epoch after any state mutation visible to renderers.
    pub(crate) fn bump_render_epoch(&self) {
        self.render_epoch.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether the backing process has exited.
    pub(crate) fn is_exited(&self) -> bool {
        self.exited.lock().map(|exited| *exited).unwrap_or(true)
    }

    /// Mark the backing process as exited.
    pub(crate) fn mark_exited(&self) {
        if let Ok(mut exited) = self.exited.lock() {
            *exited = true;
        }
    }

    /// Write input bytes to the active embedded session transport.
    pub(crate) fn write_input(&self, bytes: &[u8]) -> io::Result<()> {
        let Some(input_writer) = self.input_writer.as_ref() else {
            return Ok(());
        };

        let mut input_writer = input_writer.lock().map_err(|err| io::Error::other(err.to_string()))?;
        input_writer.write_all(bytes)
    }

    /// Extract text for a typed terminal-coordinate selection.
    pub(crate) fn selection_text_for(&self, selection: TerminalSelection) -> io::Result<String> {
        let engine = self.engine.lock().map_err(|err| io::Error::other(err.to_string()))?;
        Ok(engine.view_model().selection_text_for(selection))
    }

    /// Resize both the PTY surface and the canonical terminal engine.
    pub(crate) fn resize(&self, rows: u16, cols: u16) {
        let rows = rows.max(1);
        let cols = cols.max(1);

        if let Some(pty_master) = self.pty_master.as_ref() {
            match pty_master.lock() {
                Ok(pty_master) => {
                    if let Err(err) = pty_master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    }) {
                        log_error!("Failed to resize PTY: {}", err);
                    }
                }
                Err(err) => log_error!("Failed to lock PTY for resize: {}", err),
            }
        }

        match self.engine.lock() {
            Ok(mut engine) => engine.resize_surface(rows, cols),
            Err(err) => log_error!("Failed to lock terminal engine for resize: {}", err),
        }

        self.bump_render_epoch();
    }

    /// Lifecycle management.
    /// Closing a tab should always terminate the underlying managed process tree.
    pub(crate) fn terminate(&mut self) {
        let terminate_result = match &self.child {
            TerminalChild::Pty(child) => match child.lock() {
                Ok(mut child) => {
                    let result = child.kill();
                    let _ = child.try_wait();
                    result
                }
                Err(err) => Err(io::Error::other(err.to_string())),
            },
            TerminalChild::Process(child) => match child.lock() {
                Ok(mut child) => {
                    let result = child.kill();
                    let _ = child.try_wait();
                    result
                }
                Err(err) => Err(io::Error::other(err.to_string())),
            },
        };

        if let Err(err) = terminate_result {
            log_error!("Failed to terminate managed session: {}", err);
        }
        self.mark_exited();
    }
}
