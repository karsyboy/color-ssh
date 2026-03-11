use crate::config;
use crate::test::support::config::base_config;
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

    pub(crate) fn with_home_and_cwd<T>(&self, home: &Path, cwd: &Path, run: impl FnOnce() -> T) -> io::Result<T> {
        std::fs::create_dir_all(home.join(".color-ssh"))?;
        std::fs::create_dir_all(cwd)?;

        let _cwd = CurrentDirGuard::enter(cwd)?;
        Ok(temp_env::with_var("HOME", Some(home.as_os_str()), run))
    }
}

struct CurrentDirGuard {
    previous_dir: PathBuf,
}

impl CurrentDirGuard {
    fn enter(cwd: &Path) -> io::Result<Self> {
        let previous_dir = std::env::current_dir()?;
        std::env::set_current_dir(cwd)?;
        Ok(Self { previous_dir })
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous_dir);
    }
}

impl Drop for TestStateGuard {
    fn drop(&mut self) {
        reset_config_runtime_state();
    }
}
