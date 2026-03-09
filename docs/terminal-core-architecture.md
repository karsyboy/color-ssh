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
- Can snapshot the visible grid into backend-neutral viewport rows/cells so ratatui today and a future GUI can render the same terminal state.
- Is intended to be consumed by both the current TUI and a future GUI renderer.

### `HighlightOverlayEngine`

- Lives next to the terminal core, not inside process streaming.
- Consumes `TerminalViewModel` text instead of raw stdout chunks.
- Reuses the existing compiled regex rules, but converts their ANSI styles into frontend-neutral overlay styles.
- Returns additive highlight spans for currently visible rows without mutating PTY bytes or `alacritty_terminal` state.
- Is reusable by the ratatui frontend today and a future GUI renderer later.

## Overlay Highlighting Behavior

- Highlighting now happens at render time on top of terminal grid state.
- Raw PTY bytes remain unchanged, so cursor control, alternate-screen programs, and terminal correctness continue to follow `alacritty_terminal` exactly.
- Visible shell output still receives semantic keyword highlighting when the viewport is stable.
- Highlighting is automatically suppressed in `interactive_settings.overlay_highlighting: auto` when:
  - the terminal is in the alternate screen
  - mouse-reporting modes indicate a TUI-style application
  - the visible viewport is repainting aggressively enough that semantic overlays become noisy or misleading
- `interactive_settings.overlay_highlighting: always` forces overlays on even in those cases.
- `interactive_settings.overlay_highlighting: off` disables the renderer-side overlay entirely.

Overlay mode is safer than stream rewriting because the renderer only changes presentation. It does not inject ANSI sequences back into the PTY stream, so remote programs and local terminal emulation continue to observe the original byte stream.

## Transitional Code Still In Use

The following code remains transitional in this phase:

- `src/process/stream.rs`
  - Still powers explicit legacy fallback selection for direct SSH launches.
  - Still powers embedded recursive `cossh ssh` launches from the current TUI, but those embedded launches now run in plain stream mode so the outer renderer owns highlighting.
  - Still contains the legacy stream-based stdout rewriting path for fallback use.

- `src/highlighter/`
  - Still contains the legacy ANSI-oriented highlighting implementation.
  - Regex rule compilation and match ordering remain reusable, but renderer overlays are now the default direction for interactive terminal highlighting.

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
3. broader reuse of renderer-side syntax overlays across frontends
4. alternate renderer support, including a future GUI frontend