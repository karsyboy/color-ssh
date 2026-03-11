use super::{AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MIN, AgentRuntime, next_idle_shutdown_poll_interval};
use crate::auth::ipc::UnlockPolicy;
use std::time::{Duration, Instant};

#[test]
fn runtime_expiry_and_poll_backoff_follow_timeout_rules() {
    let mut runtime = AgentRuntime::new();
    runtime.unlock([7u8; 32], UnlockPolicy::new(1, 10));
    runtime.last_activity_at = Some(Instant::now() - Duration::from_secs(2));
    assert!(runtime.expire_if_needed());
    assert!(runtime.data_key.is_none());

    let first = next_idle_shutdown_poll_interval(AGENT_IDLE_SHUTDOWN_POLL_INTERVAL_MIN);
    let second = next_idle_shutdown_poll_interval(first);
    assert_eq!(first, Duration::from_millis(10));
    assert_eq!(second, Duration::from_millis(20));
}
