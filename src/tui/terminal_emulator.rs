//! Compatibility facade for terminal adapter APIs.

pub(crate) use crate::tui::terminal::*;

#[cfg(test)]
#[path = "../test/tui/terminal_emulator.rs"]
mod tests;
