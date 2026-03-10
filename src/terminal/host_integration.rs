use super::{TerminalClipboardTarget, TerminalHostCallbacks};
use crossterm::clipboard::CopyToClipboard;
use crossterm::execute;
use std::io::{Write, stdout};

pub(crate) fn terminal_host_callbacks() -> TerminalHostCallbacks {
    TerminalHostCallbacks::default().with_clipboard_store(copy_to_clipboard)
}

pub(crate) fn copy_to_clipboard(target: TerminalClipboardTarget, text: &str) {
    let mut output = stdout();
    let command = match target {
        TerminalClipboardTarget::Clipboard => CopyToClipboard::to_clipboard_from(text),
        TerminalClipboardTarget::Selection => CopyToClipboard::to_primary_from(text),
    };
    let _ = execute!(output, command);
    let _ = output.flush();
}
