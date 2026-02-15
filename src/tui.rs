//! Interactive TUI-based SSH host selector
//!
//! Provides a terminal-based UI for selecting SSH hosts to connect to.
//!
//! This module is organized into submodules:
//! - [`input`] — Keyboard and mouse input handling
//! - [`render`] — UI rendering (host list, tabs, terminal content)
//! - [`search`] — Fuzzy matching and host filtering
//! - [`selection`] — Text selection and clipboard support
//! - [`ssh_session`] — PTY session spawning and management

mod input;
mod render;
mod search;
mod selection;
mod ssh_session;

use crate::ssh_config::{SshHost, load_ssh_hosts};
use crate::{config, log_debug, log_error};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use portable_pty::{Child, MasterPty};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect, widgets::ListState};
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::{
    io::{self},
    process::Command,
    time::{Duration, Instant},
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
    /// Flag indicating a clear screen sequence was detected
    clear_pending: Arc<Mutex<bool>>,
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
    /// Start of text selection as absolute coords (absolute_row, col)
    /// absolute_row = screen_row - scroll_offset (can be negative for scrollback)
    selection_start: Option<(i64, u16)>,
    /// End of text selection as absolute coords (absolute_row, col)
    selection_end: Option<(i64, u16)>,
    /// Whether we're currently dragging a selection
    is_selecting: bool,
    /// Whether a drag event actually moved the mouse (distinguishes click from single-char select)
    selection_dragged: bool,
    /// Cached area for the tab terminal content (for mouse coordinate mapping)
    tab_content_area: Rect,
    /// Cached area for the tab bar (for mouse click on tabs)
    tab_bar_area: Rect,
    /// Cached area for the entire host panel (host list + info)
    host_panel_area: Rect,
    /// Last left-click time and position (for double-click detection)
    last_click: Option<(Instant, u16, u16)>,
    /// Whether we're currently dragging the panel divider to resize
    is_dragging_divider: bool,
    /// Cached area for the exit button (top-right corner)
    exit_button_area: Rect,
    /// Horizontal scroll offset for the tab bar (in chars)
    tab_scroll_offset: usize,
    /// History buffer size (scrollback lines for VT100 parser)
    history_buffer: usize,
    /// Whether we're in terminal search mode (Ctrl+F)
    terminal_search_mode: bool,
    /// Terminal search query
    terminal_search_query: String,
    /// Match positions for terminal search (row, col, length)
    terminal_search_matches: Vec<(i64, u16, usize)>,
    /// Current match index in terminal search
    terminal_search_current: usize,
    /// Whether the host panel is visible (can be toggled with Ctrl+B)
    host_panel_visible: bool,
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
            selection_dragged: false,
            tab_content_area: Rect::default(),
            tab_bar_area: Rect::default(),
            host_panel_area: Rect::default(),
            last_click: None,
            is_dragging_divider: false,
            exit_button_area: Rect::default(),
            tab_scroll_offset: 0,
            history_buffer: config::SESSION_CONFIG
                .get()
                .and_then(|c| c.read().ok().map(|cfg| cfg.settings.history_buffer))
                .unwrap_or(1000),
            terminal_search_mode: false,
            terminal_search_query: String::new(),
            terminal_search_matches: Vec::new(),
            terminal_search_current: 0,
            host_panel_visible: true,
        })
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
        // Check for pending clear screen and reset scroll offset
        app.check_clear_pending();

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
