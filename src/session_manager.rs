//! Interactive TUI-based SSH host selector
//!
//! Provides a terminal-based UI for selecting SSH hosts to connect to.

use crate::ssh_config::{SshHost, load_ssh_hosts};
use crate::{log_debug, log_error};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind},
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
use std::sync::{Arc, Mutex};
use std::{
    io::{self},
    process::Command,
    time::Duration,
};
use vt100::Parser;

/// Represents an SSH session output buffer
pub struct SshSession {
    /// PTY master for resizing
    pty_master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// PTY writer for sending input
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// Child process
    _child: Box<dyn Child + Send>,
    /// VT100 parser for terminal emulation
    parser: Arc<Mutex<Parser>>,
    /// Whether the process has exited
    exited: Arc<Mutex<bool>>,
}

/// Represents an open tab for a host
pub struct HostTab {
    /// The SSH host this tab represents
    host: SshHost,
    /// Tab title (usually host name)
    title: String,
    /// SSH session (if active)
    session: Option<SshSession>,
    /// Scrollback offset (0 = live view, >0 = scrolled up)
    scroll_offset: usize,
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
            return None; // Pattern not found
        }
    }

    Some(score)
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
    /// Selected host to connect to (if any)
    selected_host_to_connect: Option<SshHost>,
    /// Cached area for the host list
    host_list_area: Rect,
    /// Host list scroll offset
    host_scroll_offset: usize,
    /// Width of the host panel in columns (adjustable with Ctrl+Left/Right)
    host_panel_width: u16,
    /// Open tabs for hosts
    tabs: Vec<HostTab>,
    /// Currently selected tab index
    selected_tab: usize,
    /// Whether the focus is on the session manager (true) or tabs (false)
    focus_on_manager: bool,
    /// Start of text selection in VT100 screen coords (row, col)
    selection_start: Option<(u16, u16)>,
    /// End of text selection in VT100 screen coords (row, col)
    selection_end: Option<(u16, u16)>,
    /// Whether we're currently dragging a selection
    is_selecting: bool,
    /// Cached area for the tab terminal content (for mouse coordinate mapping)
    tab_content_area: Rect,
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
            host_panel_width: 25,
            search_query: String::new(),
            search_mode: false,
            should_exit: false,
            selected_host_to_connect: None,
            tabs: Vec::new(),
            selected_tab: 0,
            focus_on_manager: true,
            selection_start: None,
            selection_end: None,
            is_selecting: false,
            tab_content_area: Rect::default(),
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

                    // Try matching against name
                    if let Some(score) = fuzzy_match(&host.name, query) {
                        best_score = Some(score + 100); // Boost name matches
                    }

                    // Try matching against hostname
                    if let Some(hostname) = &host.hostname {
                        if let Some(score) = fuzzy_match(hostname, query) {
                            best_score = Some(best_score.unwrap_or(0).max(score + 50));
                        }
                    }

                    // Try matching against user
                    if let Some(user) = &host.user {
                        if let Some(score) = fuzzy_match(user, query) {
                            best_score = Some(best_score.unwrap_or(0).max(score + 30));
                        }
                    }

                    best_score.map(|score| (idx, score))
                })
                .collect();

            // Sort by score descending
            matches.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered_hosts = matches;
        }

        // Reset selection
        self.selected_host = 0;
        if !self.filtered_hosts.is_empty() {
            self.host_list_state.select(Some(0));
        } else {
            self.host_list_state.select(None);
        }
        self.host_scroll_offset = 0;
    }

    /// Select a host to open in a new tab
    fn select_host_to_connect(&mut self) {
        if self.filtered_hosts.is_empty() {
            return;
        }

        let host_idx = self.filtered_hosts[self.selected_host].0;
        let host = self.hosts[host_idx].clone();

        log_debug!("Opening tab for host: {}", host.name);

        // Check if tab already exists for this host
        if let Some(existing_tab_idx) = self.tabs.iter().position(|tab| tab.host.name == host.name) {
            // Focus on existing tab
            self.selected_tab = existing_tab_idx;
            self.focus_on_manager = false;
            log_debug!("Tab already exists, switching to it");
            return;
        }

        // Spawn SSH session
        let session = match Self::spawn_ssh_session(&host) {
            Ok(session) => Some(session),
            Err(e) => {
                log_error!("Failed to spawn SSH session: {}", e);
                None
            }
        };

        // Create new tab
        let tab = HostTab {
            title: host.name.clone(),
            host: host.clone(),
            session,
            scroll_offset: 0,
        };

        self.tabs.push(tab);
        self.selected_tab = self.tabs.len() - 1;
        self.focus_on_manager = false;

        log_debug!("Created new tab at index {}", self.selected_tab);
    }

    /// Spawn an SSH session in a PTY
    fn spawn_ssh_session(host: &SshHost) -> io::Result<SshSession> {
        let pty_system = native_pty_system();

        // Create a new PTY with initial size (will be resized later)
        let pty_pair = pty_system
            .openpty(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        // Build cossh command to get syntax highlighting
        let cossh_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("cossh"));

        let mut cmd = CommandBuilder::new(cossh_path);
        cmd.arg(&host.name);

        log_debug!("Spawning cossh command: cossh {}", host.name);

        // Spawn the command in the PTY
        let child = pty_pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        // Get the master for reading/writing
        let mut reader = pty_pair
            .master
            .try_clone_reader()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        let writer = pty_pair.master.take_writer().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        // Create VT100 parser for terminal emulation
        let parser = Arc::new(Mutex::new(Parser::new(40, 120, 1000)));
        let parser_clone = parser.clone();
        let exited = Arc::new(Mutex::new(false));
        let exited_clone = exited.clone();
        let pty_master = Arc::new(Mutex::new(pty_pair.master));
        let writer = Arc::new(Mutex::new(writer));

        // Spawn a thread to read from PTY
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF - process exited
                        if let Ok(mut exited) = exited_clone.lock() {
                            *exited = true;
                        }
                        break;
                    }
                    Ok(n) => {
                        let data = &buf[..n];
                        if let Ok(mut parser) = parser_clone.lock() {
                            // Process the data through VT100 parser
                            parser.process(data);
                        }
                    }
                    Err(e) => {
                        log_error!("Error reading from PTY: {}", e);
                        if let Ok(mut exited) = exited_clone.lock() {
                            *exited = true;
                        }
                        break;
                    }
                }
            }
            log_debug!("PTY reader thread exiting");
        });

        Ok(SshSession {
            pty_master,
            writer,
            _child: child,
            parser,
            exited,
        })
    }

    /// Update host list scroll to keep selection visible
    fn update_host_scroll(&mut self, viewport_height: usize) {
        if self.filtered_hosts.is_empty() {
            return;
        }

        // Symmetric scrolling: keep cursor in viewport before scrolling
        if self.selected_host < self.host_scroll_offset {
            self.host_scroll_offset = self.selected_host;
        } else if self.selected_host >= self.host_scroll_offset + viewport_height {
            self.host_scroll_offset = self.selected_host.saturating_sub(viewport_height - 1);
        }
    }

    /// Handle keyboard input
    fn handle_key(&mut self, key: KeyEvent) -> io::Result<()> {
        // Only process key press events, not release
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

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

        // If focused on a tab with an active session, forward most keys to PTY
        if !self.focus_on_manager && !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
            // Check for special control keys that should be handled by the session manager
            match key.code {
                KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_exit = true;
                    return Ok(());
                }
                KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Close current tab
                    self.tabs.remove(self.selected_tab);
                    if self.selected_tab >= self.tabs.len() && self.selected_tab > 0 {
                        self.selected_tab -= 1;
                    }
                    if self.tabs.is_empty() {
                        self.focus_on_manager = true;
                    }
                    return Ok(());
                }
                KeyCode::Tab if key.modifiers.is_empty() => {
                    // Tab without modifiers - forward to PTY for command completion
                    self.send_key_to_pty(key)?;
                    return Ok(());
                }
                KeyCode::BackTab => {
                    // Shift+Tab: Switch focus back to manager
                    self.focus_on_manager = true;
                    return Ok(());
                }
                KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                    // Alt+Left: previous tab
                    if self.selected_tab > 0 {
                        self.selected_tab -= 1;
                        self.selection_start = None;
                        self.selection_end = None;
                    }
                    return Ok(());
                }
                KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                    // Alt+Right: next tab
                    if self.selected_tab < self.tabs.len() - 1 {
                        self.selected_tab += 1;
                        self.selection_start = None;
                        self.selection_end = None;
                    }
                    return Ok(());
                }
                KeyCode::PageUp if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    // Shift+PageUp: scroll up in scrollback
                    let tab = &mut self.tabs[self.selected_tab];
                    tab.scroll_offset = tab.scroll_offset.saturating_add(10);
                    // Clamp will happen in set_scrollback
                    return Ok(());
                }
                KeyCode::PageDown if key.modifiers.contains(KeyModifiers::SHIFT) => {
                    // Shift+PageDown: scroll down (towards live)
                    let tab = &mut self.tabs[self.selected_tab];
                    tab.scroll_offset = tab.scroll_offset.saturating_sub(10);
                    return Ok(());
                }
                _ => {
                    // Any other key: reset scroll to live view, clear selection, and forward to PTY
                    self.tabs[self.selected_tab].scroll_offset = 0;
                    self.selection_start = None;
                    self.selection_end = None;
                    self.send_key_to_pty(key)?;
                    return Ok(());
                }
            }
        }

        match key.code {
            // Global commands
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_exit = true;
            }
            KeyCode::Esc => {
                // If on tabs, go back to manager
                if !self.focus_on_manager {
                    self.focus_on_manager = true;
                } else {
                    self.should_exit = true;
                }
            }

            // Tab management
            KeyCode::BackTab => {
                // Shift+Tab: Switch focus between manager and tabs
                if !self.tabs.is_empty() {
                    self.focus_on_manager = !self.focus_on_manager;
                }
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Close current tab
                if !self.focus_on_manager && !self.tabs.is_empty() {
                    self.tabs.remove(self.selected_tab);
                    if self.selected_tab >= self.tabs.len() && self.selected_tab > 0 {
                        self.selected_tab -= 1;
                    }
                    if self.tabs.is_empty() {
                        self.focus_on_manager = true;
                    }
                }
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Connect to the tab's host (exit session manager and connect normally)
                if !self.focus_on_manager && !self.tabs.is_empty() {
                    let host = self.tabs[self.selected_tab].host.clone();
                    self.selected_host_to_connect = Some(host);
                    self.should_exit = true;
                }
            }

            // Tab navigation (when focused on tabs)
            KeyCode::Left if !self.focus_on_manager => {
                if self.selected_tab > 0 {
                    self.selected_tab -= 1;
                }
            }
            KeyCode::Right if !self.focus_on_manager => {
                if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() - 1 {
                    self.selected_tab += 1;
                }
            }

            // Host list navigation (when focused on manager)
            KeyCode::Char('/') if self.focus_on_manager => {
                self.search_mode = true;
            }
            KeyCode::Left if self.focus_on_manager && key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+Left: shrink host panel
                self.host_panel_width = self.host_panel_width.saturating_sub(5).max(15);
            }
            KeyCode::Right if self.focus_on_manager && key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Ctrl+Right: grow host panel
                self.host_panel_width = (self.host_panel_width + 5).min(80);
            }
            KeyCode::Up if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() && self.selected_host > 0 {
                    self.selected_host -= 1;
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::Down if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() && self.selected_host < self.filtered_hosts.len() - 1 {
                    self.selected_host += 1;
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::PageUp if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() {
                    let page_size = 10.max(self.host_list_area.height.saturating_sub(3) as usize);
                    self.selected_host = self.selected_host.saturating_sub(page_size);
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::PageDown if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() {
                    let page_size = 10.max(self.host_list_area.height.saturating_sub(3) as usize);
                    self.selected_host = (self.selected_host + page_size).min(self.filtered_hosts.len().saturating_sub(1));
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::Home if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() {
                    self.selected_host = 0;
                    self.host_list_state.select(Some(0));
                }
            }
            KeyCode::End if self.focus_on_manager => {
                if !self.filtered_hosts.is_empty() {
                    self.selected_host = self.filtered_hosts.len().saturating_sub(1);
                    self.host_list_state.select(Some(self.selected_host));
                }
            }
            KeyCode::Enter if self.focus_on_manager => {
                self.select_host_to_connect();
            }
            _ => {}
        }

        Ok(())
    }

    /// Send keyboard input to the active PTY
    fn send_key_to_pty(&mut self, key: KeyEvent) -> io::Result<()> {
        if self.selected_tab >= self.tabs.len() {
            return Ok(());
        }

        let tab = &mut self.tabs[self.selected_tab];
        if let Some(session) = &mut tab.session {
            let bytes = match key.code {
                KeyCode::Char(c) => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        // Send control character
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
                _ => return Ok(()),
            };

            // Write to PTY using stored writer
            if let Ok(mut writer) = session.writer.lock() {
                let _ = writer.write_all(&bytes);
            }
        }

        Ok(())
    }

    /// Resize PTY for the current tab based on available area
    fn resize_current_pty(&mut self, area: Rect) {
        if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
            if let Some(session) = &mut self.tabs[self.selected_tab].session {
                // Calculate rows and cols from area (accounting for borders and status line)
                let rows = area.height.saturating_sub(3) as u16; // Subtract borders
                let cols = area.width.saturating_sub(2) as u16; // Subtract borders

                if let Ok(pty_master) = session.pty_master.lock() {
                    let _ = pty_master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }

                // Also resize the VT100 parser
                if let Ok(mut parser) = session.parser.lock() {
                    parser.set_size(rows, cols);
                }
            }
        }
    }

    /// Handle mouse events
    fn handle_mouse(&mut self, mouse: event::MouseEvent) -> io::Result<()> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Start text selection if click is in tab content area
                let area = self.tab_content_area;
                if !self.tabs.is_empty()
                    && area.width > 2
                    && area.height > 2
                    && mouse.column > area.x
                    && mouse.column < area.x + area.width - 1
                    && mouse.row > area.y
                    && mouse.row < area.y + area.height - 1
                {
                    let vt_row = mouse.row.saturating_sub(area.y + 1);
                    let vt_col = mouse.column.saturating_sub(area.x + 1);
                    self.selection_start = Some((vt_row, vt_col));
                    self.selection_end = Some((vt_row, vt_col));
                    self.is_selecting = true;
                } else {
                    self.selection_start = None;
                    self.selection_end = None;
                    self.is_selecting = false;
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if self.is_selecting {
                    let area = self.tab_content_area;
                    // Clamp to content area bounds
                    let clamped_col = mouse.column.max(area.x + 1).min(area.x + area.width.saturating_sub(2));
                    let clamped_row = mouse.row.max(area.y + 1).min(area.y + area.height.saturating_sub(2));
                    let vt_row = clamped_row.saturating_sub(area.y + 1);
                    let vt_col = clamped_col.saturating_sub(area.x + 1);
                    self.selection_end = Some((vt_row, vt_col));
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if self.is_selecting {
                    self.is_selecting = false;
                    // Copy to clipboard if there's a real selection (not just a click)
                    if self.selection_start != self.selection_end {
                        self.copy_selection_to_clipboard();
                    } else {
                        // Single click - clear selection
                        self.selection_start = None;
                        self.selection_end = None;
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                // Scroll wheel up: scroll back through PTY history
                if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
                    let tab = &mut self.tabs[self.selected_tab];
                    tab.scroll_offset = tab.scroll_offset.saturating_add(3);
                }
            }
            MouseEventKind::ScrollDown => {
                // Scroll wheel down: scroll towards live PTY view
                if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
                    let tab = &mut self.tabs[self.selected_tab];
                    tab.scroll_offset = tab.scroll_offset.saturating_sub(3);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Copy the current text selection to clipboard via OSC 52 escape sequence
    fn copy_selection_to_clipboard(&self) {
        let (start, end) = match (self.selection_start, self.selection_end) {
            (Some(s), Some(e)) => {
                // Normalize so start <= end in reading order
                if s.0 < e.0 || (s.0 == e.0 && s.1 <= e.1) {
                    (s, e)
                } else {
                    (e, s)
                }
            }
            _ => return,
        };

        if self.tabs.is_empty() || self.selected_tab >= self.tabs.len() {
            return;
        }

        let tab = &self.tabs[self.selected_tab];
        let scroll_offset = tab.scroll_offset;
        let session = match &tab.session {
            Some(s) => s,
            None => return,
        };

        let text = if let Ok(mut parser) = session.parser.lock() {
            // Set scrollback so we read the same content the user sees
            parser.set_scrollback(scroll_offset);
            let screen = parser.screen();
            let (rows, cols) = screen.size();
            let mut result = String::new();

            for row in start.0..=end.0.min(rows.saturating_sub(1)) {
                let col_start = if row == start.0 { start.1 } else { 0 };
                let col_end = if row == end.0 { end.1 } else { cols.saturating_sub(1) };
                let mut line = String::new();
                for col in col_start..=col_end.min(cols.saturating_sub(1)) {
                    if let Some(cell) = screen.cell(row, col) {
                        if cell.has_contents() {
                            line.push_str(&cell.contents());
                        } else {
                            line.push(' ');
                        }
                    }
                }
                let trimmed = line.trim_end();
                result.push_str(trimmed);
                if row < end.0.min(rows.saturating_sub(1)) {
                    result.push('\n');
                }
            }

            result
        } else {
            return;
        };

        if text.is_empty() {
            return;
        }

        // Copy to clipboard using OSC 52 escape sequence
        let encoded = base64_encode(text.as_bytes());
        let osc = format!("\x1b]52;c;{}\x07", encoded);
        let mut stdout = io::stdout();
        let _ = stdout.write_all(osc.as_bytes());
        let _ = stdout.flush();
    }

    /// Render the UI
    fn draw(&mut self, frame: &mut Frame) {
        let size = frame.area();

        // Create main layout: adjustable left panel and expanding right panel
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(self.host_panel_width), Constraint::Min(0)])
            .split(size);

        // Render host list
        self.render_host_list(frame, main_chunks[0]);

        // If there are tabs, render tabs; otherwise render host details
        if !self.tabs.is_empty() {
            self.render_tabs(frame, main_chunks[1]);
        } else {
            self.render_host_details(frame, main_chunks[1]);
        }
    }

    /// Render the host list
    fn render_host_list(&mut self, frame: &mut Frame, area: Rect) {
        // Cache area and calculate viewport
        self.host_list_area = area;
        let viewport_height = area.height.saturating_sub(3) as usize; // minus borders and title

        // Update scroll to keep selection visible
        self.update_host_scroll(viewport_height);

        // Create visible items with scrolling
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
            format!(" SSH Hosts (Search: {}_) ", self.search_query)
        } else {
            let total = self.filtered_hosts.len();
            let showing = visible_hosts.len();
            let tabs_info = if !self.tabs.is_empty() {
                format!(" | {} tabs", self.tabs.len())
            } else {
                String::new()
            };

            if self.host_scroll_offset > 0 || showing < total {
                format!(" Hosts ({}/{}){}  [Ctrl+←/→: resize] ", showing, total, tabs_info)
            } else {
                format!(" Hosts ({}){}  [Ctrl+←/→: resize] ", total, tabs_info)
            }
        };

        let border_style = if self.focus_on_manager {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let list = List::new(visible_hosts)
            .block(Block::default().title(title).borders(Borders::ALL).border_style(border_style))
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

        // Adjust list state for scrolling
        let adjusted_selection = self.selected_host.saturating_sub(self.host_scroll_offset);
        let mut adjusted_state = ListState::default();
        adjusted_state.select(Some(adjusted_selection));

        frame.render_stateful_widget(list, area, &mut adjusted_state);
    }

    /// Render the host details
    fn render_host_details(&self, frame: &mut Frame, area: Rect) {
        let content = if !self.filtered_hosts.is_empty() {
            let host_idx = self.filtered_hosts[self.selected_host].0;
            let host = &self.hosts[host_idx];

            let mut lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Host: ", Style::default().fg(Color::Gray)),
                    Span::styled(&host.name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
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
                    Span::styled("  User: ", Style::default().fg(Color::Gray)),
                    Span::styled(user, Style::default().fg(Color::White)),
                ]));
            }

            if let Some(port) = &host.port {
                lines.push(Line::from(vec![
                    Span::styled("  Port: ", Style::default().fg(Color::Gray)),
                    Span::styled(port.to_string(), Style::default().fg(Color::White)),
                ]));
            }

            if let Some(identity) = &host.identity_file {
                lines.push(Line::from(vec![
                    Span::styled("  IdentityFile: ", Style::default().fg(Color::Gray)),
                    Span::styled(identity, Style::default().fg(Color::DarkGray)),
                ]));
            }

            if let Some(proxy) = &host.proxy_jump {
                lines.push(Line::from(vec![
                    Span::styled("  ProxyJump: ", Style::default().fg(Color::Gray)),
                    Span::styled(proxy, Style::default().fg(Color::White)),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::Gray)),
                Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(" to open in new tab", Style::default().fg(Color::Gray)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::Gray)),
                Span::styled("Shift+Tab", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" to switch focus to tabs", Style::default().fg(Color::Gray)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::Gray)),
                Span::styled("Esc", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::styled(" or ", Style::default().fg(Color::Gray)),
                Span::styled("Ctrl+Q", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                Span::styled(" to quit", Style::default().fg(Color::Gray)),
            ]));

            lines
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled("No hosts found", Style::default().fg(Color::DarkGray))),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Press ", Style::default().fg(Color::Gray)),
                    Span::styled("/", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::styled(" to search", Style::default().fg(Color::Gray)),
                ]),
            ]
        };

        let paragraph = Paragraph::new(content).block(Block::default().title(" Host Details ").borders(Borders::ALL));

        frame.render_widget(paragraph, area);
    }

    /// Render the tabs panel
    fn render_tabs(&mut self, frame: &mut Frame, area: Rect) {
        // Split the area vertically: tab bar at top, content below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        // Render tab bar
        self.render_tab_bar(frame, chunks[0]);

        // Render current tab content
        if !self.tabs.is_empty() && self.selected_tab < self.tabs.len() {
            self.render_tab_content(frame, chunks[1], self.selected_tab);
        }
    }

    /// Render the tab bar showing all open tabs
    fn render_tab_bar(&self, frame: &mut Frame, area: Rect) {
        let mut tab_titles = Vec::new();

        for (idx, tab) in self.tabs.iter().enumerate() {
            let is_selected = idx == self.selected_tab && !self.focus_on_manager;

            let style = if is_selected {
                Style::default().fg(Color::Yellow).bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_selected { " [" } else { " " };
            let suffix = if is_selected { "] " } else { " " };

            tab_titles.push(Span::styled(format!("{}{}{}", prefix, &tab.title, suffix), style));

            // Add separator between tabs
            if idx < self.tabs.len() - 1 {
                tab_titles.push(Span::styled("│", Style::default().fg(Color::DarkGray)));
            }
        }

        let border_style = if self.focus_on_manager {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Cyan)
        };

        let tabs_line = Line::from(tab_titles);
        let paragraph = Paragraph::new(tabs_line).block(
            Block::default()
                .title(" Tabs [Shift+Tab: switch focus, Alt+←/→: navigate, Ctrl+W: close] ")
                .borders(Borders::ALL)
                .border_style(border_style),
        );

        frame.render_widget(paragraph, area);
    }

    /// Render the content of a specific tab
    fn render_tab_content(&mut self, frame: &mut Frame, area: Rect, tab_idx: usize) {
        if tab_idx >= self.tabs.len() {
            return;
        }

        // Resize PTY to match display area
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        self.resize_current_pty(chunks[1]);
        self.tab_content_area = chunks[1];

        let tab = &self.tabs[tab_idx];
        let host = &tab.host;
        let scroll_offset = tab.scroll_offset;
        let sel_start = self.selection_start;
        let sel_end = self.selection_end;

        // Check if session exists and get screen contents
        let (session_active, screen_lines, is_exited, _cursor_pos) = if let Some(session) = &tab.session {
            let exited = session.exited.lock().unwrap();
            let is_exited = *exited;

            // Get the terminal screen rendered line-by-line from VT100 parser
            let (lines, cursor) = if let Ok(mut parser) = session.parser.lock() {
                // Set scrollback offset for viewing history
                parser.set_scrollback(scroll_offset);
                let screen = parser.screen();
                let (rows, cols) = (screen.size().0, screen.size().1);
                let cursor_position = screen.cursor_position();
                let hide_cursor = screen.hide_cursor();

                let mut result = Vec::new();
                for row in 0..rows {
                    let mut line_spans = Vec::new();
                    let mut current_text = String::new();
                    let mut current_fg = vt100::Color::Default;
                    let mut current_bg = vt100::Color::Default;
                    let mut current_bold = false;
                    let mut current_selected = false;

                    for col in 0..cols {
                        let cell = screen.cell(row, col).unwrap();
                        let ch = if cell.has_contents() { cell.contents() } else { " ".to_string() };

                        let fg = cell.fgcolor();
                        let bg = cell.bgcolor();
                        let bold = cell.bold();
                        let is_cursor = !hide_cursor && scroll_offset == 0 && row == cursor_position.0 && col == cursor_position.1;
                        let is_selected = is_cell_in_selection(row, col, sel_start, sel_end);

                        // Check if style changed, cursor, or selection boundary
                        if fg != current_fg || bg != current_bg || bold != current_bold || is_cursor || is_selected != current_selected {
                            // Flush current span if any
                            if !current_text.is_empty() {
                                let style = if current_selected {
                                    let mut s = Style::default().bg(Color::Blue).fg(Color::White);
                                    if current_bold {
                                        s = s.add_modifier(Modifier::BOLD);
                                    }
                                    s
                                } else {
                                    let mut s = Style::default();
                                    if current_fg != vt100::Color::Default {
                                        s = s.fg(vt100_to_ratatui_color(current_fg));
                                    }
                                    if current_bold {
                                        s = s.add_modifier(Modifier::BOLD);
                                    }
                                    s
                                };

                                line_spans.push(Span::styled(current_text.clone(), style));
                                current_text.clear();
                            }

                            // If this is the cursor cell, emit it as a separate span
                            if is_cursor {
                                let mut style = Style::default().bg(Color::White).fg(Color::Black);
                                if bold {
                                    style = style.add_modifier(Modifier::BOLD);
                                }
                                line_spans.push(Span::styled(ch.clone(), style));

                                // Reset for next span
                                current_fg = vt100::Color::Default;
                                current_bg = vt100::Color::Default;
                                current_bold = false;
                                current_selected = false;
                                continue;
                            }

                            current_fg = fg;
                            current_bg = bg;
                            current_bold = bold;
                            current_selected = is_selected;
                        }

                        current_text.push_str(&ch);
                    }

                    // Flush remaining text in the line
                    if !current_text.is_empty() {
                        let style = if current_selected {
                            let mut s = Style::default().bg(Color::Blue).fg(Color::White);
                            if current_bold {
                                s = s.add_modifier(Modifier::BOLD);
                            }
                            s
                        } else {
                            let mut s = Style::default();
                            if current_fg != vt100::Color::Default {
                                s = s.fg(vt100_to_ratatui_color(current_fg));
                            }
                            if current_bold {
                                s = s.add_modifier(Modifier::BOLD);
                            }
                            s
                        };

                        line_spans.push(Span::styled(current_text, style));
                    }

                    result.push(Line::from(line_spans));
                }
                let cur = if !hide_cursor { Some(cursor_position) } else { None };
                (result, cur)
            } else {
                (Vec::new(), None)
            };

            (true, lines, is_exited, cursor)
        } else {
            (false, Vec::new(), false, None)
        };

        if session_active {
            // Status line
            let scroll_info = if scroll_offset > 0 {
                format!(" | [SCROLLBACK +{}] Shift+PgUp/PgDn", scroll_offset)
            } else {
                String::new()
            };
            let status_text = if is_exited {
                format!("Status: Disconnected [Press Ctrl+W to close] | Host: {}{}", host.name, scroll_info)
            } else {
                format!("Status: Connected to {} | Shift+Tab to switch{}", host.name, scroll_info)
            };

            let status_style = if is_exited {
                Style::default().fg(Color::Red)
            } else if scroll_offset > 0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Green)
            };

            let status = Paragraph::new(status_text).style(status_style).block(Block::default());

            frame.render_widget(status, chunks[0]);

            // Terminal output from VT100 screen (rendered line by line)
            let terminal_output = Paragraph::new(screen_lines)
                .block(Block::default().borders(Borders::ALL).title(format!(" {} ", &tab.title)))
                .style(Style::default());

            frame.render_widget(terminal_output, chunks[1]);
        } else {
            // Session failed to start
            let error_lines = vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("Failed to start SSH session for ", Style::default().fg(Color::Red)),
                    Span::styled(&host.name, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Press ", Style::default().fg(Color::Gray)),
                    Span::styled("Ctrl+W", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::styled(" to close this tab", Style::default().fg(Color::Gray)),
                ]),
            ];

            let paragraph = Paragraph::new(error_lines).block(
                Block::default()
                    .title(format!(" {} ", &tab.title))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red)),
            );

            frame.render_widget(paragraph, area);
        }
    }
}

/// Check if a cell at (row, col) is within the current text selection
fn is_cell_in_selection(row: u16, col: u16, start: Option<(u16, u16)>, end: Option<(u16, u16)>) -> bool {
    let (start, end) = match (start, end) {
        (Some(s), Some(e)) => {
            // Normalize so start <= end in reading order
            if s.0 < e.0 || (s.0 == e.0 && s.1 <= e.1) {
                (s, e)
            } else {
                (e, s)
            }
        }
        _ => return false,
    };

    if row < start.0 || row > end.0 {
        return false;
    }
    if start.0 == end.0 {
        // Single row: selected from start.1 to end.1
        col >= start.1 && col <= end.1
    } else if row == start.0 {
        // First row: from start.1 to end of line
        col >= start.1
    } else if row == end.0 {
        // Last row: from beginning to end.1
        col <= end.1
    } else {
        // Middle rows: entirely selected
        true
    }
}

/// Encode bytes as base64 for OSC 52 clipboard escape sequence
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((n >> 18) & 0x3f) as usize] as char);
        result.push(CHARS[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 0x3f) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Convert VT100 color to Ratatui color
fn vt100_to_ratatui_color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(idx) => match idx {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::Gray,
            8 => Color::DarkGray,
            9 => Color::LightRed,
            10 => Color::LightGreen,
            11 => Color::LightYellow,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            15 => Color::White,
            _ => Color::Indexed(idx),
        },
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Run the interactive session manager
pub fn run_session_manager() -> io::Result<()> {
    log_debug!("Starting interactive session manager");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;

    // Main loop
    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(err) = result {
        log_error!("Session manager error: {}", err);
        eprintln!("Error: {}", err);
        return Err(err);
    }

    // If a host was selected, connect to it
    if let Some(host) = app.selected_host_to_connect {
        log_debug!("Connecting to host: {}", host.name);

        // Get current executable (cossh)
        let cossh_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("cossh"));

        // Execute cossh with the host
        let status = Command::new(cossh_path).arg(&host.name).status()?;

        if !status.success() {
            log_error!("SSH connection failed with code: {:?}", status.code());
        }
    }

    log_debug!("Session manager exited");
    Ok(())
}

/// Run the app event loop
fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| app.draw(f))?;

        if app.should_exit {
            break;
        }

        // Poll for events with shorter timeout for better responsiveness
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    app.handle_key(key)?;
                }
                Event::Mouse(mouse) => {
                    app.handle_mouse(mouse)?;
                }
                Event::Resize(_, _) => {
                    // Terminal was resized, redraw on next iteration
                }
                _ => {}
            }
        }
    }

    Ok(())
}
