//! Per-tab and PTY session state.

use crate::ssh_config::SshHost;
use portable_pty::{Child, MasterPty};
use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Mutex, atomic::AtomicU64};
use vt100::Parser;

/// Represents an SSH session output buffer.
pub struct SshSession {
    pub(crate) pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub(crate) writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub(crate) _child: Box<dyn Child + Send>,
    pub(crate) parser: Arc<Mutex<Parser>>,
    pub(crate) replay_log: Arc<Mutex<VecDeque<u8>>>,
    pub(crate) exited: Arc<Mutex<bool>>,
    pub(crate) clear_pending: Arc<Mutex<bool>>,
    pub(crate) render_epoch: Arc<AtomicU64>,
}

#[derive(Debug, Clone, Default)]
pub struct TerminalSearchState {
    pub(crate) active: bool,
    pub(crate) query: String,
    pub(crate) matches: Vec<(i64, u16, usize)>,
    pub(crate) current: usize,
}

/// Represents an open host tab.
pub struct HostTab {
    pub(crate) host: SshHost,
    pub(crate) title: String,
    pub(crate) session: Option<SshSession>,
    pub(crate) scroll_offset: usize,
    pub(crate) terminal_search: TerminalSearchState,
    pub(crate) force_ssh_logging: bool,
    pub(crate) last_pty_size: Option<(u16, u16)>,
}
