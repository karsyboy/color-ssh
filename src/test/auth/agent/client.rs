use super::spawn_child_waiter;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
#[test]
fn background_waiter_reaps_repeated_agent_children() {
    for _ in 0..5 {
        let child = Command::new("/bin/sh").args(["-c", "exit 0"]).spawn().expect("spawn test child");
        let child_proc = format!("/proc/{}", child.id());

        spawn_child_waiter(child).expect("spawn child waiter");

        let deadline = Instant::now() + Duration::from_secs(2);
        while Path::new(&child_proc).exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }

        assert!(!Path::new(&child_proc).exists(), "child process was not reaped");
    }
}
