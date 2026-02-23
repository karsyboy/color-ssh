use super::should_flush;
use std::time::Duration;

#[test]
fn should_flush_on_size_or_interval() {
    assert!(!should_flush(512, Duration::from_millis(20)));
    assert!(should_flush(16 * 1024, Duration::from_millis(20)));
    assert!(should_flush(1, Duration::from_millis(100)));
}
