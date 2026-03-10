use super::{TerminalEventListener, TerminalHostCallbacks};
use crate::terminal_core::{TerminalClipboardTarget, TerminalInputWriter};
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::term::ClipboardType;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

struct SharedBufferWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl SharedBufferWriter {
    fn new(buffer: Arc<Mutex<Vec<u8>>>) -> Self {
        Self { buffer }
    }
}

impl Write for SharedBufferWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut buffer = self.buffer.lock().map_err(|err| io::Error::other(err.to_string()))?;
        buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn clipboard_store_uses_host_callback() {
    let clipboard_store = Arc::new(Mutex::new(Vec::new()));
    let callbacks = TerminalHostCallbacks::default().with_clipboard_store({
        let clipboard_store = clipboard_store.clone();
        move |target, text| {
            clipboard_store.lock().expect("clipboard store").push((target, text.to_string()));
        }
    });
    let listener = TerminalEventListener::new_with_host_and_remote_clipboard_policy(2, 2, None, callbacks, true, 1024);

    listener.send_event(Event::ClipboardStore(ClipboardType::Clipboard, "copied".to_string()));

    let stored = clipboard_store.lock().expect("stored clipboard");
    assert_eq!(stored.as_slice(), &[(TerminalClipboardTarget::Clipboard, "copied".to_string())]);
}

#[test]
fn clipboard_store_respects_remote_clipboard_policy() {
    let clipboard_store = Arc::new(Mutex::new(Vec::new()));
    let callbacks = TerminalHostCallbacks::default().with_clipboard_store({
        let clipboard_store = clipboard_store.clone();
        move |target, text| {
            clipboard_store.lock().expect("clipboard store").push((target, text.to_string()));
        }
    });
    let listener = TerminalEventListener::new_with_host_and_remote_clipboard_policy(2, 2, None, callbacks, false, 1024);

    listener.send_event(Event::ClipboardStore(ClipboardType::Clipboard, "blocked".to_string()));

    assert!(clipboard_store.lock().expect("stored clipboard").is_empty());
}

#[test]
fn clipboard_load_formats_host_contents_back_into_input() {
    let captured_input = Arc::new(Mutex::new(Vec::new()));
    let input_writer: TerminalInputWriter = Arc::new(Mutex::new(Box::new(SharedBufferWriter::new(captured_input.clone()))));
    let callbacks = TerminalHostCallbacks::default().with_clipboard_load(|target| match target {
        TerminalClipboardTarget::Clipboard => None,
        TerminalClipboardTarget::Selection => Some("selection-data".to_string()),
    });
    let listener = TerminalEventListener::new_with_host_and_remote_clipboard_policy(2, 2, Some(input_writer), callbacks, true, 1024);

    listener.send_event(Event::ClipboardLoad(ClipboardType::Selection, Arc::new(|text| format!("reply:{text}"))));

    let captured = captured_input.lock().expect("captured input");
    assert_eq!(String::from_utf8(captured.clone()).expect("utf8 input"), "reply:selection-data");
}
