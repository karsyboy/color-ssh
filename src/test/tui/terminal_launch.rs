use super::*;
use crate::config::{self, AuthSettings};
use crate::highlight_rules::CompiledHighlightRule;
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::terminal_core::{TerminalEngine, highlight_overlay::HighlightOverlayContext, highlight_overlay::HighlightOverlayViewport};
use crate::test::support::{config::base_config, fs::TestWorkspace};
use crate::tui::VaultUnlockAction;
use regex::Regex;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, MutexGuard, OnceLock,
    atomic::{AtomicBool, Ordering},
};

static PROFILE_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

struct ProfileTestEnvironment {
    _lock: MutexGuard<'static, ()>,
    previous_dir: PathBuf,
    previous_home: Option<OsString>,
}

impl ProfileTestEnvironment {
    fn enter(workspace: &TestWorkspace) -> Self {
        let lock = PROFILE_TEST_LOCK.get_or_init(|| Mutex::new(())).lock().expect("profile test lock");
        let previous_dir = std::env::current_dir().expect("current dir");
        let previous_home = std::env::var_os("HOME");
        let home_dir = workspace.join("home");
        let current_dir = workspace.join("cwd");
        std::fs::create_dir_all(home_dir.join(".color-ssh")).expect("create temp config dir");
        std::fs::create_dir_all(&current_dir).expect("create temp current dir");
        std::env::set_current_dir(&current_dir).expect("set current dir");
        unsafe {
            std::env::set_var("HOME", &home_dir);
        }
        config::with_current_config_mut("reset test config before profile launch test", |cfg| *cfg = base_config());
        config::set_config_version(0);

        Self {
            _lock: lock,
            previous_dir,
            previous_home,
        }
    }
}

impl Drop for ProfileTestEnvironment {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous_dir);
        unsafe {
            match self.previous_home.as_ref() {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
        }
        config::with_current_config_mut("reset test config after profile launch test", |cfg| *cfg = base_config());
        config::set_config_version(0);
    }
}

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

fn build_overlay_for_text(
    overlay_engine: &mut HighlightOverlayEngine,
    text: &str,
    render_epoch: u64,
) -> crate::terminal_core::highlight_overlay::HighlightOverlay {
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

#[test]
fn auto_login_notice_for_rdp_mentions_freerdp_prompt() {
    let host = sample_rdp_host();

    let notice = auto_login_notice(&host, "Password vault unlock canceled");

    assert!(notice.contains("FreeRDP password prompt"));
    assert!(!notice.contains("standard SSH password prompt"));
}

#[test]
fn resolve_host_pass_password_for_rdp_without_vault_pass_is_launchable() {
    let mut app = AppState::new_for_tests();
    let host = sample_rdp_host();
    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host.clone()),
        force_ssh_logging: false,
        auth_settings: AuthSettings::default(),
    };

    let resolution = app.resolve_host_pass_password_with_autologin(&host, action, true);

    assert_eq!(
        resolution,
        Some(HostPassResolution {
            pass_entry_override: None,
            pass_fallback_notice: None,
            disable_vault_autologin: false,
        })
    );
}

#[test]
fn resolve_host_pass_password_for_rdp_with_tui_autologin_disabled_is_launchable() {
    let mut app = AppState::new_for_tests();
    let mut host = sample_rdp_host();
    host.vault_pass = Some("shared".to_string());
    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host.clone()),
        force_ssh_logging: false,
        auth_settings: AuthSettings::default(),
    };

    let resolution = app.resolve_host_pass_password_with_autologin(&host, action, false);

    assert_eq!(
        resolution,
        Some(HostPassResolution {
            pass_entry_override: None,
            pass_fallback_notice: None,
            disable_vault_autologin: true,
        })
    );
}

#[test]
fn startup_only_rdp_launch_payload_closes_writer_after_write() {
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let dropped = Arc::new(AtomicBool::new(false));
    let writer = TrackingWriter {
        bytes: bytes.clone(),
        dropped: dropped.clone(),
    };
    let payload = crate::auth::secret::sensitive_string("/u:alice\n/p:super-secret");

    write_startup_payload_and_close_stdin(Box::new(writer), Some(&payload)).expect("write startup payload");

    assert_eq!(
        String::from_utf8(bytes.lock().expect("payload bytes").clone()).expect("payload should be utf-8"),
        "/u:alice\n/p:super-secret"
    );
    assert!(dropped.load(Ordering::Relaxed));
}

#[test]
fn vault_backed_rdp_launch_uses_captured_output_mode() {
    let payload = crate::auth::secret::sensitive_string("/u:alice\n/p:super-secret");

    assert_eq!(rdp_session_launch_mode(Some(&payload)), RdpSessionLaunchMode::CapturedOutput);
}

#[test]
fn prompt_backed_rdp_launch_uses_pty_mode() {
    assert_eq!(rdp_session_launch_mode(None), RdpSessionLaunchMode::Pty);
}

#[test]
fn captured_output_newlines_expand_to_crlf() {
    let mut normalizer = CapturedOutputNewlineNormalizer::default();

    let normalized = normalize_captured_output_chunk(&mut normalizer, b"first line\nsecond line\n");

    assert_eq!(normalized, b"first line\r\nsecond line\r\n");
}

#[test]
fn captured_output_newlines_preserve_chunked_crlf_sequences() {
    let mut normalizer = CapturedOutputNewlineNormalizer::default();

    let first_chunk = normalize_captured_output_chunk(&mut normalizer, b"first line\r");
    let second_chunk = normalize_captured_output_chunk(&mut normalizer, b"\nsecond line\n");

    assert_eq!(first_chunk, b"first line\r");
    assert_eq!(second_chunk, b"\nsecond line\r\n");
}

#[test]
fn resolve_host_pass_password_uses_inventory_profile_auth_settings() {
    let workspace = TestWorkspace::new("tui", "profile_launch").expect("test workspace");
    let _env = ProfileTestEnvironment::enter(&workspace);
    workspace
        .write_rel("home/.color-ssh/cossh-config.yaml", &launch_config_yaml(true, 1000))
        .expect("write default config");
    workspace
        .write_rel("home/.color-ssh/network.cossh-config.yaml", &launch_config_yaml(false, 2048))
        .expect("write profile config");
    config::init_session_config(None).expect("load default config");

    let mut app = AppState::new_for_tests();
    let mut host = InventoryHost::new("router01".to_string());
    host.host = "10.0.0.10".to_string();
    host.profile = Some("network".to_string());
    host.vault_pass = Some("shared".to_string());

    let session_profile = AppState::resolve_session_profile(&host).expect("resolve host profile settings");
    assert_eq!(session_profile.history_buffer, 2048);
    assert!(!session_profile.auth_settings.tui_password_autologin);

    let action = VaultUnlockAction::OpenHostTab {
        host: Box::new(host.clone()),
        force_ssh_logging: false,
        auth_settings: session_profile.auth_settings.clone(),
    };
    let resolution = app.resolve_host_pass_password(&host, action, &session_profile.auth_settings);

    assert_eq!(
        resolution,
        Some(HostPassResolution {
            pass_entry_override: None,
            pass_fallback_notice: None,
            disable_vault_autologin: true,
        })
    );
}

#[test]
fn resolve_session_profile_includes_profile_logging_settings_and_secret_patterns() {
    let workspace = TestWorkspace::new("tui", "profile_logging_snapshot").expect("test workspace");
    let _env = ProfileTestEnvironment::enter(&workspace);
    workspace
        .write_rel(
            "home/.color-ssh/network.cossh-config.yaml",
            "settings:\n  ssh_logging: true\n  remove_secrets:\n    - token=\\w+\nauth_settings:\n  tui_password_autologin: false\ninteractive_settings:\n  history_buffer: 2048\npalette: {}\nrules: []\n",
        )
        .expect("write profile config");

    let mut host = InventoryHost::new("router01".to_string());
    host.host = "10.0.0.10".to_string();
    host.profile = Some("network".to_string());

    let session_profile = AppState::resolve_session_profile(&host).expect("resolve host profile settings");

    assert!(session_profile.ssh_logging_enabled);
    assert_eq!(session_profile.secret_patterns.len(), 1);
}

#[test]
fn no_profile_tabs_use_live_current_config_overlay_rules() {
    let _lock = PROFILE_TEST_LOCK.get_or_init(|| Mutex::new(())).lock().expect("profile test lock");
    let mut config = base_config();
    config.metadata.compiled_rules = vec![compiled_rule("error", "\x1b[38;2;255;0;0m")];
    config.metadata.compiled_rule_set = Some(regex::RegexSet::new(["error"]).expect("rule set"));
    config::with_current_config_mut("install test overlay config", |current| *current = config);
    config::set_config_version(1);

    let host = InventoryHost::new("router01".to_string());
    let session_profile = AppState::resolve_session_profile(&host).expect("resolve current profile settings");
    let mut overlay_engine = highlight_overlay_for_host(&host, &session_profile);

    let mut terminal_engine = TerminalEngine::new(2, 20, 128);
    terminal_engine.process_output(b"error");
    let view = terminal_engine.view_model();
    let viewport = view.viewport_snapshot(2, 20);
    let overlay_view = HighlightOverlayViewport::new(&viewport, view.is_alternate_screen(), view.mouse_protocol().0, view.cursor_hidden());
    let first_overlay = overlay_engine.build_visible_overlay(
        &overlay_view,
        HighlightOverlayContext {
            render_epoch: 1,
            display_scrollback: 0,
        },
    );
    assert!(first_overlay.style_for_cell(0, 0).is_some());

    config::with_current_config_mut("update test overlay config", |current| {
        current.metadata.compiled_rules = vec![compiled_rule("warn", "\x1b[38;2;0;255;0m")];
        current.metadata.compiled_rule_set = Some(regex::RegexSet::new(["warn"]).expect("rule set"));
    });
    config::set_config_version(2);

    terminal_engine.process_output(b"\rwarn\x1b[K");
    let view = terminal_engine.view_model();
    let viewport = view.viewport_snapshot(2, 20);
    let overlay_view = HighlightOverlayViewport::new(&viewport, view.is_alternate_screen(), view.mouse_protocol().0, view.cursor_hidden());
    let second_overlay = overlay_engine.build_visible_overlay(
        &overlay_view,
        HighlightOverlayContext {
            render_epoch: 2,
            display_scrollback: 0,
        },
    );

    assert!(second_overlay.style_for_cell(0, 0).is_some());

    config::with_current_config_mut("reset test overlay config", |current| *current = base_config());
    config::set_config_version(0);
}

#[test]
fn profile_tabs_refresh_overlay_rules_when_profile_reload_event_arrives() {
    let workspace = TestWorkspace::new("tui", "profile_tab_reload").expect("test workspace");
    let _env = ProfileTestEnvironment::enter(&workspace);
    workspace
        .write_rel(
            "home/.color-ssh/cossh-config.yaml",
            "settings:\n  ssh_logging: false\nauth_settings:\n  tui_password_autologin: true\ninteractive_settings:\n  history_buffer: 1000\npalette: {}\nrules: []\n",
        )
        .expect("write default config");
    workspace
        .write_rel(
            "home/.color-ssh/linux.cossh-config.yaml",
            "interactive_settings:\n  history_buffer: 2048\npalette:\n  alert: '#ffc800'\nrules:\n  - regex: 'warn'\n    color: alert\n",
        )
        .expect("write linux profile config");
    config::init_session_config(None).expect("load default config");

    let mut host = InventoryHost::new("linux01".to_string());
    host.host = "10.0.0.20".to_string();
    host.profile = Some("linux".to_string());

    let session_profile = AppState::resolve_session_profile(&host).expect("resolve initial profile settings");
    let mut overlay_engine = highlight_overlay_for_host(&host, &session_profile);
    let overlay_before = build_overlay_for_text(&mut overlay_engine, "error", 1);
    assert!(overlay_before.style_for_cell(0, 0).is_none());

    let mut app = AppState::new_for_tests();
    app.tabs.push(crate::tui::HostTab {
        host: host.clone(),
        title: host.name.clone(),
        session: None,
        session_error: None,
        highlight_overlay: overlay_engine,
        scroll_offset: 0,
        terminal_search: crate::tui::TerminalSearchState::default(),
        force_ssh_logging: false,
        last_pty_size: None,
    });

    workspace
        .write_rel(
            "home/.color-ssh/linux.cossh-config.yaml",
            "interactive_settings:\n  history_buffer: 2048\nrules:\n  - regex: 'error'\n    color: alert\npalette:\n  alert: '#00ff00'\n",
        )
        .expect("rewrite linux profile config");

    config::queue_profile_reload_event(config::ProfileReloadEvent {
        profile: "linux".to_string(),
        message: "Config profile 'linux' reloaded successfully".to_string(),
        success: true,
    });

    app.apply_config_reload_notifications();

    let overlay_after = build_overlay_for_text(&mut app.tabs[0].highlight_overlay, "error", 2);
    assert!(overlay_after.style_for_cell(0, 0).is_some());
    assert_eq!(
        app.reload_notice_toast.as_ref().map(|toast| toast.message()),
        Some("[color-ssh] Config profile 'linux' reloaded successfully")
    );
}
