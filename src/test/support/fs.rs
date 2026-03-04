use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_root(namespace: &str, prefix: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock drift").as_nanos();
    let serial = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("cossh_{namespace}_{prefix}_{nanos}_{serial}"))
}

pub(crate) struct TestWorkspace {
    root: PathBuf,
}

impl TestWorkspace {
    pub(crate) fn new(namespace: &str, prefix: &str) -> io::Result<Self> {
        let root = unique_root(namespace, prefix);
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub(crate) fn join(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub(crate) fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)
    }

    pub(crate) fn write_rel(&self, rel: &str, contents: &str) -> io::Result<PathBuf> {
        let path = self.join(rel);
        self.write(&path, contents)?;
        Ok(path)
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
