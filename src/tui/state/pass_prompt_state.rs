//! #_pass modal state and deferred action context.

use crate::ssh_config::SshHost;

pub(crate) const PASS_PROMPT_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone)]
pub(crate) enum PassPromptAction {
    OpenHostTab { host: SshHost, force_ssh_logging: bool },
    ReconnectTab { tab_index: usize },
}

#[derive(Debug, Clone)]
pub(crate) struct PassPromptState {
    pub(crate) pass_key: String,
    pub(crate) passphrase: String,
    pub(crate) cursor: usize,
    pub(crate) attempts: usize,
    pub(crate) max_attempts: usize,
    pub(crate) error: Option<String>,
    pub(crate) action: PassPromptAction,
}

impl PassPromptState {
    pub(crate) fn new(pass_key: String, action: PassPromptAction) -> Self {
        Self {
            pass_key,
            passphrase: String::new(),
            cursor: 0,
            attempts: 0,
            max_attempts: PASS_PROMPT_MAX_ATTEMPTS,
            error: None,
            action,
        }
    }

    pub(crate) fn masked_passphrase(&self) -> String {
        "*".repeat(self.passphrase.chars().count())
    }

    pub(crate) fn remaining_attempts(&self) -> usize {
        self.max_attempts.saturating_sub(self.attempts)
    }

    pub(crate) fn clear_passphrase(&mut self) {
        self.passphrase.clear();
        self.cursor = 0;
    }

    pub(crate) fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub(crate) fn move_cursor_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.passphrase.chars().count());
    }

    pub(crate) fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub(crate) fn move_cursor_end(&mut self) {
        self.cursor = self.passphrase.chars().count();
    }

    pub(crate) fn insert_char(&mut self, ch: char) {
        let insert_at = byte_index_for_char(&self.passphrase, self.cursor);
        self.passphrase.insert(insert_at, ch);
        self.cursor += 1;
    }

    pub(crate) fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let end = byte_index_for_char(&self.passphrase, self.cursor);
        let start = byte_index_for_char(&self.passphrase, self.cursor - 1);
        self.passphrase.replace_range(start..end, "");
        self.cursor -= 1;
    }

    pub(crate) fn delete(&mut self) {
        let len = self.passphrase.chars().count();
        if self.cursor >= len {
            return;
        }
        let start = byte_index_for_char(&self.passphrase, self.cursor);
        let end = byte_index_for_char(&self.passphrase, self.cursor + 1);
        self.passphrase.replace_range(start..end, "");
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
