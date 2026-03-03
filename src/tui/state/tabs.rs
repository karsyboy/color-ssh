//! Per-tab and PTY session state.

use crate::log_error;
use crate::ssh_config::SshHost;
use crate::tui::terminal_emulator::Parser;
use portable_pty::{Child, MasterPty};
use std::io::Write;
use std::sync::{Arc, Mutex, atomic::AtomicU64};

/// Represents an SSH session output buffer.
pub(crate) struct SshSession {
    pub(crate) pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub(crate) writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub(crate) _child: Box<dyn Child + Send>,
    pub(crate) parser: Arc<Mutex<Parser>>,
    pub(crate) exited: Arc<Mutex<bool>>,
    pub(crate) render_epoch: Arc<AtomicU64>,
}

impl SshSession {
    // Lifecycle management.
    // Closing a tab should always terminate the underlying SSH process tree.
    pub(crate) fn terminate(&mut self) {
        if let Err(err) = self._child.kill() {
            log_error!("Failed to terminate SSH session: {}", err);
        }
        let _ = self._child.try_wait();
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
}

/// Represents an open host tab.
pub(crate) struct HostTab {
    pub(crate) host: SshHost,
    pub(crate) title: String,
    pub(crate) session: Option<SshSession>,
    pub(crate) scroll_offset: usize,
    pub(crate) terminal_search: TerminalSearchState,
    pub(crate) force_ssh_logging: bool,
    pub(crate) last_pty_size: Option<(u16, u16)>,
}
