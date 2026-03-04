use crate::auth::vault::{UnlockedVault, VaultPaths, initialize_vault_with_paths, unlock_with_password_and_paths};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_base_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_nanos();
    let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("cossh_auth_{prefix}_{nanos}_{serial}"))
}

pub(crate) struct TestVaultEnv {
    paths: VaultPaths,
}

impl TestVaultEnv {
    pub(crate) fn new(prefix: &str) -> Self {
        Self {
            paths: VaultPaths::new(unique_base_dir(prefix)),
        }
    }

    pub(crate) fn paths(&self) -> &VaultPaths {
        &self.paths
    }

    pub(crate) fn init(&self, master_password: &str) {
        initialize_vault_with_paths(&self.paths, master_password).expect("initialize vault");
    }

    pub(crate) fn unlock(&self, master_password: &str) -> UnlockedVault {
        unlock_with_password_and_paths(&self.paths, master_password).expect("unlock vault")
    }

    pub(crate) fn init_and_unlock(&self, master_password: &str) -> UnlockedVault {
        self.init(master_password);
        self.unlock(master_password)
    }
}

impl Drop for TestVaultEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.paths.base_dir());
    }
}
