//! Password vault unlock modal state and deferred action context.

use crate::ssh_config::SshHost;
use zeroize::Zeroize;

pub(crate) const VAULT_UNLOCK_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone)]
pub(crate) enum VaultUnlockAction {
    OpenHostTab { host: Box<SshHost>, force_ssh_logging: bool },
    ReconnectTab { tab_index: usize },
}

#[derive(Debug, Clone)]
pub(crate) struct VaultUnlockState {
    pub(crate) entry_name: String,
    pub(crate) master_password: String,
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
            master_password: String::new(),
            cursor: 0,
            attempts: 0,
            max_attempts: VAULT_UNLOCK_MAX_ATTEMPTS,
            error: None,
            action,
        }
    }

    pub(crate) fn masked_master_password(&self) -> String {
        "*".repeat(self.master_password.chars().count())
    }

    pub(crate) fn remaining_attempts(&self) -> usize {
        self.max_attempts.saturating_sub(self.attempts)
    }

    pub(crate) fn clear_master_password(&mut self) {
        self.master_password.zeroize();
        self.cursor = 0;
    }

    pub(crate) fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub(crate) fn move_cursor_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.master_password.chars().count());
    }

    pub(crate) fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn move_cursor_end(&mut self) {
        self.cursor = self.master_password.chars().count();
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        let insert_at = byte_index_for_char(&self.master_password, self.cursor);
        self.master_password.insert(insert_at, ch);
        self.cursor += 1;
    }

    pub(crate) fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let end = byte_index_for_char(&self.master_password, self.cursor);
        let start = byte_index_for_char(&self.master_password, self.cursor - 1);
        self.master_password.replace_range(start..end, "");
        self.cursor -= 1;
    }

    pub(crate) fn delete(&mut self) {
        let len = self.master_password.chars().count();
        if self.cursor >= len {
            return;
        }
        let start = byte_index_for_char(&self.master_password, self.cursor);
        let end = byte_index_for_char(&self.master_password, self.cursor + 1);
        self.master_password.replace_range(start..end, "");
    }
}

impl Drop for VaultUnlockState {
    fn drop(&mut self) {
        self.master_password.zeroize();
    }
}

fn byte_index_for_char(text: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }

    let max = text.chars().count();
    let clamped = char_index.min(max);
    if clamped == max {
        return text.len();
    }

    text.char_indices().nth(clamped).map_or(text.len(), |(byte_index, _)| byte_index)
}
