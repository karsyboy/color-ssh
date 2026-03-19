//! Folder picker and folder-management modals.

const FOLDER_DELETE_CONFIRM_DELETE_LABEL: &str = "[ Enter/Y ] Delete";
const FOLDER_DELETE_CONFIRM_CANCEL_LABEL: &str = "[ Esc/N ] Cancel";
const FOLDER_DELETE_CONFIRM_ACTION_SEPARATOR: &str = " | ";

pub(crate) mod input;
pub(crate) mod mouse;
pub(crate) mod render;
