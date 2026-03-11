use super::{TerminalEventListener, TerminalHostCallbacks};
use crate::terminal::TerminalClipboardTarget;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::term::ClipboardType;
use std::sync::{Arc, Mutex};

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
