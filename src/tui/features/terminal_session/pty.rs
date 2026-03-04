//! Compatibility facade for terminal session helpers.

pub(crate) use super::input::encode_key_event_bytes;
#[cfg(test)]
pub(crate) use super::io::normalize_managed_output_newlines;

#[cfg(test)]
#[path = "../../../test/tui/features/terminal_session/pty.rs"]
mod tests;
