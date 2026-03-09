//! PTY-centered interactive SSH runtime.
//!
//! This is the preferred direct `cossh ssh` architecture. It launches SSH in a
//! PTY, feeds raw PTY bytes into `alacritty_terminal`, and renders terminal
//! state from the canonical engine instead of rewriting stdout chunks.

use super::command_spec::PreparedCommand;
use super::exit::map_exit_code;
use crate::auth::secret::ExposeSecret;
use crate::terminal_core::{TerminalChild, TerminalEngine, TerminalInputWriter, TerminalSession, TerminalViewport};
use crate::terminal_ratatui::paint_terminal_viewport;
use crate::{Result, command_path, config, log, log_debug, log_error};
use crossterm::{
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ratatui::{
    Frame, Terminal,
    layout::{Position, Rect},
    style::{Modifier, Style},
};
use std::io::{self, IsTerminal, Read};
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
    LegacyStream,
}

enum PtyRuntimeEvent {
    Output(Vec<u8>),
    ReaderClosed,
    Exited(io::Result<portable_pty::ExitStatus>),
}

struct InteractivePtyRuntime {
    session: TerminalSession,
    event_rx: Receiver<PtyRuntimeEvent>,
}

#[derive(Debug, Default)]
struct TerminalModeGuard {
    active: bool,
}

impl TerminalModeGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
        Ok(Self { active: true })
    }

    fn cleanup(&mut self) {
        if !self.active {
            return;
        }

        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, DisableBracketedPaste);
        self.active = false;
    }
}

impl Drop for TerminalModeGuard {
    fn drop(&mut self) {
        self.cleanup();
    }
}

pub(super) fn prefer_pty_centered_ssh_runtime() -> bool {
    let is_embedded = std::env::var_os(super::EMBEDDED_INTERACTIVE_SSH_ENV).is_some();
    let force_legacy = std::env::var_os(super::LEGACY_STREAM_INTERACTIVE_SSH_ENV).is_some();
    let has_interactive_tty = io::stdin().is_terminal() && io::stdout().is_terminal();

    matches!(
        select_interactive_ssh_runtime(is_embedded, force_legacy, has_interactive_tty),
        InteractiveSshRuntime::PtyCentered
    )
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

fn select_interactive_ssh_runtime(is_embedded: bool, force_legacy: bool, has_interactive_tty: bool) -> InteractiveSshRuntime {
    if force_legacy || is_embedded || !has_interactive_tty {
        InteractiveSshRuntime::LegacyStream
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
    let engine = Arc::new(Mutex::new(TerminalEngine::new_with_input_writer(rows, cols, history_buffer, writer.clone())));
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
    spawn_pty_reader(reader_thread_name(&command_spec.program), reader, event_tx.clone())?;
    spawn_exit_watcher(child, exited, event_tx)?;

    Ok(InteractivePtyRuntime { session, event_rx })
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

fn spawn_pty_reader(name: String, mut reader: Box<dyn Read + Send>, event_tx: SyncSender<PtyRuntimeEvent>) -> io::Result<()> {
    thread::Builder::new().name(name).spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    let _ = event_tx.send(PtyRuntimeEvent::ReaderClosed);
                    break;
                }
                Ok(bytes_read) => {
                    if event_tx.send(PtyRuntimeEvent::Output(buf[..bytes_read].to_vec())).is_err() {
                        break;
                    }
                }
                Err(err) => {
                    log_error!("Error reading from PTY: {}", err);
                    let _ = event_tx.send(PtyRuntimeEvent::ReaderClosed);
                    break;
                }
            }
        }
        log_debug!("PTY reader thread exiting");
    })?;
    Ok(())
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
    let mut terminal = Terminal::new(backend)?;
    let terminal_size = terminal.size()?;
    runtime.session.resize(terminal_size.height.max(1), terminal_size.width.max(1));

    let mut scroll_offset = 0usize;
    let mut last_drawn_epoch = u64::MAX;
    let mut last_draw_at = Instant::now();
    let mut exit_status = None;
    let mut reader_closed = false;
    let mut force_redraw = true;

    loop {
        drain_pty_runtime_events(runtime, &mut exit_status, &mut reader_closed)?;

        let current_epoch = runtime.session.render_epoch();
        if force_redraw || current_epoch != last_drawn_epoch || last_draw_at.elapsed() >= RENDER_HEARTBEAT {
            let mut render_error = None;
            terminal.draw(|frame| {
                if let Err(err) = render_terminal_frame(frame, &runtime.session, scroll_offset) {
                    render_error = Some(err);
                }
            })?;
            if let Some(err) = render_error {
                return Err(err.into());
            }
            last_drawn_epoch = runtime.session.render_epoch();
            last_draw_at = Instant::now();
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
                force_redraw = true;
            }
            _ => {}
        }
    }

    let show_cursor_result = terminal.show_cursor();
    mode_guard.cleanup();
    if let Err(err) = show_cursor_result {
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

    if log::LOGGER.is_ssh_logging_enabled() {
        let chunk = String::from_utf8_lossy(bytes);
        if let Err(err) = log::LOGGER.log_ssh_raw(chunk.as_ref()) {
            log_error!("Failed to write session log data: {}", err);
        }
    }

    let mut engine = session.engine().lock().map_err(|err| io::Error::other(err.to_string()))?;
    engine.process_output(bytes);
    drop(engine);
    session.bump_render_epoch();
    Ok(())
}

fn max_scrollback(session: &TerminalSession) -> usize {
    match session.engine().lock() {
        Ok(engine) => engine.max_scrollback(),
        Err(_) => 0,
    }
}

fn bracketed_paste_enabled(session: &TerminalSession) -> io::Result<bool> {
    let engine = session.engine().lock().map_err(|err| io::Error::other(err.to_string()))?;
    Ok(engine.screen().bracketed_paste_enabled())
}

fn render_terminal_frame(frame: &mut Frame, session: &TerminalSession, scroll_offset: usize) -> io::Result<()> {
    let area = frame.area();
    let mut engine = session.engine().lock().map_err(|err| io::Error::other(err.to_string()))?;
    engine.set_display_scrollback(scroll_offset);
    let viewport = engine.view_model().viewport_snapshot(area.height, area.width);
    drop(engine);

    let cursor = paint_terminal_view(frame.buffer_mut(), area, &viewport, scroll_offset == 0);
    if let Some(cursor) = cursor {
        frame.set_cursor_position(cursor);
    }
    Ok(())
}

fn paint_terminal_view(buffer: &mut ratatui::buffer::Buffer, area: Rect, viewport: &TerminalViewport, show_cursor: bool) -> Option<Position> {
    paint_terminal_viewport(buffer, area, viewport, show_cursor, |_absolute_row, _col, cell, is_cursor, base_style| {
        if is_cursor {
            let mut style = Style::default().bg(ratatui::style::Color::White).fg(ratatui::style::Color::Black);
            if cell.bold() {
                style = style.add_modifier(Modifier::BOLD);
            }
            style
        } else {
            base_style
        }
    })
}

fn encode_paste_bytes(pasted: &str, bracketed: bool) -> Vec<u8> {
    if !bracketed {
        return pasted.as_bytes().to_vec();
    }

    let mut out = Vec::with_capacity(pasted.len() + 12);
    out.extend_from_slice(b"\x1b[200~");
    out.extend_from_slice(pasted.as_bytes());
    out.extend_from_slice(b"\x1b[201~");
    out
}

fn modifier_parameter(modifiers: KeyModifiers) -> u8 {
    let mut param = 1u8;
    if modifiers.contains(KeyModifiers::SHIFT) {
        param = param.saturating_add(1);
    }
    if modifiers.contains(KeyModifiers::ALT) {
        param = param.saturating_add(2);
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        param = param.saturating_add(4);
    }
    param
}

fn prefix_with_escape(mut bytes: Vec<u8>) -> Vec<u8> {
    let mut prefixed = Vec::with_capacity(bytes.len() + 1);
    prefixed.push(0x1b);
    prefixed.append(&mut bytes);
    prefixed
}

fn encode_csi_cursor_key(final_byte: u8, modifiers: KeyModifiers) -> Vec<u8> {
    let base = vec![0x1b, b'[', final_byte];
    if modifiers.is_empty() {
        return base;
    }
    if modifiers == KeyModifiers::ALT {
        return prefix_with_escape(base);
    }

    let final_char = final_byte as char;
    format!("\x1b[1;{}{}", modifier_parameter(modifiers), final_char).into_bytes()
}

fn encode_csi_tilde_key(code: u8, modifiers: KeyModifiers) -> Vec<u8> {
    if modifiers.is_empty() {
        return format!("\x1b[{}~", code).into_bytes();
    }
    if modifiers == KeyModifiers::ALT {
        return prefix_with_escape(format!("\x1b[{}~", code).into_bytes());
    }

    format!("\x1b[{};{}~", code, modifier_parameter(modifiers)).into_bytes()
}

fn encode_key_event_bytes(key: KeyEvent) -> Option<Vec<u8>> {
    let modifiers = key.modifiers & (KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL);

    let bytes = match key.code {
        KeyCode::Char(ch) => {
            let mut out = if modifiers.contains(KeyModifiers::CONTROL) {
                let control_byte = match ch {
                    '@' | ' ' => 0,
                    'a'..='z' => (ch as u8) - b'a' + 1,
                    'A'..='Z' => (ch as u8) - b'A' + 1,
                    '[' => 27,
                    '\\' => 28,
                    ']' => 29,
                    '^' => 30,
                    '_' => 31,
                    '?' => 127,
                    _ => ch as u8,
                };
                vec![control_byte]
            } else {
                ch.to_string().into_bytes()
            };

            if modifiers.contains(KeyModifiers::ALT) {
                out = prefix_with_escape(out);
            }
            out
        }
        KeyCode::Enter => {
            let out = vec![b'\r'];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Backspace => {
            let out = vec![127];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Tab => {
            let out = vec![b'\t'];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Esc => {
            let out = vec![27];
            if modifiers.contains(KeyModifiers::ALT) {
                prefix_with_escape(out)
            } else {
                out
            }
        }
        KeyCode::Up => encode_csi_cursor_key(b'A', modifiers),
        KeyCode::Down => encode_csi_cursor_key(b'B', modifiers),
        KeyCode::Right => encode_csi_cursor_key(b'C', modifiers),
        KeyCode::Left => encode_csi_cursor_key(b'D', modifiers),
        KeyCode::Home => encode_csi_cursor_key(b'H', modifiers),
        KeyCode::End => encode_csi_cursor_key(b'F', modifiers),
        KeyCode::PageUp => encode_csi_tilde_key(5, modifiers),
        KeyCode::PageDown => encode_csi_tilde_key(6, modifiers),
        KeyCode::Delete => encode_csi_tilde_key(3, modifiers),
        KeyCode::Insert => encode_csi_tilde_key(2, modifiers),
        _ => return None,
    };

    Some(bytes)
}

#[cfg(test)]
#[path = "../test/process_pty_runtime.rs"]
mod tests;
