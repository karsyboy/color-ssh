# Terminal Core Architecture

Color-SSH's interactive architecture is now PTY-centered. Interactive SSH display is derived from canonical terminal state, not from stdout rewriting.

## PTY Ownership

- Direct `cossh ssh` in `src/process/pty_runtime.rs` opens the PTY and owns the local terminal mode while that session is active.
- `TerminalSession` owns the PTY master handle when a session has one, plus the input writer, child handle, exit state, and render epoch for that session.
- TUI-managed SSH tabs also wrap their per-tab PTY state in `TerminalSession`.
- `src/process/interactive_passthrough.rs` is the only non-PTY interactive path that remains, and it exists only where a PTY-owned renderer is not yet available.

## Terminal State Ownership

- `TerminalEngine` owns `alacritty_terminal::Term` and the VTE processor.
- PTY bytes or managed-process output are applied to the engine exactly once.
- `TerminalViewModel` exposes snapshot data for visible rows, cells, cursor state, selection helpers, alternate-screen state, and mouse protocol state.
- Renderers do not read from PTY streams and do not mutate emulator state directly.

## Renderer Responsibilities

- `src/terminal_ratatui.rs` converts `TerminalViewport` snapshots into ratatui buffer cells.
- The renderer combines base terminal cell styling with optional overlay styling at paint time.
- Cursor presentation, scrollback presentation, and viewport painting are renderer concerns.
- Renderers never inject ANSI sequences back into a PTY or child stdout stream.

## Highlight Overlay Responsibilities

- `HighlightOverlayEngine` consumes a `HighlightOverlayViewport` snapshot and returns additive highlight spans for visible rows.
- It uses shared compiled regex/style data from `src/highlight_rules.rs`.
- It decides suppression based on alternate-screen state, mouse reporting, fullscreen compatibility heuristics, volatile repaint detection, and config.
- It does not rewrite PTY bytes and does not mutate `TerminalEngine` state.

## Runtime Selection

- Direct interactive SSH uses the PTY-centered runtime whenever stdin and stdout are attached to an interactive TTY.
- Direct SSH without a controlling TTY uses `src/process/interactive_passthrough.rs`.
- Direct RDP still uses a captured-output fallback when Color-SSH must inject auth data and forward FreeRDP stdout/stderr instead of handing the session to a PTY-owned renderer.

## Overlay Behavior

- Highlighting happens at render time on top of canonical terminal grid state.
- Raw PTY bytes remain unchanged, so cursor control, alternate-screen programs, and terminal correctness continue to follow `alacritty_terminal` exactly.
- Visible shell output still receives semantic keyword highlighting when the viewport is stable enough for additive decoration.
- Matching is row-local to the terminal grid. Soft-wrapped logical lines are highlighted per visible terminal row rather than across wrap boundaries.
- In `interactive_settings.overlay_highlighting: auto`, overlays are suppressed for alternate-screen sessions, mouse-reporting TUIs, suspicious primary-screen fullscreen views, and volatile repaint churn.
- `interactive_settings.overlay_auto_policy` controls whether suspicious primary-screen fullscreen views are fully suppressed, reduced to trailing shell-like rows, or left enabled unless a hard suppression applies.

## Overlay Performance Model

- Overlay analysis runs from a renderer-owned viewport snapshot after the terminal engine lock is released.
- The engine mutex is held only long enough to snapshot the visible grid, cursor visibility, alternate-screen state, and mouse protocol state.
- Row analysis is cached by normalized visible text, so repeated prompt lines, log lines, and scroll shifts can reuse prior work.
- Cache size is bounded to roughly `visible_rows * 8`, clamped to `[128, 1024]`, and pruned by recency.
- Config reloads clear overlay caches because rules, styles, or suppression mode may have changed.

## Intentionally Retained Transitional Code

- `src/process/interactive_passthrough.rs`
  - Required for direct SSH when there is no interactive controlling TTY and the PTY renderer cannot own the local terminal surface.
  - Required for the explicit RDP compatibility exception where FreeRDP still needs captured stdout/stderr forwarding.

## Long-Term Design

- Direct interactive SSH owns a PTY.
- Terminal state lives in `TerminalEngine`.
- Renderers consume snapshots from `TerminalViewModel`.
- Semantic highlighting is an overlay applied during rendering.
- No interactive SSH path depends on stdout rewriting.