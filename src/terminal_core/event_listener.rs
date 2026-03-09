//! Event bridge between `alacritty_terminal` and the host application.
//!
//! This listener intentionally stays small. The terminal engine owns the core
//! terminal state, while the listener only translates terminal-originated
//! events, such as PTY writes and clipboard requests, back to the embedding
//! session/frontend.

use super::ansi_index_to_rgb;
use super::types::TerminalInputWriter;
use crate::config;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::vte::ansi::Rgb;
use crossterm::clipboard::CopyToClipboard;
use crossterm::execute;
use std::io::{Write, stdout};
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
}

impl TerminalEventListener {
    pub(super) fn new(rows: u16, cols: u16, input_writer: Option<TerminalInputWriter>) -> Self {
        let state = TerminalEventState {
            input_writer,
            rows: rows.max(1),
            cols: cols.max(1),
        };
        Self {
            state: Arc::new(Mutex::new(state)),
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

    fn copy_to_clipboard(text: &str) {
        let mut out = stdout();
        let _ = execute!(out, CopyToClipboard::to_clipboard_from(text));
        let _ = out.flush();
    }

    fn remote_clipboard_policy() -> (bool, usize) {
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
}

impl EventListener for TerminalEventListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => self.write_input(text.as_bytes()),
            Event::ClipboardStore(_, text) => {
                let (enabled, max_bytes) = Self::remote_clipboard_policy();
                if enabled && Self::allow_remote_clipboard_write(&text, max_bytes) {
                    Self::copy_to_clipboard(&text);
                }
            }
            Event::ClipboardLoad(_, formatter) => {
                let response = formatter("");
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
