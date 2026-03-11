//! Event bridge between `alacritty_terminal` and the host application.
//!
//! This listener intentionally stays small. The terminal engine owns the core
//! terminal state, while the listener only translates terminal-originated
//! events, such as PTY writes and clipboard requests, back to the embedding
//! session/frontend.

use super::types::TerminalInputWriter;
use super::{TerminalClipboardTarget, TerminalHostCallbacks, ansi_index_to_rgb};
use crate::config;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::term::ClipboardType;
use alacritty_terminal::vte::ansi::Rgb;
use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct TerminalEventState {
    input_writer: Option<TerminalInputWriter>,
    rows: u16,
    cols: u16,
}

#[derive(Clone)]
pub(super) struct TerminalEventListener {
    state: Arc<Mutex<TerminalEventState>>,
    host_callbacks: TerminalHostCallbacks,
    allow_remote_clipboard_write: bool,
    remote_clipboard_max_bytes: usize,
}

impl TerminalEventListener {
    pub(super) fn new_with_host(rows: u16, cols: u16, input_writer: Option<TerminalInputWriter>, host_callbacks: TerminalHostCallbacks) -> Self {
        let (allow_remote_clipboard_write, remote_clipboard_max_bytes) = Self::current_remote_clipboard_policy();
        Self::new_with_host_and_remote_clipboard_policy(
            rows,
            cols,
            input_writer,
            host_callbacks,
            allow_remote_clipboard_write,
            remote_clipboard_max_bytes,
        )
    }

    pub(super) fn new_with_host_and_remote_clipboard_policy(
        rows: u16,
        cols: u16,
        input_writer: Option<TerminalInputWriter>,
        host_callbacks: TerminalHostCallbacks,
        allow_remote_clipboard_write: bool,
        remote_clipboard_max_bytes: usize,
    ) -> Self {
        let state = TerminalEventState {
            input_writer,
            rows: rows.max(1),
            cols: cols.max(1),
        };
        Self {
            state: Arc::new(Mutex::new(state)),
            host_callbacks,
            allow_remote_clipboard_write,
            remote_clipboard_max_bytes,
        }
    }

    pub(super) fn resize_surface(&self, rows: u16, cols: u16) {
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

    fn write_input(&self, bytes: &[u8]) {
        if let Ok(state) = self.state.lock()
            && let Some(input_writer) = &state.input_writer
            && let Ok(mut guard) = input_writer.lock()
        {
            let _ = guard.write_all(bytes);
            let _ = guard.flush();
        }
    }

    fn current_remote_clipboard_policy() -> (bool, usize) {
        config::with_current_config("reading remote clipboard policy", |cfg| {
            cfg.interactive_settings
                .as_ref()
                .map(|interactive| (interactive.allow_remote_clipboard_write, interactive.remote_clipboard_max_bytes))
                .unwrap_or((false, 4096))
        })
    }

    fn allow_remote_clipboard_write(text: &str, max_bytes: usize) -> bool {
        if text.is_empty() || text.len() > max_bytes {
            return false;
        }

        !text.chars().any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    }

    fn color_index_rgb(index: usize) -> Rgb {
        if index <= 15 { ansi_index_to_rgb(index as u8) } else { ansi_index_to_rgb(7) }
    }

    fn clipboard_target(target: ClipboardType) -> TerminalClipboardTarget {
        match target {
            ClipboardType::Clipboard => TerminalClipboardTarget::Clipboard,
            ClipboardType::Selection => TerminalClipboardTarget::Selection,
        }
    }
}

impl EventListener for TerminalEventListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => self.write_input(text.as_bytes()),
            Event::ClipboardStore(target, text) => {
                if self.allow_remote_clipboard_write && Self::allow_remote_clipboard_write(&text, self.remote_clipboard_max_bytes) {
                    self.host_callbacks.store_clipboard(Self::clipboard_target(target), &text);
                }
            }
            Event::ClipboardLoad(target, formatter) => {
                let clipboard = self.host_callbacks.load_clipboard(Self::clipboard_target(target)).unwrap_or_default();
                let response = formatter(&clipboard);
                self.write_input(response.as_bytes());
            }
            Event::TextAreaSizeRequest(formatter) => {
                let (rows, cols) = self.size();
                let response = formatter(WindowSize {
                    num_lines: rows,
                    num_cols: cols,
                    cell_width: 0,
                    cell_height: 0,
                });
                self.write_input(response.as_bytes());
            }
            Event::ColorRequest(index, formatter) => {
                let response = formatter(Self::color_index_rgb(index));
                self.write_input(response.as_bytes());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "../test/terminal/event_listener.rs"]
mod tests;
