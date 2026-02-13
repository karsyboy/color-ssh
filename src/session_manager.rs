//! Interactive TUI-based SSH host selector with native terminal sessions
//!
//! Sessions run natively in the terminal for full scrollback and copy/paste
//! support. Press Shift+Tab to detach from a session and return to the manager.
//! Multiple sessions can run in the background simultaneously.

use crate::ssh_config::{SshHost, load_ssh_hosts};
use crate::{log_debug, log_error};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{io, time::Duration};

/// Maximum size of per-session output buffer (256KB)
const SESSION_BUFFER_SIZE: usize = 256 * 1024;

/// Represents a background SSH session running in a PTY
pub struct SshSession {
    /// The SSH host this session connects to
    host: SshHost,
    /// PTY master for resizing the terminal
    pty_master: Box<dyn MasterPty + Send>,
    /// PTY writer for sending input to the session
    writer: Box<dyn Write + Send>,
    /// Child process handle
    _child: Box<dyn Child + Send>,
    /// Whether the process has exited
    exited: Arc<AtomicBool>,
    /// Whether the session is currently attached (controls output forwarding to stdout)
    attached: Arc<AtomicBool>,
    /// Per-session output buffer for separate scrollback
    output_buffer: Arc<Mutex<Vec<u8>>>,
}

/// Fuzzy match scoring for host search
fn fuzzy_match(text: &str, pattern: &str) -> Option<i32> {
    let text = text.to_lowercase();
    let pattern = pattern.to_lowercase();

    let mut text_chars = text.chars().peekable();
    let mut pattern_chars = pattern.chars().peekable();
    let mut score = 0;
    let mut consecutive = 0;

    while let Some(&pattern_char) = pattern_chars.peek() {
        let mut found = false;

        while let Some(&text_char) = text_chars.peek() {
            text_chars.next();
            if text_char == pattern_char {
                score += 1 + consecutive;
                consecutive += 1;
                pattern_chars.next();
                found = true;
                break;
            } else {
                consecutive = 0;
            }
        }

        if !found {
            return None;
        }
    }

    Some(score)
}

/// Convert a crossterm key event to raw terminal escape bytes
fn key_event_to_bytes(key: &KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if c.is_ascii_alphabetic() {
                    vec![(c.to_ascii_lowercase() as u8) - b'a' + 1]
                } else {
                    vec![c as u8]
                }
            } else {
                c.to_string().into_bytes()
            }
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![127],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![27],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        _ => vec![],
    }
}

/// Main application state
pub struct App {
    /// List of all SSH hosts
    hosts: Vec<SshHost>,
    /// Filtered hosts based on search (with scores for fuzzy matching)
    filtered_hosts: Vec<(usize, i32)>,
    /// Currently selected host index (in filtered list)
    selected_host: usize,
    /// List state for the host list
    host_list_state: ListState,
    /// Search query
    search_query: String,
    /// Whether we're in search mode
    search_mode: bool,
    /// Whether to exit the app
    should_exit: bool,
    /// Cached area for the host list
    host_list_area: Rect,
    /// Host list scroll offset
    host_scroll_offset: usize,
    /// Width of the host panel in columns (adjustable with Ctrl+Left/Right)
    host_panel_width: u16,
    /// Background SSH sessions
    sessions: Vec<SshSession>,
    /// Currently selected session index
    selected_session: usize,
    /// List state for the session list
    session_list_state: ListState,
    /// Whether focus is on host list (true) or session list (false)
    focus_on_hosts: bool,
    /// Index of session to attach to (set by key handler, executed by main loop)
    attach_request: Option<usize>,
}

impl App {
    /// Create a new App instance
    pub fn new() -> io::Result<Self> {
        log_debug!("Initializing session manager");

        let hosts = load_ssh_hosts().unwrap_or_else(|e| {
            log_error!("Failed to load SSH hosts: {}", e);
            Vec::new()
        });

        log_debug!("Loaded {} SSH hosts", hosts.len());

        let filtered_hosts: Vec<(usize, i32)> = (0..hosts.len()).map(|i| (i, 0)).collect();

        let mut host_list_state = ListState::default();
        if !filtered_hosts.is_empty() {
            host_list_state.select(Some(0));
        }

        Ok(Self {
            hosts,
            filtered_hosts,
            selected_host: 0,
            host_list_state,
            host_list_area: Rect::default(),
            host_scroll_offset: 0,
            host_panel_width: 40,
            search_query: String::new(),
            search_mode: false,
            should_exit: false,
            sessions: Vec::new(),
            selected_session: 0,
            session_list_state: ListState::default(),
            focus_on_hosts: true,
            attach_request: None,
        })
    }

    /// Update the filtered hosts based on search query with fuzzy matching
    fn update_filtered_hosts(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_hosts = (0..self.hosts.len()).map(|i| (i, 0)).collect();
        } else {
            let query = &self.search_query;
            let mut matches: Vec<(usize, i32)> = self
                .hosts
                .iter()
                .enumerate()
                .filter_map(|(idx, host)| {
                    let mut best_score = None;

                    if let Some(score) = fuzzy_match(&host.name, query) {
                        best_score = Some(score + 100);
                    }

                    if let Some(hostname) = &host.hostname {
                        if let Some(score) = fuzzy_match(hostname, query) {
                            best_score = Some(best_score.unwrap_or(0).max(score + 50));
                        }
                    }

                    if let Some(user) = &host.user {
                        if let Some(score) = fuzzy_match(user, query) {
                            best_score = Some(best_score.unwrap_or(0).max(score + 30));
                        }
                    }

                    best_score.map(|score| (idx, score))
                })
                .collect();

            matches.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered_hosts = matches;
        }

        self.selected_host = 0;
        if !self.filtered_hosts.is_empty() {
            self.host_list_state.select(Some(0));
        } else {
            self.host_list_state.select(None);
        }
        self.host_scroll_offset = 0;
    }

    /// Start a new session for the selected host and request attach
    fn start_session(&mut self) {
        if self.filtered_hosts.is_empty() {
            return;
        }

        let host_idx = self.filtered_hosts[self.selected_host].0;
        let host = self.hosts[host_idx].clone();

        log_debug!("Starting session for host: {}", host.name);

        // If a running session already exists for this host, just attach to it
        if let Some(idx) = self
            .sessions
            .iter()
            .position(|s| s.host.name == host.name && !s.exited.load(Ordering::SeqCst))
        {
            log_debug!("Running session exists, attaching");
            self.selected_session = idx;
            self.session_list_state.select(Some(idx));
            self.attach_request = Some(idx);
            return;
        }

        // Remove any exited sessions for this host
        self.sessions
            .retain(|s| s.host.name != host.name || !s.exited.load(Ordering::SeqCst));
        if self.selected_session >= self.sessions.len() {
            self.selected_session = self.sessions.len().saturating_sub(1);
        }

        // Spawn new SSH session
        match Self::spawn_ssh_session(&host) {
            Ok(session) => {
                self.sessions.push(session);
                let idx = self.sessions.len() - 1;
                self.selected_session = idx;
                self.session_list_state.select(Some(idx));
                self.attach_request = Some(idx);
                log_debug!("Created session at index {}", idx);
            }
            Err(e) => {
                log_error!("Failed to spawn SSH session: {}", e);
            }
        }
    }

    /// Spawn an SSH session in a PTY with a background reader thread
    fn spawn_ssh_session(host: &SshHost) -> io::Result<SshSession> {
        let pty_system = native_pty_system();

        let pty_pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        let cossh_path =
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("cossh"));

        let mut cmd = CommandBuilder::new(cossh_path);
        cmd.arg(&host.name);

        log_debug!("Spawning cossh session: cossh {}", host.name);

        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        let mut reader = pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        let writer = pty_pair
            .master
            .take_writer()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        let exited = Arc::new(AtomicBool::new(false));
        let attached = Arc::new(AtomicBool::new(false));
        let output_buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let exited_clone = exited.clone();
        let attached_clone = attached.clone();
        let buffer_clone = output_buffer.clone();

        // Background reader thread: always drains PTY output.
        // Always captures output to per-session buffer.
        // When attached, also forwards output directly to stdout.
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut stdout = io::stdout();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        exited_clone.store(true, Ordering::SeqCst);
                        break;
                    }
                    Ok(n) => {
                        // Always capture to session buffer
                        if let Ok(mut buffer) = buffer_clone.lock() {
                            buffer.extend_from_slice(&buf[..n]);
                            // Trim buffer if it exceeds max size
                            if buffer.len() > SESSION_BUFFER_SIZE {
                                let excess = buffer.len() - SESSION_BUFFER_SIZE;
                                buffer.drain(..excess);
                            }
                        }
                        // Forward to stdout when attached
                        if attached_clone.load(Ordering::Relaxed) {
                            let _ = stdout.write_all(&buf[..n]);
                            let _ = stdout.flush();
                        }
                    }
                    Err(e) => {
                        log_error!("Error reading from PTY: {}", e);
                        exited_clone.store(true, Ordering::SeqCst);
                        break;
                    }
                }
            }
            log_debug!("PTY reader thread exiting");
        });

        Ok(SshSession {
            host: host.clone(),
            pty_master: pty_pair.master,
            writer,
            _child: child,
            exited,
            attached,
            output_buffer,
        })
    }

    /// Attach to a session: leave TUI, proxy I/O natively, return on detach or exit
    fn attach_to_session(&mut self, idx: usize) -> io::Result<()> {
        if idx >= self.sessions.len() {
            return Ok(());
        }

        let session = &mut self.sessions[idx];
        if session.exited.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Resize PTY to match current terminal size
        if let Ok((cols, rows)) = crossterm::terminal::size() {
            let _ = session.pty_master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }

        // Mark as attached — background reader thread will start forwarding to stdout
        session.attached.store(true, Ordering::SeqCst);

        let mut stdout = io::stdout();
        // Clear screen and replay this session's output buffer for separate scrollback
        write!(stdout, "\x1b[2J\x1b[H")?;
        execute!(stdout, cursor::Show)?;
        if let Ok(buffer) = session.output_buffer.lock() {
            if !buffer.is_empty() {
                let _ = stdout.write_all(&buffer);
            }
        }
        stdout.flush()?;

        // Proxy loop: read keyboard events via crossterm, forward to PTY
        // The background reader thread handles PTY output → stdout
        loop {
            if session.exited.load(Ordering::SeqCst) {
                write!(
                    stdout,
                    "\r\n\x1b[31m--- Session disconnected ---\x1b[0m\r\n"
                )?;
                stdout.flush()?;
                std::thread::sleep(Duration::from_millis(500));
                break;
            }

            if event::poll(Duration::from_millis(10))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        // Shift+Tab to detach (keep session running in background)
                        if key.code == KeyCode::BackTab {
                            // Stop forwarding output
                            session.attached.store(false, Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(10));
                            return Ok(());
                        }

                        // Forward key to PTY as raw bytes
                        let bytes = key_event_to_bytes(&key);
                        if !bytes.is_empty() {
                            let _ = session.writer.write_all(&bytes);
                        }
                    }
                    Event::Resize(cols, rows) => {
                        let _ = session.pty_master.resize(PtySize {
                            rows,
                            cols,
                            pixel_width: 0,
                            pixel_height: 0,
                        });
                    }
                    _ => {}
                }
            }
        }

        session.attached.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Update host list scroll to keep selection visible
    fn update_host_scroll(&mut self, viewport_height: usize) {
        if self.filtered_hosts.is_empty() {
            return;
        }

        if self.selected_host < self.host_scroll_offset {
            self.host_scroll_offset = self.selected_host;
        } else if self.selected_host >= self.host_scroll_offset + viewport_height {
            self.host_scroll_offset = self.selected_host.saturating_sub(viewport_height - 1);
        }
    }

    /// Handle keyboard input
    fn handle_key(&mut self, key: KeyEvent) -> io::Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        // Search mode
        if self.search_mode {
            match key.code {
                KeyCode::Esc => {
                    self.search_mode = false;
                    self.search_query.clear();
                    self.update_filtered_hosts();
                }
                KeyCode::Enter => {
                    self.search_mode = false;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.update_filtered_hosts();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.update_filtered_hosts();
                }
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            // --- Global ---
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_exit = true;
            }
            KeyCode::Esc => {
                self.should_exit = true;
            }

            // --- Focus switching ---
            KeyCode::BackTab => {
                if !self.sessions.is_empty() {
                    self.focus_on_hosts = !self.focus_on_hosts;
                }
            }
            KeyCode::Tab => {
                if !self.sessions.is_empty() {
                    self.focus_on_hosts = !self.focus_on_hosts;
                }
            }

            // --- Host list controls (when focused on hosts) ---
            KeyCode::Char('/') if self.focus_on_hosts => {
                self.search_mode = true;
            }
            KeyCode::Left
                if self.focus_on_hosts && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.host_panel_width = self.host_panel_width.saturating_sub(5).max(15);
            }
            KeyCode::Right
                if self.focus_on_hosts && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.host_panel_width = (self.host_panel_width + 5).min(80);
            }
            KeyCode::Up if self.focus_on_hosts => {
                if !self.filtered_hosts.is_empty() && self.selected_host > 0 {
                    self.selected_host -= 1;
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::Down if self.focus_on_hosts => {
                if !self.filtered_hosts.is_empty()
                    && self.selected_host < self.filtered_hosts.len() - 1
                {
                    self.selected_host += 1;
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::PageUp if self.focus_on_hosts => {
                if !self.filtered_hosts.is_empty() {
                    let page_size = 10.max(self.host_list_area.height.saturating_sub(3) as usize);
                    self.selected_host = self.selected_host.saturating_sub(page_size);
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::PageDown if self.focus_on_hosts => {
                if !self.filtered_hosts.is_empty() {
                    let page_size = 10.max(self.host_list_area.height.saturating_sub(3) as usize);
                    self.selected_host = (self.selected_host + page_size)
                        .min(self.filtered_hosts.len().saturating_sub(1));
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::Home if self.focus_on_hosts => {
                if !self.filtered_hosts.is_empty() {
                    self.selected_host = 0;
                    self.host_list_state.select(Some(0));
                }
            }
            KeyCode::End if self.focus_on_hosts => {
                if !self.filtered_hosts.is_empty() {
                    self.selected_host = self.filtered_hosts.len().saturating_sub(1);
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::Enter if self.focus_on_hosts => {
                self.start_session();
            }

            // --- Session list controls (when focused on sessions) ---
            KeyCode::Up if !self.focus_on_hosts => {
                if !self.sessions.is_empty() && self.selected_session > 0 {
                    self.selected_session -= 1;
                    self.session_list_state.select(Some(self.selected_session));
                }
            }
            KeyCode::Down if !self.focus_on_hosts => {
                if !self.sessions.is_empty()
                    && self.selected_session < self.sessions.len() - 1
                {
                    self.selected_session += 1;
                    self.session_list_state.select(Some(self.selected_session));
                }
            }
            KeyCode::Enter if !self.focus_on_hosts => {
                // Attach to running session
                if !self.sessions.is_empty() && self.selected_session < self.sessions.len() {
                    self.attach_request = Some(self.selected_session);
                }
            }
            KeyCode::Char('d') if !self.focus_on_hosts => {
                // Remove session (kills if running)
                if !self.sessions.is_empty() && self.selected_session < self.sessions.len() {
                    self.sessions.remove(self.selected_session);
                    if self.selected_session >= self.sessions.len() && self.selected_session > 0 {
                        self.selected_session -= 1;
                    }
                    if self.sessions.is_empty() {
                        self.focus_on_hosts = true;
                        self.session_list_state.select(None);
                    } else {
                        self.session_list_state.select(Some(self.selected_session));
                    }
                }
            }

            _ => {}
        }

        Ok(())
    }

    /// Remove any sessions that have exited
    fn cleanup_exited_sessions(&mut self) {
        let had_sessions = !self.sessions.is_empty();
        self.sessions.retain(|s| !s.exited.load(Ordering::SeqCst));
        if self.selected_session >= self.sessions.len() {
            self.selected_session = self.sessions.len().saturating_sub(1);
        }
        if self.sessions.is_empty() {
            self.session_list_state.select(None);
            if had_sessions {
                self.focus_on_hosts = true;
            }
        } else {
            self.session_list_state.select(Some(self.selected_session));
        }
    }

    /// Render the UI
    fn draw(&mut self, frame: &mut Frame) {
        let size = frame.area();

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(self.host_panel_width),
                Constraint::Min(0),
            ])
            .split(size);

        if !self.sessions.is_empty() {
            // Split left panel into host list (top) and host info (bottom)
            let left_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(60),
                    Constraint::Percentage(40),
                ])
                .split(main_chunks[0]);

            self.render_host_list(frame, left_chunks[0]);
            self.render_host_info(frame, left_chunks[1]);
            self.render_sessions(frame, main_chunks[1]);
        } else {
            self.render_host_list(frame, main_chunks[0]);
            self.render_host_details(frame, main_chunks[1]);
        }
    }

    /// Render the host list panel
    fn render_host_list(&mut self, frame: &mut Frame, area: Rect) {
        self.host_list_area = area;
        let viewport_height = area.height.saturating_sub(3) as usize;
        self.update_host_scroll(viewport_height);

        let visible_hosts: Vec<ListItem> = self
            .filtered_hosts
            .iter()
            .skip(self.host_scroll_offset)
            .take(viewport_height)
            .map(|(idx, _score)| {
                let host = &self.hosts[*idx];
                let display = if let Some(user) = &host.user {
                    format!("{}@{}", user, host.name)
                } else {
                    host.name.clone()
                };
                ListItem::new(display)
            })
            .collect();

        let title = if self.search_mode {
            format!(" Hosts (Search: {}_) ", self.search_query)
        } else {
            let total = self.filtered_hosts.len();
            let sessions_info = if !self.sessions.is_empty() {
                format!(" | {} sessions", self.sessions.len())
            } else {
                String::new()
            };
            format!(" Hosts ({}){} ", total, sessions_info)
        };

        let search_hint = if !self.search_mode {
            Span::styled(
                " / to search ",
                Style::default().fg(Color::DarkGray),
            )
        } else {
            Span::styled(
                " Esc to cancel ",
                Style::default().fg(Color::DarkGray),
            )
        };

        let border_style = if self.focus_on_hosts {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let list = List::new(visible_hosts)
            .block(
                Block::default()
                    .title(title)
                    .title_bottom(search_hint)
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        let adjusted_selection = self.selected_host.saturating_sub(self.host_scroll_offset);
        let mut adjusted_state = ListState::default();
        adjusted_state.select(Some(adjusted_selection));

        frame.render_stateful_widget(list, area, &mut adjusted_state);
    }

    /// Render compact host info panel below the host list (when sessions are visible)
    fn render_host_info(&self, frame: &mut Frame, area: Rect) {
        let content = if !self.filtered_hosts.is_empty() {
            let host_idx = self.filtered_hosts[self.selected_host].0;
            let host = &self.hosts[host_idx];

            let mut lines = vec![
                Line::from(vec![
                    Span::styled(" Host: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        &host.name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            ];

            if let Some(hostname) = &host.hostname {
                lines.push(Line::from(vec![
                    Span::styled(" Addr: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(hostname, Style::default().fg(Color::White)),
                ]));
            }

            if let Some(user) = &host.user {
                lines.push(Line::from(vec![
                    Span::styled(" User: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(user, Style::default().fg(Color::White)),
                ]));
            }

            if let Some(port) = &host.port {
                lines.push(Line::from(vec![
                    Span::styled(" Port: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(port.to_string(), Style::default().fg(Color::White)),
                ]));
            }

            if let Some(identity) = &host.identity_file {
                lines.push(Line::from(vec![
                    Span::styled(" Key:  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(identity, Style::default().fg(Color::DarkGray)),
                ]));
            }

            if let Some(proxy) = &host.proxy_jump {
                lines.push(Line::from(vec![
                    Span::styled(" Jump: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(proxy, Style::default().fg(Color::White)),
                ]));
            }

            if !host.local_forward.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled(" LFwd: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        host.local_forward.join(", "),
                        Style::default().fg(Color::White),
                    ),
                ]));
            }

            if !host.remote_forward.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled(" RFwd: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        host.remote_forward.join(", "),
                        Style::default().fg(Color::White),
                    ),
                ]));
            }

            lines
        } else {
            vec![Line::from(Span::styled(
                " No host selected",
                Style::default().fg(Color::DarkGray),
            ))]
        };

        let paragraph = Paragraph::new(content).block(
            Block::default()
                .title(" Host Info ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        frame.render_widget(paragraph, area);
    }

    /// Render host details panel (shown when no sessions exist)
    fn render_host_details(&self, frame: &mut Frame, area: Rect) {
        let content = if !self.filtered_hosts.is_empty() {
            let host_idx = self.filtered_hosts[self.selected_host].0;
            let host = &self.hosts[host_idx];

            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Host: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        &host.name,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(""),
            ];

            if let Some(hostname) = &host.hostname {
                lines.push(Line::from(vec![
                    Span::styled("  Hostname: ", Style::default().fg(Color::Gray)),
                    Span::styled(hostname, Style::default().fg(Color::White)),
                ]));
            }

            if let Some(user) = &host.user {
                lines.push(Line::from(vec![
                    Span::styled("  User:     ", Style::default().fg(Color::Gray)),
                    Span::styled(user, Style::default().fg(Color::White)),
                ]));
            }

            if let Some(port) = &host.port {
                lines.push(Line::from(vec![
                    Span::styled("  Port:     ", Style::default().fg(Color::Gray)),
                    Span::styled(port.to_string(), Style::default().fg(Color::White)),
                ]));
            }

            if let Some(identity) = &host.identity_file {
                lines.push(Line::from(vec![
                    Span::styled("  Identity: ", Style::default().fg(Color::Gray)),
                    Span::styled(identity, Style::default().fg(Color::DarkGray)),
                ]));
            }

            if let Some(proxy) = &host.proxy_jump {
                lines.push(Line::from(vec![
                    Span::styled("  ProxyJump:", Style::default().fg(Color::Gray)),
                    Span::styled(format!(" {}", proxy), Style::default().fg(Color::White)),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  Connect to host", Style::default().fg(Color::Gray)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    "/",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("      Search hosts", Style::default().fg(Color::Gray)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("    Quit", Style::default().fg(Color::Gray)),
            ]));

            lines
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No hosts found",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Press ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        "/",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" to search", Style::default().fg(Color::Gray)),
                ]),
            ]
        };

        let paragraph = Paragraph::new(content)
            .block(Block::default().title(" Details ").borders(Borders::ALL));
        frame.render_widget(paragraph, area);
    }

    /// Render the sessions list panel
    fn render_sessions(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(area);

        let items: Vec<ListItem> = self
            .sessions
            .iter()
            .map(|session| {
                let line = Line::from(vec![
                    Span::styled(
                        format!(" {} ", &session.host.name),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(
                        "\u{25cf} Running",
                        Style::default().fg(Color::Green),
                    ),
                ]);

                ListItem::new(line)
            })
            .collect();

        let border_style = if !self.focus_on_hosts {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let running = self.sessions.len();
        let title = format!(" Sessions ({}) ", running);

        let list = List::new(items)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        frame.render_stateful_widget(list, chunks[0], &mut self.session_list_state);

        // Help bar at the bottom
        let help = Line::from(vec![
            Span::styled(
                " Enter",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(": attach  ", Style::default().fg(Color::Gray)),
            Span::styled(
                "d",
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(": remove  ", Style::default().fg(Color::Gray)),
            Span::styled(
                "Tab",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(": switch focus  ", Style::default().fg(Color::Gray)),
            Span::styled(
                "Shift+Tab",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(": detach in session", Style::default().fg(Color::Gray)),
        ]);

        let help_bar = Paragraph::new(help).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        frame.render_widget(help_bar, chunks[1]);
    }
}

/// Run the interactive session manager
pub fn run_session_manager() -> io::Result<()> {
    log_debug!("Starting interactive session manager");

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new()?;

    loop {
        // Run TUI event loop until exit or attach request
        run_app(&mut terminal, &mut app)?;

        if app.should_exit {
            break;
        }

        // Handle attach request
        if let Some(idx) = app.attach_request.take() {
            // Leave TUI — enter native terminal mode
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

            // Attach to session (raw mode stays on for key interception)
            let result = app.attach_to_session(idx);

            // Return to TUI
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
            terminal.clear()?;

            if let Err(e) = result {
                log_error!("Attach error: {}", e);
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    log_debug!("Session manager exited");
    Ok(())
}

/// Run the TUI event loop until exit or attach request
fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        // Clean up exited sessions before drawing
        app.cleanup_exited_sessions();

        terminal.draw(|f| app.draw(f))?;

        if app.should_exit || app.attach_request.is_some() {
            break;
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    app.handle_key(key)?;
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    Ok(())
}
