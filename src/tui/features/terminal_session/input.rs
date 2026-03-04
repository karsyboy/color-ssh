use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn modifier_parameter(modifiers: KeyModifiers) -> u8 {
    let mut param = 1u8;
    if modifiers.contains(KeyModifiers::SHIFT) {
        param = param.saturating_add(1);
    }
    if modifiers.contains(KeyModifiers::ALT) {
        param = param.saturating_add(2);
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        param = param.saturating_add(4);
    }
    param
}

fn prefix_with_escape(mut bytes: Vec<u8>) -> Vec<u8> {
    let mut prefixed = Vec::with_capacity(bytes.len() + 1);
    prefixed.push(0x1b);
    prefixed.append(&mut bytes);
    prefixed
}

fn encode_csi_cursor_key(final_byte: u8, modifiers: KeyModifiers) -> Vec<u8> {
    let base = vec![0x1b, b'[', final_byte];
    if modifiers.is_empty() {
        return base;
    }
    if modifiers == KeyModifiers::ALT {
        return prefix_with_escape(base);
    }

    let final_char = final_byte as char;
    format!("\x1b[1;{}{}", modifier_parameter(modifiers), final_char).into_bytes()
}

fn encode_csi_tilde_key(code: u8, modifiers: KeyModifiers) -> Vec<u8> {
    if modifiers.is_empty() {
        return format!("\x1b[{}~", code).into_bytes();
    }
    if modifiers == KeyModifiers::ALT {
        return prefix_with_escape(format!("\x1b[{}~", code).into_bytes());
    }

    format!("\x1b[{};{}~", code, modifier_parameter(modifiers)).into_bytes()
}

pub(crate) fn encode_key_event_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    let modifiers = key.modifiers & (KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL);

    let bytes = match key.code {
        KeyCode::Char(ch) => {
            let mut out = if modifiers.contains(KeyModifiers::CONTROL) {
                let control_byte = match ch {
                    '@' | ' ' => 0,
                    'a'..='z' => (ch as u8) - b'a' + 1,
                    'A'..='Z' => (ch as u8) - b'A' + 1,
                    '[' => 27,
                    '\\' => 28,
                    ']' => 29,
                    '^' => 30,
                    '_' => 31,
                    '?' => 127,
                    _ => ch as u8,
                };
                vec![control_byte]
            } else {
                ch.to_string().into_bytes()
            };

            if modifiers.contains(KeyModifiers::ALT) {
                out = prefix_with_escape(out);
            }
            out
        }
        KeyCode::Enter => {
            let out = vec![b'\r'];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Backspace => {
            let out = vec![127];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Tab => {
            let out = vec![b'\t'];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Esc => {
            let out = vec![27];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Up => encode_csi_cursor_key(b'A', modifiers),
        KeyCode::Down => encode_csi_cursor_key(b'B', modifiers),
        KeyCode::Right => encode_csi_cursor_key(b'C', modifiers),
        KeyCode::Left => encode_csi_cursor_key(b'D', modifiers),
        KeyCode::Home => encode_csi_cursor_key(b'H', modifiers),
        KeyCode::End => encode_csi_cursor_key(b'F', modifiers),
        KeyCode::PageUp => encode_csi_tilde_key(5, modifiers),
        KeyCode::PageDown => encode_csi_tilde_key(6, modifiers),
        KeyCode::Delete => encode_csi_tilde_key(3, modifiers),
        KeyCode::Insert => encode_csi_tilde_key(2, modifiers),
        _ => return None,
    };

    Some(bytes)
}
