use crate::auth::secret::{ExposeSecret, SensitiveBuffer, SensitiveString};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, BufRead, Write};

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

pub(crate) fn prompt_hidden_secret(prompt: &str) -> io::Result<SensitiveString> {
    let mut stderr = io::stderr().lock();
    stderr.write_all(b"\r")?;
    stderr.write_all(prompt.as_bytes())?;
    stderr.flush()?;

    let _raw_mode: RawModeGuard = RawModeGuard::enter()?;
    let mut buffer = SensitiveBuffer::new();

    loop {
        match event::read()? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => match key.code {
                KeyCode::Enter => {
                    stderr.write_all(b"\r\n")?;
                    stderr.flush()?;
                    return buffer
                        .into_sensitive_string()
                        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, format!("failed to decode hidden input: {err}")));
                }
                KeyCode::Backspace => {
                    let cursor = buffer.char_len();
                    let _ = buffer.backspace_char(cursor);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    stderr.write_all(b"\r\n")?;
                    stderr.flush()?;
                    return Err(io::Error::new(io::ErrorKind::Interrupted, "input canceled"));
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                    buffer.insert_char(buffer.char_len(), ch);
                }
                _ => {}
            },
            Event::Paste(pasted) => {
                for ch in pasted.chars().filter(|ch| *ch != '\n' && *ch != '\r') {
                    buffer.insert_char(buffer.char_len(), ch);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn prompt_visible_value(prompt: &str) -> io::Result<String> {
    let mut stderr = io::stderr().lock();
    stderr.write_all(b"\r")?;
    stderr.write_all(prompt.as_bytes())?;
    stderr.flush()?;

    let mut line = String::new();
    let bytes_read = io::stdin().lock().read_line(&mut line)?;
    if bytes_read == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "input canceled"));
    }

    while matches!(line.chars().last(), Some('\n' | '\r')) {
        line.pop();
    }

    Ok(line)
}

pub(crate) fn confirm_hidden_value(prompt: &str, confirm_prompt: &str, empty_message: &str, mismatch_message: &str) -> Result<SensitiveString, String> {
    let value = prompt_hidden_secret(prompt).map_err(|err| err.to_string())?;
    let confirm = prompt_hidden_secret(confirm_prompt).map_err(|err| err.to_string())?;
    if value.expose_secret().is_empty() {
        return Err(empty_message.to_string());
    }
    if value != confirm {
        return Err(mismatch_message.to_string());
    }
    Ok(value)
}

pub(crate) fn prompt_new_master_password() -> Result<SensitiveString, String> {
    confirm_hidden_value(
        "Enter vault master password: ",
        "Confirm vault master password: ",
        "master password cannot be empty",
        "master password confirmation did not match",
    )
}

pub(crate) fn prompt_new_master_password_with_label(label: &str) -> Result<SensitiveString, String> {
    confirm_hidden_value(
        &format!("Enter {label} vault master password: "),
        &format!("Confirm {label} vault master password: "),
        "master password cannot be empty",
        "master password confirmation did not match",
    )
}

pub(crate) fn prompt_existing_master_password() -> Result<SensitiveString, String> {
    let password = prompt_hidden_secret("Enter vault master password: ").map_err(|err| err.to_string())?;
    if password.expose_secret().is_empty() {
        return Err("master password cannot be empty".to_string());
    }
    Ok(password)
}

pub(crate) fn prompt_existing_master_password_with_label(label: &str) -> Result<SensitiveString, String> {
    let password = prompt_hidden_secret(&format!("Enter {label} vault master password: ")).map_err(|err| err.to_string())?;
    if password.expose_secret().is_empty() {
        return Err("master password cannot be empty".to_string());
    }
    Ok(password)
}

pub(crate) fn prompt_entry_secret() -> Result<SensitiveString, String> {
    confirm_hidden_value(
        "Enter SSH password to store: ",
        "Confirm SSH password: ",
        "password cannot be empty",
        "password confirmation did not match",
    )
}
