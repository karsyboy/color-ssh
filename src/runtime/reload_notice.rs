use std::time::{Duration, Instant};

pub(crate) const RELOAD_NOTICE_TOAST_DURATION: Duration = Duration::from_secs(5);

pub(crate) struct ReloadNoticeToast {
    message: String,
    shown_at: Instant,
}

impl ReloadNoticeToast {
    pub(crate) fn new(message: String) -> Self {
        Self {
            message,
            shown_at: Instant::now(),
        }
    }

    pub(crate) fn expired(&self) -> bool {
        self.shown_at.elapsed() >= RELOAD_NOTICE_TOAST_DURATION
    }

    pub(crate) fn message(&self) -> &str {
        self.message.as_str()
    }
}

pub(crate) fn format_reload_notice(message: &str) -> String {
    format!("[color-ssh] {message}")
}
