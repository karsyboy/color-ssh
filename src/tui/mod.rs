//! Interactive session manager UI.
//!
//! This module exposes the public TUI entrypoint and keeps internal state,
//! rendering, and terminal emulation modules private.

mod app;
mod features;
mod state;
mod text_edit;
mod ui;

pub(crate) use state::{
    AppState, ConnectRequest, HostContextMenuAction, HostContextMenuState, HostDeleteConfirmState, HostEditorField, HostEditorMode, HostEditorState, HostTab,
    HostTreeRow, HostTreeRowKind, QuickConnectField, QuickConnectRow, QuickConnectState, QuickConnectSubmission, QuickConnectValidationError,
    RdpCredentialLaunchContext, RdpCredentialSubmission, RdpCredentialValidationError, RdpCredentialsAction, RdpCredentialsField, RdpCredentialsState,
    TerminalSearchState, VaultStatusModalState, VaultUnlockAction, VaultUnlockState,
};

/// Run the interactive session manager.
pub use app::run_session_manager;
