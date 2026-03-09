mod color;

pub(crate) use crate::terminal_core::{MouseProtocolEncoding, MouseProtocolMode, TerminalEngine as Parser};
pub(crate) use color::{to_ratatui_background_color, to_ratatui_color};
