//! TUI state model split by feature domain.

mod app_state;
mod host_browser_state;
mod pass_prompt_state;
mod quick_connect_state;
mod tab_state;

pub(crate) use app_state::{AppState, ConnectRequest, HOST_PANEL_MAX_WIDTH, HOST_PANEL_MIN_WIDTH, SessionManager};
pub(crate) use host_browser_state::{HostSearchEntry, HostTreeRow, HostTreeRowKind};
pub(crate) use pass_prompt_state::{PassPromptAction, PassPromptState};
pub(crate) use quick_connect_state::{QuickConnectField, QuickConnectState};
pub(crate) use tab_state::{HostTab, SshSession, TerminalSearchState};
