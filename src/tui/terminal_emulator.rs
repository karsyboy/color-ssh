//! Compatibility facade over the new terminal core layer.
//!
//! The current TUI still imports `Parser`/`Screen`-style names from here while
//! the real implementation now lives in `crate::terminal_core`.

pub(crate) use crate::tui::terminal::*;

#[cfg(test)]
#[path = "../test/tui/terminal_emulator.rs"]
mod tests;
