//! Interactive TUI-based SSH host selector.

mod app;
mod features;
mod state;
mod ui;

pub(crate) use state::{
    AppState, ConnectRequest, HostTab, HostTreeRow, HostTreeRowKind, QuickConnectField, QuickConnectState, SessionManager, SshSession, TerminalSearchState,
};

pub use app::run_session_manager;
