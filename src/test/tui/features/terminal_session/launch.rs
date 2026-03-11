use super::*;
use crate::config::CompiledHighlightRule;
use crate::config::{self, AuthSettings};
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::terminal::{TerminalEngine, highlight_overlay::HighlightOverlayContext, highlight_overlay::HighlightOverlayViewport};
use crate::test::support::{
    config::base_config,
    fs::TestWorkspace,
    state::{HomeAndCwdGuard, TestStateGuard},
};
use crate::tui::VaultUnlockAction;
use regex::Regex;
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

fn compiled_rule(pattern: &str, style: &str) -> CompiledHighlightRule {
    CompiledHighlightRule::new(Regex::new(pattern).expect("regex"), style.to_string())
}

fn build_overlay_for_text(overlay_engine: &mut HighlightOverlayEngine, text: &str, render_epoch: u64) -> crate::terminal::highlight_overlay::HighlightOverlay {
    let mut terminal_engine = TerminalEngine::new(2, 32, 128);
    terminal_engine.process_output(text.as_bytes());
    let view = terminal_engine.view_model();
    let viewport = view.viewport_snapshot(2, 32);
    let overlay_view = HighlightOverlayViewport::new(&viewport, view.is_alternate_screen(), view.mouse_protocol().0, view.cursor_hidden());
    overlay_engine.build_visible_overlay(
        &overlay_view,
        HighlightOverlayContext {
            render_epoch,
            display_scrollback: 0,
        },
    )
}

struct ProfileTestEnvironment {
    _state: TestStateGuard,
    _env: HomeAndCwdGuard,
}

impl ProfileTestEnvironment {
    fn enter(workspace: &TestWorkspace) -> Self {
        let state = TestStateGuard::lock();
        let home_dir = workspace.join("home");
        let current_dir = workspace.join("cwd");
        let env = state
            .enter_home_and_cwd(&home_dir, &current_dir)
            .expect("configure HOME and cwd for profile tests");

        Self { _state: state, _env: env }
    }
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
