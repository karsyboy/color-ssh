//! Core unit test index.
//!
//! Core tests are split into files under `src/test/` and attached to the source
//! modules via `#[path = "..."] mod tests;` so they keep access to module-private
//! items while remaining out of production files.
//!
//! CLI and process:
//! - `src/test/args.rs`
//! - `src/test/main.rs`
//! - `src/test/process.rs`
//!
//! Highlighter:
//! - `src/test/highlighter.rs`
//!
//! Configuration:
//! - `src/test/config/loader.rs`
//! - `src/test/config/watcher.rs`
//!
//! Auth:
//! - `src/test/auth/pass.rs`
//!
//! Logging:
//! - `src/test/log.rs`
//! - `src/test/log/debug.rs`
//! - `src/test/log/macros.rs`
//! - `src/test/log/ssh.rs`
//!
//! SSH config parsing:
//! - `src/test/ssh_config/include.rs`
//! - `src/test/ssh_config/parser.rs`
//!
//! TUI:
//! - `src/test/tui/terminal_emulator.rs`
//! - `src/test/tui/state/app_state.rs`
//! - `src/test/tui/ui/status_bar.rs`
//! - `src/test/tui/ui/theme.rs`
//! - `src/test/tui/features/host_browser/input.rs`
//! - `src/test/tui/features/host_browser/search.rs`
//! - `src/test/tui/features/selection/extract.rs`
//! - `src/test/tui/features/terminal_search/engine.rs`
//! - `src/test/tui/features/terminal_search/input.rs`
//! - `src/test/tui/features/terminal_search/render_highlight.rs`
//! - `src/test/tui/features/terminal_session/pty.rs`
//! - `src/test/tui/features/terminal_tabs/input.rs`
//! - `src/test/tui/features/terminal_tabs/mouse.rs`
//! - `src/test/tui/features/terminal_tabs/scroll.rs`
