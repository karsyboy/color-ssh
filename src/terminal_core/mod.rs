//! Frontend-neutral terminal core for embedded remote sessions.
//!
//! This module is the long-term home for the PTY-centered terminal stack used by
//! the TUI today and a future GUI frontend later. The goal is to keep process
//! management, terminal emulation, renderer-facing data extraction, and overlay
//! logic explicit and independently evolvable.
//!
//! Ownership split:
//! - `TerminalSession` owns PTY/process lifecycle, input transport, and render invalidation.
//! - `TerminalEngine` owns canonical terminal state.
//! - renderers consume viewport snapshots plus overlay spans and never rewrite PTY bytes.

mod color;
mod engine;
mod event_listener;
mod frontend;
pub(crate) mod highlight_overlay;
mod protocol;
mod session;
mod types;
mod view;

pub(crate) use color::{AnsiColor, ansi_index_to_rgb};
pub(crate) use engine::TerminalEngine;
#[allow(unused_imports)]
pub(crate) use frontend::{
    TerminalCursorState, TerminalFrontendSnapshot, TerminalGridPoint, TerminalMouseProtocolState, TerminalScrollbackState, TerminalSelection,
    TerminalSessionSnapshot,
};
pub(crate) use protocol::{encode_key_event_bytes, encode_mouse_event_bytes, encode_paste_bytes};
pub(crate) use session::{TerminalChild, TerminalSession};
pub(crate) use types::TerminalInputWriter;
#[allow(unused_imports)]
pub(crate) use view::{MouseProtocolEncoding, MouseProtocolMode, TerminalCellSnapshot, TerminalCursorSnapshot, TerminalViewport};
