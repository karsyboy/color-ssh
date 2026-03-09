# Terminal Core Architecture

Color-SSH now has a dedicated `src/terminal_core/` layer for embedded terminal frontends.

## Layers

### `TerminalSession`

- Owns PTY or child-process lifecycle.
- Owns input transport back to the running session.
- Owns resize coordination between the PTY surface and terminal engine.
- Tracks exit state and render invalidation epoch.

### `TerminalEngine`

- Owns `alacritty_terminal::Term` and the VTE processor.
- Applies terminal output bytes to canonical terminal state.
- Handles resize, scrollback offset, search, and selection extraction.
- Is the source of truth for embedded interactive terminal state.

### `TerminalViewModel`

- Exposes renderer-facing terminal data without exposing raw PTY streams.
- Provides visible cells, cursor state, mouse protocol state, visible row text, and selection extraction helpers.
- Is intended to be consumed by both the current TUI and a future GUI renderer.

### `HighlightOverlayEngine`

- Lives next to the terminal core, not inside process streaming.
- Consumes `TerminalViewModel` text instead of raw stdout chunks.
- Currently builds visible-row highlight ranges only.
- Exists to establish the renderer-overlay boundary before the legacy highlighter is fully replaced.

## Transitional Code Still In Use

The following code remains transitional in this phase:

- `src/process/stream.rs`
  - Still powers embedded recursive `cossh ssh` launches from the current TUI.
  - Still powers explicit legacy fallback selection for direct SSH launches.
  - Still performs stream-based stdout rewriting for syntax highlighting.

- `src/highlighter/`
  - Still contains the legacy ANSI-oriented highlighting implementation.
  - Regex rule compilation and match ordering remain reusable, but renderer overlays are the intended long-term direction.

- `src/tui/terminal_emulator.rs` and `src/tui/terminal/`
  - Now act as compatibility facades so the current TUI can adopt the new core layer with minimal churn.

## Immediate Intent

Direct `cossh ssh` launches are now expected to prefer the PTY-centered runtime in `src/process/pty_runtime.rs`.

That PTY runtime is now authoritative for direct interactive SSH behavior:

1. SSH runs inside a PTY
2. PTY bytes feed `alacritty_terminal`
3. visible terminal state is rendered from the terminal engine
4. terminal display no longer depends on stdout transformation

The current TUI recursion path remains transitional for one reason: it still launches `cossh ssh` inside its own PTY. That recursive path is explicitly marked as embedded legacy mode until the session manager launches SSH directly instead of re-entering the CLI.

This phase still does not replace every interactive session path end to end.

It does establish the ownership boundaries needed for the next phases:

1. direct PTY-backed session launching without recursive `cossh` embedding
2. PTY-side logging for embedded sessions
3. syntax highlighting moved from stdout rewriting to renderer overlays
4. alternate renderer support, including a future GUI frontend