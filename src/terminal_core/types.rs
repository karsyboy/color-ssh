//! Shared terminal-core type aliases.

use std::io::Write;
use std::sync::{Arc, Mutex};

/// Shared writer used by embedded sessions to send input back into the PTY.
///
/// The writer itself stays transport-agnostic: the session layer owns whether
/// the bytes ultimately reach a PTY master, a child stdin pipe, or another
/// future backend.
pub(crate) type TerminalInputWriter = Arc<Mutex<Box<dyn Write + Send>>>;

/// Search match coordinates expressed as `(absolute_line, column, cell_len)`.
///
/// Keeping this in terminal space instead of renderer space allows both the
/// current TUI and a future GUI to project search results onto their own view.
pub(crate) type TerminalSearchMatch = (i64, u16, usize);
