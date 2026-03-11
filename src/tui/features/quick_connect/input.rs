//! Quick-connect keyboard handling.

use crate::tui::{AppState, QuickConnectField, QuickConnectState, QuickConnectValidationError};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl AppState {
    pub(crate) fn open_quick_connect_modal(&mut self) {
        let profiles = self.discover_quick_connect_profiles();
        self.quick_connect = Some(QuickConnectState::new(self.quick_connect_default_ssh_logging, profiles));
        self.mark_ui_dirty();
    }

    pub(crate) fn handle_quick_connect_key(&mut self, key: KeyEvent) {
        let mut should_submit = false;
        let mut should_close = false;

        if let Some(form) = self.quick_connect.as_mut() {
            form.finish_mouse_selection();
            match key.code {
                KeyCode::Esc => {
                    should_close = true;
                }
                KeyCode::Tab | KeyCode::Down => {
                    form.select_next_field();
                }
                KeyCode::BackTab | KeyCode::Up => {
                    form.select_prev_field();
                }
                KeyCode::Enter => match form.selected {
                    QuickConnectField::Protocol => {
                        form.toggle_protocol_forward();
                    }
                    QuickConnectField::Profile => {
                        form.select_next_profile();
                    }
                    QuickConnectField::Logging => {
                        form.ssh_logging = !form.ssh_logging;
                    }
                    QuickConnectField::Connect => {
                        should_submit = true;
                    }
                    QuickConnectField::Cancel => {
                        should_close = true;
                    }
                    _ => {
                        form.select_next_field();
                    }
                },
                KeyCode::Char(' ') => match form.selected {
                    QuickConnectField::Protocol => form.toggle_protocol_forward(),
                    QuickConnectField::Logging => form.ssh_logging = !form.ssh_logging,
                    _ => {}
                },
                KeyCode::Left => match form.selected {
                    QuickConnectField::Protocol => form.toggle_protocol_backward(),
                    QuickConnectField::Profile => form.select_prev_profile(),
                    QuickConnectField::User | QuickConnectField::Host | QuickConnectField::Port | QuickConnectField::Domain | QuickConnectField::Password => {
                        form.move_cursor_left(form.selected)
                    }
                    _ => {}
                },
                KeyCode::Right => match form.selected {
                    QuickConnectField::Protocol => form.toggle_protocol_forward(),
                    QuickConnectField::Profile => form.select_next_profile(),
                    QuickConnectField::User | QuickConnectField::Host | QuickConnectField::Port | QuickConnectField::Domain | QuickConnectField::Password => {
                        form.move_cursor_right(form.selected)
                    }
                    _ => {}
                },
                KeyCode::Home => {
                    form.move_cursor_home(form.selected);
                }
                KeyCode::End => {
                    form.move_cursor_end(form.selected);
                }
                KeyCode::Backspace => {
                    form.backspace(form.selected);
                    if form.selected == QuickConnectField::Host {
                        form.host_required = false;
                    }
                    form.error = None;
                }
                KeyCode::Delete => {
                    form.delete(form.selected);
                    if form.selected == QuickConnectField::Host {
                        form.host_required = false;
                    }
                    form.error = None;
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                    form.insert_char(form.selected, ch);
                    if form.selected == QuickConnectField::Host {
                        form.host_required = false;
                    }
                    form.error = None;
                }
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    form.select_all(form.selected);
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    form.move_cursor_end(form.selected);
                }
                _ => {}
            }
        }

        if should_submit {
            self.submit_quick_connect_modal();
        } else if should_close {
            self.quick_connect = None;
            self.mark_ui_dirty();
        }
    }

    pub(crate) fn handle_quick_connect_paste(&mut self, pasted: &str) {
        let Some(form) = self.quick_connect.as_mut() else {
            return;
        };

        form.finish_mouse_selection();
        let filtered: String = pasted.chars().filter(|ch| !ch.is_control()).collect();
        if filtered.is_empty() {
            return;
        }

        match form.selected {
            QuickConnectField::User | QuickConnectField::Host | QuickConnectField::Port | QuickConnectField::Domain | QuickConnectField::Password => {
                let field = form.selected;
                for ch in filtered.chars() {
                    form.insert_char(field, ch);
                }
                if field == QuickConnectField::Host {
                    form.host_required = false;
                }
                form.error = None;
            }
            _ => {}
        }
    }

    pub(crate) fn submit_quick_connect_modal(&mut self) {
        let Some(form) = self.quick_connect.as_mut() else {
            return;
        };

        let submission = match form.build_submission() {
            Ok(submission) => submission,
            Err(err) => {
                form.host_required = matches!(err, QuickConnectValidationError::HostRequired);
                form.selected = match err {
                    QuickConnectValidationError::HostRequired => QuickConnectField::Host,
                    QuickConnectValidationError::RdpUserRequired => QuickConnectField::User,
                    QuickConnectValidationError::InvalidPort => QuickConnectField::Port,
                    QuickConnectValidationError::PasswordEncoding(_) => QuickConnectField::Password,
                };
                form.error = Some(err.message());
                self.mark_ui_dirty();
                return;
            }
        };

        form.host_required = false;
        form.error = None;
        self.open_quick_connect_host(submission);
    }
}
