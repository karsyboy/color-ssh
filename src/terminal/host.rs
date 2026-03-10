use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalClipboardTarget {
    Clipboard,
    Selection,
}

type ClipboardStoreCallback = dyn Fn(TerminalClipboardTarget, &str) + Send + Sync;
type ClipboardLoadCallback = dyn Fn(TerminalClipboardTarget) -> Option<String> + Send + Sync;

#[derive(Clone, Default)]
pub(crate) struct TerminalHostCallbacks {
    clipboard_store: Option<Arc<ClipboardStoreCallback>>,
    clipboard_load: Option<Arc<ClipboardLoadCallback>>,
}

impl TerminalHostCallbacks {
    pub(crate) fn with_clipboard_store<F>(mut self, callback: F) -> Self
    where
        F: Fn(TerminalClipboardTarget, &str) + Send + Sync + 'static,
    {
        self.clipboard_store = Some(Arc::new(callback));
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn with_clipboard_load<F>(mut self, callback: F) -> Self
    where
        F: Fn(TerminalClipboardTarget) -> Option<String> + Send + Sync + 'static,
    {
        self.clipboard_load = Some(Arc::new(callback));
        self
    }

    pub(crate) fn store_clipboard(&self, target: TerminalClipboardTarget, text: &str) {
        if let Some(callback) = self.clipboard_store.as_ref() {
            callback(target, text);
        }
    }

    pub(crate) fn load_clipboard(&self, target: TerminalClipboardTarget) -> Option<String> {
        self.clipboard_load.as_ref().and_then(|callback| callback(target))
    }
}
