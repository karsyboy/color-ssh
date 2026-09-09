use super::{
    DirectRuntimePtySyncDecision, DirectRuntimeResizeDecision, DirectRuntimeViewportState, InteractiveSshRuntime, direct_runtime_exit_cleanup_sequence,
    direct_runtime_inline_viewport_height, direct_runtime_resize_dimensions, direct_runtime_resize_rows, encode_mouse_event, infer_scrolled_line_count,
    select_interactive_ssh_runtime, spawn_exit_watcher, take_latest_reload_notice_toast,
};
use crate::config;
use crate::runtime::format_reload_notice;
use crate::terminal::{MouseProtocolEncoding, MouseProtocolMode, TerminalChild, TerminalEngine, TerminalHostCallbacks, TerminalSession};
use crate::test::support::state::TestStateGuard;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use portable_pty::{Child as PtyChild, ChildKiller, ExitStatus};
use ratatui::layout::Rect;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct LiveMockChild {
    observed_by_watcher: Arc<AtomicBool>,
    killed: Arc<AtomicBool>,
    reaped: Arc<AtomicBool>,
}

#[derive(Debug)]
struct LiveMockChildKiller {
    killed: Arc<AtomicBool>,
}

impl ChildKiller for LiveMockChild {
    fn kill(&mut self) -> std::io::Result<()> {
        self.killed.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(LiveMockChildKiller {
            killed: Arc::clone(&self.killed),
        })
    }
}

impl ChildKiller for LiveMockChildKiller {
    fn kill(&mut self) -> std::io::Result<()> {
        self.killed.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(Self {
            killed: Arc::clone(&self.killed),
        })
    }
}

impl PtyChild for LiveMockChild {
    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.observed_by_watcher.store(true, Ordering::SeqCst);
        if self.killed.load(Ordering::SeqCst) {
            self.reaped.store(true, Ordering::SeqCst);
            Ok(Some(ExitStatus::with_exit_code(0)))
        } else {
            Ok(None)
        }
    }

    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.observed_by_watcher.store(true, Ordering::SeqCst);
        while !self.killed.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
        self.reaped.store(true, Ordering::SeqCst);
        Ok(ExitStatus::with_exit_code(0))
    }

    fn process_id(&self) -> Option<u32> {
        Some(42)
    }
}

#[test]
fn select_interactive_ssh_runtime_prefers_pty_only_for_direct_terminals() {
    assert_eq!(select_interactive_ssh_runtime(true), InteractiveSshRuntime::PtyCentered);
    assert_eq!(select_interactive_ssh_runtime(false), InteractiveSshRuntime::CompatibilityPassthrough);
}

#[test]
fn frontend_failure_cleanup_terminates_and_reaps_live_child_without_blocking() {
    let observed_by_watcher = Arc::new(AtomicBool::new(false));
    let killed = Arc::new(AtomicBool::new(false));
    let reaped = Arc::new(AtomicBool::new(false));
    let child: Arc<Mutex<Box<dyn PtyChild + Send + Sync>>> = Arc::new(Mutex::new(Box::new(LiveMockChild {
        observed_by_watcher: Arc::clone(&observed_by_watcher),
        killed: Arc::clone(&killed),
        reaped: Arc::clone(&reaped),
    })));
    let exited = Arc::new(Mutex::new(false));
    let (event_tx, event_rx) = mpsc::sync_channel(1);
    spawn_exit_watcher(Arc::clone(&child), Arc::clone(&exited), event_tx).expect("spawn exit watcher");

    let observation_deadline = Instant::now() + Duration::from_secs(1);
    while !observed_by_watcher.load(Ordering::SeqCst) && Instant::now() < observation_deadline {
        thread::yield_now();
    }
    assert!(observed_by_watcher.load(Ordering::SeqCst), "exit watcher did not observe child");

    let engine = Arc::new(Mutex::new(TerminalEngine::new_with_host_and_remote_clipboard_policy(
        24,
        80,
        100,
        TerminalHostCallbacks::default(),
        false,
        1024,
    )));
    let mut session = TerminalSession::new(None, None, TerminalChild::Pty(child), engine, Arc::clone(&exited), Arc::new(AtomicU64::new(0)));
    let (cleanup_tx, cleanup_rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        session.terminate();
        let _ = cleanup_tx.send(());
    });

    cleanup_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("frontend failure cleanup blocked on live child");
    let exit_event = event_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("exit watcher did not reap terminated child");

    assert!(matches!(exit_event, super::PtyRuntimeEvent::Exited(Ok(_))));
    assert!(killed.load(Ordering::SeqCst), "cleanup did not terminate child");
    assert!(reaped.load(Ordering::SeqCst), "exit watcher did not reap child");
    assert!(*exited.lock().expect("exited state lock"));
}

#[test]
fn direct_runtime_inline_viewport_uses_full_height_with_minimum_one_row() {
    assert_eq!(direct_runtime_inline_viewport_height(24), 24);
    assert_eq!(direct_runtime_inline_viewport_height(0), 1);
}

#[test]
fn direct_runtime_resize_rows_tracks_terminal_height_with_minimum_one_row() {
    assert_eq!(direct_runtime_resize_rows(24), 24);
    assert_eq!(direct_runtime_resize_rows(1), 1);
    assert_eq!(direct_runtime_resize_rows(0), 1);
}

#[test]
fn direct_runtime_resize_dimensions_clamp_to_valid_pty_size() {
    assert_eq!(direct_runtime_resize_dimensions(120, 24), (120, 24));
    assert_eq!(direct_runtime_resize_dimensions(0, 24), (1, 24));
    assert_eq!(direct_runtime_resize_dimensions(120, 0), (120, 1));
}

#[test]
fn direct_runtime_viewport_state_noops_when_terminal_size_is_unchanged() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(state.observe_terminal_size(120, 24), DirectRuntimeResizeDecision::Noop);
}

#[test]
fn direct_runtime_viewport_state_shrink_requests_redraw_without_rebuild() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(state.observe_terminal_size(100, 20), DirectRuntimeResizeDecision::Redraw);
    assert_eq!(state.inline_viewport_height(), 24);
}

#[test]
fn direct_runtime_viewport_state_growth_above_inline_cap_triggers_rebuild() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(
        state.observe_terminal_size(120, 40),
        DirectRuntimeResizeDecision::RebuildTerminal { inline_viewport_height: 40 }
    );
    assert_eq!(state.inline_viewport_height(), 40);
}

#[test]
fn direct_runtime_viewport_state_width_only_change_does_not_rebuild() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(state.observe_terminal_size(140, 24), DirectRuntimeResizeDecision::Redraw);
    assert_eq!(state.inline_viewport_height(), 24);
}

#[test]
fn direct_runtime_viewport_state_shrink_sync_uses_drawn_viewport_size() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(
        state.record_drawn_viewport(Rect::new(0, 0, 80, 20)),
        DirectRuntimePtySyncDecision::ResizePty { cols: 80, rows: 20 }
    );
}

#[test]
fn direct_runtime_viewport_state_duplicate_drawn_viewport_does_not_resend_pty_resize() {
    let mut state = DirectRuntimeViewportState::new(120, 24);

    assert_eq!(state.record_drawn_viewport(Rect::new(0, 0, 120, 24)), DirectRuntimePtySyncDecision::Noop);
    assert_eq!(
        state.record_drawn_viewport(Rect::new(0, 0, 90, 18)),
        DirectRuntimePtySyncDecision::ResizePty { cols: 90, rows: 18 }
    );
    assert_eq!(state.record_drawn_viewport(Rect::new(0, 0, 90, 18)), DirectRuntimePtySyncDecision::Noop);
}

#[test]
fn direct_runtime_exit_cleanup_sequence_resets_and_appends_newline() {
    assert_eq!(direct_runtime_exit_cleanup_sequence(), "\x1b[0m\r\n");
}

#[test]
fn mouse_event_encoding_uses_sgr_release_suffix_for_release_events() {
    let mouse = MouseEvent {
        kind: MouseEventKind::Up(MouseButton::Left),
        column: 1,
        row: 1,
        modifiers: KeyModifiers::empty(),
    };

    let bytes = encode_mouse_event(mouse, Rect::new(0, 0, 20, 5), MouseProtocolMode::ButtonMotion, MouseProtocolEncoding::Sgr).expect("encoded mouse release");

    assert_eq!(String::from_utf8(bytes).expect("utf8 mouse release"), "\x1b[<0;2;2m");
}

#[test]
fn mouse_event_encoding_ignores_events_outside_viewport() {
    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 30,
        row: 10,
        modifiers: KeyModifiers::empty(),
    };

    assert!(encode_mouse_event(mouse, Rect::new(0, 0, 20, 5), MouseProtocolMode::Press, MouseProtocolEncoding::Default).is_none());
}

#[test]
fn infer_scrolled_line_count_detects_simple_terminal_scroll() {
    let previous = vec!["line1".to_string(), "line2".to_string(), "line3".to_string()];
    let current = vec!["line2".to_string(), "line3".to_string(), "line4".to_string()];

    assert_eq!(infer_scrolled_line_count(&previous, &current), 1);
}

#[test]
fn format_reload_notice_prefixes_message() {
    assert_eq!(format_reload_notice("Config reloaded successfully"), "[color-ssh] Config reloaded successfully");
}

#[test]
fn take_latest_reload_notice_toast_uses_latest_notice() {
    let _state = TestStateGuard::lock();
    config::queue_reload_notice("Config reloaded successfully".to_string());
    config::queue_reload_notice("Config reload failed: parse error at line 77".to_string());

    let toast = take_latest_reload_notice_toast().expect("reload notice toast");

    assert_eq!(toast.message(), "[color-ssh] Config reload failed: parse error at line 77");
}
