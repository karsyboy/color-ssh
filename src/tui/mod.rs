//! Interactive TUI-based SSH host selector.

mod app;
mod features;
mod state;
mod terminal_emulator;
mod ui;

pub(crate) use state::{
    AppState, ConnectRequest, HostTab, HostTreeRow, HostTreeRowKind, ManagedChild, ManagedSession, QuickConnectField, QuickConnectState, TerminalSearchState,
    VaultStatusModalState, VaultUnlockAction, VaultUnlockState,
};

pub use app::run_session_manager;
