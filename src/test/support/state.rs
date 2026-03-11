use crate::config;
use crate::test::support::config::base_config;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

static TEST_STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn reset_config_runtime_state() {
    config::with_current_config_mut("reset test config runtime state", |cfg| *cfg = base_config());
    config::set_config_version(0);
    let _ = config::take_reload_notices();
    let _ = config::take_profile_reload_events();
}

pub(crate) struct TestStateGuard {
    _lock: MutexGuard<'static, ()>,
}

impl TestStateGuard {
    pub(crate) fn lock() -> Self {
        let lock = TEST_STATE_LOCK.get_or_init(|| Mutex::new(())).lock().expect("test state lock");
        reset_config_runtime_state();
        Self { _lock: lock }
    }

    pub(crate) fn enter_home_and_cwd(&self, home: &Path, cwd: &Path) -> io::Result<HomeAndCwdGuard> {
        HomeAndCwdGuard::enter(home, cwd)
    }
}

impl Drop for TestStateGuard {
    fn drop(&mut self) {
        reset_config_runtime_state();
    }
}

pub(crate) struct HomeAndCwdGuard {
    previous_dir: PathBuf,
    previous_home: Option<OsString>,
}

impl HomeAndCwdGuard {
    pub(crate) fn enter(home: &Path, cwd: &Path) -> io::Result<Self> {
        let previous_dir = std::env::current_dir()?;
        let previous_home = std::env::var_os("HOME");

        std::fs::create_dir_all(home.join(".color-ssh"))?;
        std::fs::create_dir_all(cwd)?;
        std::env::set_current_dir(cwd)?;
        unsafe {
            std::env::set_var("HOME", home);
        }

        Ok(Self { previous_dir, previous_home })
    }
}

impl Drop for HomeAndCwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous_dir);
        unsafe {
            match self.previous_home.as_ref() {
                Some(home) => std::env::set_var("HOME", home),
                None => std::env::remove_var("HOME"),
            }
        }
    }
}
