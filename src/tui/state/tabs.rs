//! Per-tab session state.

use crate::inventory::InventoryHost;
use crate::log_error;
use crate::tui::terminal_emulator::{Parser, PtyWriter};
use portable_pty::{Child as PtyChild, MasterPty};
use std::collections::HashMap;
use std::process::Child as ProcessChild;
use std::sync::{Arc, Mutex, atomic::AtomicU64};

pub(crate) enum ManagedChild {
    Pty(Arc<Mutex<Box<dyn PtyChild + Send + Sync>>>),
    Process(Arc<Mutex<ProcessChild>>),
}

/// Represents an active tab session output buffer.
pub(crate) struct ManagedSession {
    pub(crate) pty_master: Option<Arc<Mutex<Box<dyn MasterPty + Send>>>>,
    pub(crate) writer: Option<PtyWriter>,
    pub(crate) child: ManagedChild,
    pub(crate) parser: Arc<Mutex<Parser>>,
    pub(crate) exited: Arc<Mutex<bool>>,
    pub(crate) render_epoch: Arc<AtomicU64>,
}

impl ManagedSession {
    // Lifecycle management.
    // Closing a tab should always terminate the underlying managed process tree.
    pub(crate) fn terminate(&mut self) {
        let terminate_result = match &self.child {
            ManagedChild::Pty(child) => match child.lock() {
                Ok(mut child) => {
                    let result = child.kill();
                    let _ = child.try_wait();
                    result
                }
                Err(err) => Err(std::io::Error::other(err.to_string())),
            },
            ManagedChild::Process(child) => match child.lock() {
                Ok(mut child) => {
                    let result = child.kill();
                    let _ = child.try_wait();
                    result
                }
                Err(err) => Err(std::io::Error::other(err.to_string())),
            },
        };

        if let Err(err) = terminate_result {
            log_error!("Failed to terminate managed session: {}", err);
        }
        if let Ok(mut exited) = self.exited.lock() {
            *exited = true;
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TerminalSearchState {
    pub(crate) active: bool,
    pub(crate) query: String,
    pub(crate) query_cursor: usize,
    pub(crate) query_selection: Option<(usize, usize)>,
    pub(crate) matches: Vec<(i64, u16, usize)>,
    pub(crate) current: usize,
    pub(crate) highlight_row_ranges: HashMap<i64, Vec<(u16, u16)>>,
    pub(crate) current_highlight_range: Option<(i64, u16, u16)>,
    pub(crate) last_search_query: String,
    pub(crate) last_scanned_render_epoch: u64,
}

/// Represents an open host tab.
pub(crate) struct HostTab {
    pub(crate) host: InventoryHost,
    pub(crate) title: String,
    pub(crate) session: Option<ManagedSession>,
    pub(crate) session_error: Option<String>,
    pub(crate) scroll_offset: usize,
    pub(crate) terminal_search: TerminalSearchState,
    pub(crate) force_ssh_logging: bool,
    pub(crate) last_pty_size: Option<(u16, u16)>,
}
