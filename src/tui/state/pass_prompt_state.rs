//! Password vault unlock modal state and deferred action context.

use crate::auth::secret::SensitiveBuffer;
use crate::ssh_config::SshHost;

pub(crate) const VAULT_UNLOCK_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone)]
pub(crate) enum VaultUnlockAction {
    OpenHostTab { host: Box<SshHost>, force_ssh_logging: bool },
    ReconnectTab { tab_index: usize },
}

pub(crate) struct VaultUnlockState {
    pub(crate) entry_name: String,
    pub(crate) master_password: SensitiveBuffer,
    pub(crate) cursor: usize,
    pub(crate) attempts: usize,
    pub(crate) max_attempts: usize,
    pub(crate) error: Option<String>,
    pub(crate) action: VaultUnlockAction,
}

impl VaultUnlockState {
    pub(crate) fn new(entry_name: String, action: VaultUnlockAction) -> Self {
        Self {
            entry_name,
            master_password: SensitiveBuffer::new(),
            cursor: 0,
            attempts: 0,
            max_attempts: VAULT_UNLOCK_MAX_ATTEMPTS,
            error: None,
            action,
        }
    }

    pub(crate) fn masked_master_password(&self) -> String {
        self.master_password.masked()
    }

    pub(crate) fn remaining_attempts(&self) -> usize {
        self.max_attempts.saturating_sub(self.attempts)
    }

    pub(crate) fn clear_master_password(&mut self) {
        self.master_password.clear();
        self.cursor = 0;
    }

    pub(crate) fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub(crate) fn move_cursor_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.master_password.char_len());
    }

    pub(crate) fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn move_cursor_end(&mut self) {
        self.cursor = self.master_password.char_len();
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        self.master_password.insert_char(self.cursor, ch);
        self.cursor += 1;
    }

    pub(crate) fn backspace(&mut self) {
        self.cursor = self.master_password.backspace_char(self.cursor);
    }

    pub(crate) fn delete(&mut self) {
        self.cursor = self.master_password.delete_char(self.cursor);
    }
}

impl Drop for VaultUnlockState {
    fn drop(&mut self) {
        self.master_password.clear();
    }
}
