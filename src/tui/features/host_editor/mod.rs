//! Host entry editing/creation feature.

const HOST_DELETE_CONFIRM_DELETE_LABEL: &str = "[ Enter/Y ] Delete";
const HOST_DELETE_CONFIRM_CANCEL_LABEL: &str = "[ Esc/N ] Cancel";
const HOST_DELETE_CONFIRM_ACTION_SEPARATOR: &str = " | ";

pub(crate) mod input;
pub(crate) mod mouse;
pub(crate) mod render;
pub(crate) mod scroll;
