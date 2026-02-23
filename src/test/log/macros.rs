use crate::log::LOGGER;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

static LOG_MODE_TEST_LOCK: Mutex<()> = Mutex::new(());
static FORMAT_SIDE_EFFECT: AtomicUsize = AtomicUsize::new(0);

fn side_effect_value() -> usize {
    FORMAT_SIDE_EFFECT.fetch_add(1, Ordering::Relaxed);
    42
}

#[test]
fn log_debug_does_not_evaluate_format_args_when_disabled() {
    let _guard = LOG_MODE_TEST_LOCK.lock().expect("test lock");
    let was_debug = LOGGER.is_debug_enabled();
    LOGGER.disable_debug();
    FORMAT_SIDE_EFFECT.store(0, Ordering::Relaxed);

    crate::log_debug!("debug side effect {}", side_effect_value());
    assert_eq!(FORMAT_SIDE_EFFECT.load(Ordering::Relaxed), 0);

    if was_debug {
        LOGGER.enable_debug();
    } else {
        LOGGER.disable_debug();
    }
}

#[test]
fn log_ssh_does_not_evaluate_format_args_when_disabled() {
    let _guard = LOG_MODE_TEST_LOCK.lock().expect("test lock");
    let was_enabled = LOGGER.is_ssh_logging_enabled();
    LOGGER.disable_ssh_logging();
    FORMAT_SIDE_EFFECT.store(0, Ordering::Relaxed);

    crate::log_ssh!("ssh side effect {}", side_effect_value());
    assert_eq!(FORMAT_SIDE_EFFECT.load(Ordering::Relaxed), 0);

    if was_enabled {
        LOGGER.enable_ssh_logging();
    } else {
        LOGGER.disable_ssh_logging();
    }
}
