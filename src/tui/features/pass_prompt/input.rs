//! #_pass prompt keyboard handling.

use crate::auth::pass::{self, PassFallbackReason, PassPromptSubmitResult};
use crate::tui::{PassPromptAction, PassPromptState, SessionManager};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use zeroize::Zeroize;

const PASS_PROMPT_CANCEL_NOTICE: &str = "Password auto-login canceled; falling back to standard SSH password prompt.";
const PASS_PROMPT_RETRY_NOTICE: &str = "Invalid GPG passphrase. Try again.";

impl SessionManager {
    pub(crate) fn open_pass_prompt(&mut self, pass_key: String, action: PassPromptAction) {
        self.quick_connect = None;
        self.pass_prompt = Some(PassPromptState::new(pass_key, action));
        self.mark_ui_dirty();
    }

    pub(crate) fn handle_pass_prompt_key(&mut self, key: KeyEvent) {
        let Some(prompt) = self.pass_prompt.as_mut() else {
            return;
        };

        match key.code {
            KeyCode::Esc => {
                let action = prompt.action.clone();
                self.pass_prompt = None;
                self.complete_pass_prompt_action(action, None, Some(PASS_PROMPT_CANCEL_NOTICE.to_string()));
            }
            KeyCode::Enter => {
                self.submit_pass_prompt();
            }
            KeyCode::Left => {
                prompt.move_cursor_left();
            }
            KeyCode::Right => {
                prompt.move_cursor_right();
            }
            KeyCode::Home => {
                prompt.move_cursor_home();
            }
            KeyCode::End => {
                prompt.move_cursor_end();
            }
            KeyCode::Backspace => {
                prompt.backspace();
                prompt.error = None;
            }
            KeyCode::Delete => {
                prompt.delete();
                prompt.error = None;
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                prompt.move_cursor_home();
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                prompt.move_cursor_end();
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                prompt.insert_char(ch);
                prompt.error = None;
            }
            _ => {}
        }
    }

    pub(crate) fn handle_pass_prompt_paste(&mut self, pasted: &str) {
        let Some(prompt) = self.pass_prompt.as_mut() else {
            return;
        };

        let filtered: String = pasted.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }

        for ch in filtered.chars() {
            prompt.insert_char(ch);
        }
        prompt.error = None;
    }

    pub(crate) fn submit_pass_prompt(&mut self) {
        let Some(mut prompt) = self.pass_prompt.take() else {
            return;
        };

        let mut passphrase = std::mem::take(&mut prompt.passphrase);
        let action = prompt.action.clone();
        let pass_key = prompt.pass_key.clone();

        let submit_result = pass::submit_tui_passphrase(&pass_key, &passphrase, &mut self.pass_cache);
        passphrase.zeroize();

        match submit_result {
            PassPromptSubmitResult::Ready(password) => {
                self.complete_pass_prompt_action(action, Some(password), None);
            }
            PassPromptSubmitResult::Fallback(reason) => {
                self.complete_pass_prompt_action(action, None, Some(pass::fallback_notice(reason)));
            }
            PassPromptSubmitResult::InvalidPassphrase => {
                prompt.attempts += 1;
                if prompt.attempts >= prompt.max_attempts {
                    self.complete_pass_prompt_action(action, None, Some(pass::fallback_notice(PassFallbackReason::DecryptFailedAfterRetries)));
                    return;
                }

                prompt.error = Some(PASS_PROMPT_RETRY_NOTICE.to_string());
                prompt.clear_passphrase();
                self.pass_prompt = Some(prompt);
            }
        }
    }
}
