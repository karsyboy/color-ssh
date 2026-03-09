use super::*;
use crate::config::{self, AuthSettings};
use crate::inventory::{ConnectionProtocol, InventoryHost};
use crate::test::support::{config::base_config, fs::TestWorkspace};
use crate::tui::VaultUnlockAction;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

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
