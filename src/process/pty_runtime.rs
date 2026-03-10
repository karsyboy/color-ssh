//! PTY-centered interactive SSH runtime.
//!
//! This is the preferred direct `cossh ssh` architecture. It launches SSH in a
//! PTY, feeds raw PTY bytes into `alacritty_terminal`, and renders terminal
//! state from the canonical engine instead of rewriting stdout chunks.

use super::command_spec::PreparedCommand;
use super::exit::map_exit_code;
use super::{PtyLogTarget, spawn_pty_output_reader};
use crate::auth::secret::ExposeSecret;
use crate::reload_notice::{ReloadNoticeToast, format_reload_notice};
use crate::terminal_core::highlight_overlay::{HighlightOverlay, HighlightOverlayEngine};
use crate::terminal_core::{
    MouseProtocolEncoding, MouseProtocolMode, TerminalChild, TerminalEngine, TerminalFrontendSnapshot, TerminalInputWriter, TerminalSession, TerminalViewport,
    encode_key_event_bytes, encode_mouse_event_bytes, encode_paste_bytes,
};
use crate::terminal_host::terminal_host_callbacks;
use crate::terminal_ratatui::{apply_overlay_ranges, paint_terminal_viewport, render_reload_notice_toast};
use crate::{Result, command_path, config, log, log_debug, log_error};
use crossterm::{
    cursor::MoveTo,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    style::Print,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ratatui::{
    Frame, Terminal, TerminalOptions, Viewport,
    layout::{Position, Rect},
};
use std::io::{self, IsTerminal};
use std::sync::{
    Arc, Mutex,
    atomic::AtomicU64,
    mpsc::{self, Receiver, SyncSender, TryRecvError},
};
use std::thread;
use std::time::{Duration, Instant};

const DIRECT_RUNTIME_HISTORY_BUFFER: usize = 1000;
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(16);
const RENDER_HEARTBEAT: Duration = Duration::from_millis(250);
const PTY_EVENT_QUEUE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InteractiveSshRuntime {
    PtyCentered,
    CompatibilityPassthrough,
}

enum PtyRuntimeEvent {
    Output(Vec<u8>),
    ReaderClosed,
    Exited(io::Result<portable_pty::ExitStatus>),
}

struct InteractivePtyRuntime {
    session: TerminalSession,
    event_rx: Receiver<PtyRuntimeEvent>,
    highlight_overlay: HighlightOverlayEngine,
    host_scrollback: HostScrollbackMirror,
    reload_notice_toast: Option<ReloadNoticeToast>,
}

#[derive(Debug)]
struct HostScrollbackMirror {
    history_capacity: usize,
    last_history_size: usize,
    last_viewport_rows: u16,
    last_viewport_cols: u16,
    last_buffer_row_ids: Vec<usize>,
    valid: bool,
}

impl HostScrollbackMirror {
    fn new(history_capacity: usize) -> Self {
        Self {
            history_capacity,
            last_history_size: 0,
            last_viewport_rows: 0,
            last_viewport_cols: 0,
            last_buffer_row_ids: Vec::new(),
            valid: false,
        }
    }

    fn invalidate(&mut self) {
        self.valid = false;
        self.last_buffer_row_ids.clear();
    }

    fn sync_history(&mut self, history_size: usize) {
        self.last_history_size = history_size;
    }

    fn sync(&mut self, history_size: usize, viewport_rows: u16, viewport_cols: u16, buffer_row_ids: Vec<usize>) {
        self.last_history_size = history_size;
        self.last_viewport_rows = viewport_rows;
        self.last_viewport_cols = viewport_cols;
        self.last_buffer_row_ids = buffer_row_ids;
        self.valid = true;
    }
}

struct PendingScrollbackInsertion {
    snapshot: TerminalFrontendSnapshot,
}

struct CapturedScrollbackInsertions {
    pending: Vec<PendingScrollbackInsertion>,
}

impl CapturedScrollbackInsertions {
    fn empty() -> Self {
        Self { pending: Vec::new() }
    }
}

struct ScrollbackInsertion {
    viewport: TerminalViewport,
    overlay: HighlightOverlay,
}

#[derive(Debug, Default)]
struct TerminalModeGuard {
    active: bool,
    mouse_capture_enabled: bool,
}

impl TerminalModeGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnableBracketedPaste)?;
        Ok(Self {
            active: true,
            mouse_capture_enabled: false,
        })
    }

    fn sync_mouse_capture(&mut self, enabled: bool) -> io::Result<()> {
        if !self.active || self.mouse_capture_enabled == enabled {
            return Ok(());
        }

        let mut stdout = io::stdout();
        if enabled {
            execute!(stdout, EnableMouseCapture)?;
            log_debug!("Enabled direct-mode mouse capture for PTY mouse reporting");
        } else {
            execute!(stdout, DisableMouseCapture)?;
            log_debug!("Disabled direct-mode mouse capture after PTY mouse reporting stopped");
        }
        self.mouse_capture_enabled = enabled;
        Ok(())
    }

    fn cleanup(&mut self) {
        if !self.active {
            return;
        }

        let mut stdout = io::stdout();
        if self.mouse_capture_enabled {
            let _ = execute!(stdout, DisableMouseCapture);
            self.mouse_capture_enabled = false;
        }
        let _ = execute!(stdout, DisableBracketedPaste);
        let _ = disable_raw_mode();
        self.active = false;
    }
}

impl Drop for TerminalModeGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub(super) fn prefer_pty_centered_ssh_runtime() -> bool {
    let has_interactive_tty = io::stdin().is_terminal() && io::stdout().is_terminal();

    matches!(select_interactive_ssh_runtime(has_interactive_tty), InteractiveSshRuntime::PtyCentered)
}

pub(super) fn run_interactive_ssh(mut command_spec: PreparedCommand) -> Result<std::process::ExitCode> {
    let fallback_notice = command_spec.fallback_notice.take();
    let stdin_payload = command_spec.stdin_payload.take();
    let history_buffer = direct_history_buffer();
    let mut runtime = spawn_interactive_pty_runtime(command_spec, history_buffer)?;

    if let Some(notice) = fallback_notice {
        let message = format!("\r\n[color-ssh] {}\r\n", notice);
        process_pty_output(&runtime.session, message.as_bytes())?;
    }

    if let Some(stdin_payload) = stdin_payload {
        runtime.session.write_input(stdin_payload.expose_secret().as_bytes())?;
    }

    let result = run_pty_event_loop(&mut runtime);
    if result.is_err() {
        runtime.session.terminate();
    }
    result
}

fn select_interactive_ssh_runtime(has_interactive_tty: bool) -> InteractiveSshRuntime {
    if !has_interactive_tty {
        InteractiveSshRuntime::CompatibilityPassthrough
    } else {
        InteractiveSshRuntime::PtyCentered
    }
}

fn direct_history_buffer() -> usize {
    config::with_current_config("reading direct interactive history buffer", |cfg| {
        cfg.interactive_settings
            .as_ref()
            .map(|interactive| interactive.history_buffer)
            .unwrap_or(DIRECT_RUNTIME_HISTORY_BUFFER)
    })
}

fn spawn_interactive_pty_runtime(command_spec: PreparedCommand, history_buffer: usize) -> io::Result<InteractivePtyRuntime> {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let rows = rows.max(1);
    let cols = cols.max(1);

    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| io::Error::other(err.to_string()))?;

    let cmd = command_builder_from_spec(&command_spec)?;
    let child = Arc::new(Mutex::new(pty_pair.slave.spawn_command(cmd).map_err(|err| io::Error::other(err.to_string()))?));
    drop(pty_pair.slave);

    let reader = pty_pair.master.try_clone_reader().map_err(|err| io::Error::other(err.to_string()))?;
    let writer = pty_pair.master.take_writer().map_err(|err| io::Error::other(err.to_string()))?;
    let writer: TerminalInputWriter = Arc::new(Mutex::new(writer));
    let pty_master = Arc::new(Mutex::new(pty_pair.master));
    let engine = Arc::new(Mutex::new(TerminalEngine::new_with_input_writer_and_host(
        rows,
        cols,
        history_buffer,
        writer.clone(),
        terminal_host_callbacks(),
    )));
    let exited = Arc::new(Mutex::new(false));
    let render_epoch = Arc::new(AtomicU64::new(0));
    let session = TerminalSession::new(
        Some(pty_master),
        Some(writer),
        TerminalChild::Pty(child.clone()),
        engine,
        exited.clone(),
        render_epoch,
    );

    let (event_tx, event_rx) = mpsc::sync_channel(PTY_EVENT_QUEUE_CAPACITY);
    spawn_pty_output_reader(
        reader_thread_name(&command_spec.program),
        reader,
        {
            let output_tx = event_tx.clone();
            move |bytes| output_tx.send(PtyRuntimeEvent::Output(bytes.to_vec())).is_ok()
        },
        {
            let closed_tx = event_tx.clone();
            move || {
                let _ = closed_tx.send(PtyRuntimeEvent::ReaderClosed);
            }
        },
        PtyLogTarget::global_ssh(),
    )?;
    spawn_exit_watcher(child, exited, event_tx)?;

    Ok(InteractivePtyRuntime {
        session,
        event_rx,
        highlight_overlay: HighlightOverlayEngine::new(),
        host_scrollback: HostScrollbackMirror::new(history_buffer),
        reload_notice_toast: None,
    })
}

fn reader_thread_name(program: &str) -> String {
    format!("pty-reader-{}", program)
}

fn command_builder_from_spec(command_spec: &PreparedCommand) -> io::Result<CommandBuilder> {
    let program_path = command_path::resolve_known_command_path(&command_spec.program)?;
    let mut builder = CommandBuilder::new(program_path.as_os_str());
    for arg in &command_spec.args {
        builder.arg(arg);
    }
    for (key, value) in &command_spec.env {
        builder.env(key, value);
    }
    Ok(builder)
}

fn spawn_exit_watcher(
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    exited: Arc<Mutex<bool>>,
    event_tx: SyncSender<PtyRuntimeEvent>,
) -> io::Result<()> {
    thread::Builder::new().name("pty-exit-watcher".to_string()).spawn(move || {
        let exit_result = match child.lock() {
            Ok(mut child) => child.wait().map_err(|err| io::Error::other(err.to_string())),
            Err(err) => Err(io::Error::other(err.to_string())),
        };

        if let Ok(mut exited) = exited.lock() {
            *exited = true;
        }
        let _ = event_tx.send(PtyRuntimeEvent::Exited(exit_result));
    })?;
    Ok(())
}

fn run_pty_event_loop(runtime: &mut InteractivePtyRuntime) -> Result<std::process::ExitCode> {
    let mut mode_guard = TerminalModeGuard::enter()?;
    let stdout = io::stdout();
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(height.max(1)),
        },
    )?;
    runtime.session.resize(height.max(1), width.max(1));

    let mut scroll_offset = 0usize;
    let mut last_drawn_epoch = u64::MAX;
    let mut last_draw_at = Instant::now();
    let mut last_config_version = u64::MAX;
    let mut exit_status = None;
    let mut reader_closed = false;
    let mut force_redraw = true;
    let mut viewport_area = Rect::new(0, 0, width.max(1), height.max(1));

    loop {
        if expire_reload_notice_toast(&mut runtime.reload_notice_toast) {
            force_redraw = true;
        }

        drain_pty_runtime_events(runtime, &mut exit_status, &mut reader_closed)?;

        if let Some(reload_notice_toast) = take_latest_reload_notice_toast() {
            runtime.reload_notice_toast = Some(reload_notice_toast);
            force_redraw = true;
        }

        let (mouse_mode, _) = current_mouse_protocol(&runtime.session)?;
        mode_guard.sync_mouse_capture(mouse_mode != MouseProtocolMode::None)?;

        let current_epoch = runtime.session.render_epoch();
        let current_config_version = config::current_config_version();
        if force_redraw || current_epoch != last_drawn_epoch || current_config_version != last_config_version || last_draw_at.elapsed() >= RENDER_HEARTBEAT {
            let captured_scrollback = {
                let engine = runtime.session.engine().lock().map_err(|err| io::Error::other(err.to_string()))?;
                capture_host_scrollback_insertions(&engine, &mut runtime.host_scrollback)
            };
            let scrollback_insertions = build_host_scrollback_insertions(captured_scrollback, &mut runtime.highlight_overlay, current_epoch);

            for insertion in scrollback_insertions {
                let insertion_height = insertion.viewport.size().0;
                terminal.insert_before(insertion_height, |buffer| {
                    let _ = paint_terminal_view(buffer, buffer.area, &insertion.viewport, &insertion.overlay, false);
                })?;
            }

            let mut render_error = None;
            let mut drawn_area = viewport_area;
            terminal.draw(|frame| {
                drawn_area = frame.area();
                if let Err(err) = render_terminal_frame(
                    frame,
                    &runtime.session,
                    &mut runtime.highlight_overlay,
                    scroll_offset,
                    runtime.reload_notice_toast.as_ref().map(ReloadNoticeToast::message),
                ) {
                    render_error = Some(err);
                }
            })?;
            if let Some(err) = render_error {
                return Err(err.into());
            }
            viewport_area = drawn_area;
            last_drawn_epoch = runtime.session.render_epoch();
            last_draw_at = Instant::now();
            last_config_version = current_config_version;
            force_redraw = false;
        }

        if exit_status.is_some() && reader_closed {
            break;
        }

        if !event::poll(EVENT_POLL_INTERVAL)? {
            continue;
        }

        match event::read()? {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if key.code == KeyCode::PageUp && key.modifiers.contains(KeyModifiers::SHIFT) {
                    scroll_offset = scroll_offset.saturating_add(10).min(max_scrollback(&runtime.session));
                    force_redraw = true;
                    continue;
                }

                if key.code == KeyCode::PageDown && key.modifiers.contains(KeyModifiers::SHIFT) {
                    scroll_offset = scroll_offset.saturating_sub(10);
                    force_redraw = true;
                    continue;
                }

                if scroll_offset > 0 {
                    scroll_offset = 0;
                    force_redraw = true;
                }

                if let Some(bytes) = encode_key_event_bytes(key) {
                    runtime.session.write_input(&bytes)?;
                }
            }
            Event::Paste(text) => {
                if text.is_empty() {
                    continue;
                }

                if scroll_offset > 0 {
                    scroll_offset = 0;
                    force_redraw = true;
                }

                let bracketed = bracketed_paste_enabled(&runtime.session)?;
                let bytes = encode_paste_bytes(&text, bracketed);
                runtime.session.write_input(&bytes)?;
            }
            Event::Resize(width, height) => {
                runtime.session.resize(height.max(1), width.max(1));
                runtime.host_scrollback.invalidate();
                force_redraw = true;
            }
            Event::Mouse(mouse) => {
                let (mouse_mode, mouse_encoding) = current_mouse_protocol(&runtime.session)?;
                if mouse_mode == MouseProtocolMode::None {
                    continue;
                }

                if scroll_offset > 0 {
                    scroll_offset = 0;
                    force_redraw = true;
                }

                if let Some(bytes) = encode_mouse_event(mouse, viewport_area, mouse_mode, mouse_encoding) {
                    runtime.session.write_input(&bytes)?;
                }
            }
            _ => {}
        }
    }

    let show_cursor_result = terminal.show_cursor();
    mode_guard.cleanup();
    let restore_prompt_result = restore_host_terminal_prompt_line(viewport_area);
    if let Err(err) = show_cursor_result {
        return Err(err.into());
    }
    if let Err(err) = restore_prompt_result {
        return Err(err.into());
    }
    if let Err(err) = log::LOGGER.flush_ssh() {
        log_error!("Failed to flush session logs: {}", err);
    }

    match exit_status {
        Some(Ok(status)) => {
            let code = i32::try_from(status.exit_code()).ok();
            Ok(map_exit_code(status.success(), code))
        }
        Some(Err(err)) => Err(err.into()),
        None => Ok(std::process::ExitCode::from(1)),
    }
}

fn drain_pty_runtime_events(
    runtime: &InteractivePtyRuntime,
    exit_status: &mut Option<io::Result<portable_pty::ExitStatus>>,
    reader_closed: &mut bool,
) -> io::Result<()> {
    loop {
        match runtime.event_rx.try_recv() {
            Ok(PtyRuntimeEvent::Output(bytes)) => process_pty_output(&runtime.session, &bytes)?,
            Ok(PtyRuntimeEvent::ReaderClosed) => *reader_closed = true,
            Ok(PtyRuntimeEvent::Exited(status)) => *exit_status = Some(status),
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                *reader_closed = true;
                return Ok(());
            }
        }
    }
}

fn process_pty_output(session: &TerminalSession, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }

    let mut engine = session.engine().lock().map_err(|err| io::Error::other(err.to_string()))?;
    engine.process_output(bytes);
    drop(engine);
    session.bump_render_epoch();
    Ok(())
}

fn take_latest_reload_notice_toast() -> Option<ReloadNoticeToast> {
    config::take_reload_notices()
        .into_iter()
        .last()
        .map(|notice| ReloadNoticeToast::new(format_reload_notice(&notice)))
}

fn expire_reload_notice_toast(reload_notice_toast: &mut Option<ReloadNoticeToast>) -> bool {
    let should_clear = reload_notice_toast.as_ref().is_some_and(ReloadNoticeToast::expired);
    if should_clear {
        *reload_notice_toast = None;
    }
    should_clear
}

#[cfg_attr(not(test), allow(dead_code))]
fn collect_host_scrollback_insertions(
    engine: &TerminalEngine,
    highlight_overlay: &mut HighlightOverlayEngine,
    render_epoch: u64,
    host_scrollback: &mut HostScrollbackMirror,
) -> Vec<ScrollbackInsertion> {
    let captured = capture_host_scrollback_insertions(engine, host_scrollback);
    build_host_scrollback_insertions(captured, highlight_overlay, render_epoch)
}

fn capture_host_scrollback_insertions(engine: &TerminalEngine, host_scrollback: &mut HostScrollbackMirror) -> CapturedScrollbackInsertions {
    let (snapshot, buffer_row_ids) = {
        let view = engine.view_model();
        let (rows, cols) = view.size();
        (view.frontend_snapshot_at_scrollback(rows, cols, 0), view.buffer_row_storage_ids())
    };
    let (viewport_rows, viewport_cols) = snapshot.viewport().size();
    let current_history_size = snapshot.scrollback().max_offset();

    if snapshot.is_alternate_screen() || viewport_rows == 0 || viewport_cols == 0 {
        host_scrollback.sync_history(current_history_size);
        host_scrollback.invalidate();
        return CapturedScrollbackInsertions::empty();
    }

    if !host_scrollback.valid || host_scrollback.last_viewport_rows != viewport_rows || host_scrollback.last_viewport_cols != viewport_cols {
        host_scrollback.sync(current_history_size, viewport_rows, viewport_cols, buffer_row_ids);
        return CapturedScrollbackInsertions::empty();
    }

    let mut scrolled_line_count = current_history_size.saturating_sub(host_scrollback.last_history_size);
    if scrolled_line_count == 0
        && current_history_size == host_scrollback.history_capacity
        && host_scrollback.last_history_size == host_scrollback.history_capacity
    {
        scrolled_line_count = infer_scrolled_line_count(&host_scrollback.last_buffer_row_ids, &buffer_row_ids);
    }

    let mut pending = Vec::new();
    if scrolled_line_count > 0 {
        let view = engine.view_model();
        let mut remaining = scrolled_line_count;
        while remaining > 0 {
            let chunk_rows = remaining.min(viewport_rows as usize) as u16;
            let snapshot = view.frontend_snapshot_at_scrollback(chunk_rows, viewport_cols, remaining);
            pending.push(PendingScrollbackInsertion { snapshot });
            remaining = remaining.saturating_sub(chunk_rows as usize);
        }
    }

    host_scrollback.sync(current_history_size, viewport_rows, viewport_cols, buffer_row_ids);
    CapturedScrollbackInsertions { pending }
}

fn build_host_scrollback_insertions(
    captured: CapturedScrollbackInsertions,
    highlight_overlay: &mut HighlightOverlayEngine,
    render_epoch: u64,
) -> Vec<ScrollbackInsertion> {
    captured
        .pending
        .into_iter()
        .map(|pending| {
            let overlay = pending.snapshot.build_highlight_overlay(highlight_overlay, render_epoch);
            ScrollbackInsertion {
                viewport: pending.snapshot.into_viewport(),
                overlay,
            }
        })
        .collect()
}

fn infer_scrolled_line_count<T: Eq>(previous_rows: &[T], current_rows: &[T]) -> usize {
    if previous_rows.is_empty() || current_rows.is_empty() || previous_rows.len() != current_rows.len() {
        return 0;
    }

    let max_overlap = previous_rows.len().min(current_rows.len());
    for overlap in (1..=max_overlap).rev() {
        if previous_rows[previous_rows.len() - overlap..] == current_rows[..overlap] {
            return previous_rows.len().saturating_sub(overlap);
        }
    }

    0
}

fn max_scrollback(session: &TerminalSession) -> usize {
    match session.engine().lock() {
        Ok(engine) => engine.max_scrollback(),
        Err(_) => 0,
    }
}

fn bracketed_paste_enabled(session: &TerminalSession) -> io::Result<bool> {
    let engine = session.engine().lock().map_err(|err| io::Error::other(err.to_string()))?;
    Ok(engine.view_model().bracketed_paste_enabled())
}

fn current_mouse_protocol(session: &TerminalSession) -> io::Result<(MouseProtocolMode, MouseProtocolEncoding)> {
    let engine = session.engine().lock().map_err(|err| io::Error::other(err.to_string()))?;
    Ok(engine.view_model().mouse_protocol())
}

fn restore_host_terminal_prompt_line(viewport_area: Rect) -> io::Result<()> {
    if viewport_area.height == 0 {
        return Ok(());
    }

    let mut stdout = io::stdout();
    let bottom_row = viewport_area.y.saturating_add(viewport_area.height.saturating_sub(1));
    execute!(stdout, MoveTo(0, bottom_row), Print("\x1b[0m\r\n"))?;
    Ok(())
}

fn render_terminal_frame(
    frame: &mut Frame,
    session: &TerminalSession,
    highlight_overlay: &mut HighlightOverlayEngine,
    scroll_offset: usize,
    reload_notice_toast: Option<&str>,
) -> io::Result<()> {
    let area = frame.area();
    let render_snapshot = session.snapshot_for_frontend(area.height, area.width, scroll_offset)?;
    let effective_scroll_offset = render_snapshot.scrollback().display_offset();
    let overlay = render_snapshot.build_highlight_overlay(highlight_overlay);

    let _cursor = paint_terminal_view(frame.buffer_mut(), area, render_snapshot.viewport(), &overlay, effective_scroll_offset == 0);
    if let Some(reload_notice_toast) = reload_notice_toast {
        render_reload_notice_toast(frame, area, reload_notice_toast);
    }
    Ok(())
}

fn paint_terminal_view(
    buffer: &mut ratatui::buffer::Buffer,
    area: Rect,
    viewport: &TerminalViewport,
    highlight_overlay: &HighlightOverlay,
    show_cursor: bool,
) -> Option<Position> {
    let overlay_styles = highlight_overlay.styles();
    let mut active_row = None;
    let mut active_row_ranges = None;

    paint_terminal_viewport(buffer, area, viewport, show_cursor, |absolute_row, col, _cell, is_cursor, base_style| {
        if active_row != Some(absolute_row) {
            active_row = Some(absolute_row);
            active_row_ranges = highlight_overlay.ranges_for_row(absolute_row);
        }

        let syntax_style = apply_overlay_ranges(base_style, active_row_ranges, overlay_styles, col);

        if is_cursor {
            syntax_style.bg(ratatui::style::Color::White).fg(ratatui::style::Color::Black)
        } else {
            syntax_style
        }
    })
}

fn mouse_to_vt_coords(area: Rect, mouse: MouseEvent) -> Option<(u16, u16)> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    if mouse.column < area.x || mouse.column >= area.x + area.width || mouse.row < area.y || mouse.row >= area.y + area.height {
        return None;
    }

    Some(((mouse.column - area.x) + 1, (mouse.row - area.y) + 1))
}

fn encode_mouse_event(mouse: MouseEvent, area: Rect, mode: MouseProtocolMode, encoding: MouseProtocolEncoding) -> Option<Vec<u8>> {
    let (col, row) = mouse_to_vt_coords(area, mouse)?;

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => Some(encode_mouse_event_bytes(encoding, 0, col, row, false)),
        MouseEventKind::Down(MouseButton::Middle) => Some(encode_mouse_event_bytes(encoding, 1, col, row, false)),
        MouseEventKind::Down(MouseButton::Right) => Some(encode_mouse_event_bytes(encoding, 2, col, row, false)),
        MouseEventKind::Up(MouseButton::Left) if mode != MouseProtocolMode::Press => Some(encode_mouse_event_bytes(encoding, 0, col, row, true)),
        MouseEventKind::Up(MouseButton::Middle) if mode != MouseProtocolMode::Press => Some(encode_mouse_event_bytes(encoding, 1, col, row, true)),
        MouseEventKind::Up(MouseButton::Right) if mode != MouseProtocolMode::Press => Some(encode_mouse_event_bytes(encoding, 2, col, row, true)),
        MouseEventKind::Drag(MouseButton::Left) if matches!(mode, MouseProtocolMode::AnyMotion | MouseProtocolMode::ButtonMotion) => {
            Some(encode_mouse_event_bytes(encoding, 32, col, row, false))
        }
        MouseEventKind::ScrollUp => Some(encode_mouse_event_bytes(encoding, 64, col, row, false)),
        MouseEventKind::ScrollDown => Some(encode_mouse_event_bytes(encoding, 65, col, row, false)),
        MouseEventKind::Moved if mode == MouseProtocolMode::AnyMotion => Some(encode_mouse_event_bytes(encoding, 35, col, row, false)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../test/process_pty_runtime.rs"]
mod tests;
