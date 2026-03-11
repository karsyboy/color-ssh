use super::*;
use crate::config::{self, AuthSettings};
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::test::support::{fs::TestWorkspace, state::TestStateGuard};
use crate::tui::VaultUnlockAction;
use std::io::{self, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[path = "launch/auth_resolution.rs"]
mod auth_resolution;
#[path = "launch/captured_output.rs"]
mod captured_output;
#[path = "launch/profile_reload.rs"]
mod profile_reload;

fn launch_config_yaml(tui_password_autologin: bool, history_buffer: usize) -> String {
    format!(
        "auth_settings:\n  tui_password_autologin: {}\ninteractive_settings:\n  history_buffer: {}\npalette: {{}}\nrules: []\n",
        if tui_password_autologin { "true" } else { "false" },
        history_buffer
    )
}

fn sample_rdp_host() -> InventoryHost {
    let mut host = InventoryHost::new("desktop01".to_string());
    host.protocol = ConnectionProtocol::Rdp;
    host.host = "rdp.internal".to_string();
    host.user = Some("alice".to_string());
    host
}

fn with_profile_test_environment<T>(workspace: &TestWorkspace, run: impl FnOnce() -> T) -> T {
    let state = TestStateGuard::lock();
    let home_dir = workspace.join("home");
    let current_dir = workspace.join("cwd");

    state
        .with_home_and_cwd(&home_dir, &current_dir, run)
        .expect("configure HOME and cwd for profile tests")
}

struct TrackingWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
    dropped: Arc<AtomicBool>,
}

impl Write for TrackingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.lock().expect("writer bytes").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for TrackingWriter {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::Relaxed);
    }
}
