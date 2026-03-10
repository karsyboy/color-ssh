//! Interactive session manager UI.
//!
//! This module exposes the public TUI entrypoint and keeps internal state,
//! rendering, and terminal emulation modules private.

mod app;
mod features;
mod state;
mod terminal;
mod text_edit;
mod ui;

pub(crate) use state::{
    AppState, ConnectRequest, HostTab, HostTreeRow, HostTreeRowKind, QuickConnectField, QuickConnectState, TerminalSearchState, VaultStatusModalState,
    VaultUnlockAction, VaultUnlockState,
};

/// Run the interactive session manager.
pub use app::run_session_manager;
