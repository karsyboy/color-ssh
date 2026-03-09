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
- Consumes viewport snapshots instead of raw stdout chunks.
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

## Overlay Performance Design

- Overlay analysis now runs from a renderer-owned viewport snapshot (`HighlightOverlayViewport`) after the terminal engine lock is released.
- The engine mutex is only held long enough to snapshot the visible cells, cursor, mouse mode, and alternate-screen state.
- Highlight analysis is row-local and cached by normalized visible row text (trailing padding is ignored because it does not affect regex matches or cell-column ranges).
- Cache reuse is content-based rather than absolute-row-based, so repeated log lines, prompt redraws, and rows that shift during scrolling can reuse the same analyzed spans.
- Cached row analyses are bounded to roughly `visible_rows * 8`, clamped to `[128, 1024]` entries, and pruned by recency to avoid unbounded memory growth during long log streams.

## Overlay Invalidation Rules

- Reuse the entire cached overlay when the render epoch and scrollback position are unchanged and volatile-suppression state is still valid.
- Reuse the entire cached overlay even across render-epoch changes when the normalized visible rows and suppression reason are unchanged.
- Reanalyze only rows whose normalized visible text is not already present in the row-analysis cache.
- Reuse cached row analyses for rows newly entering the viewport if their text was analyzed recently, which keeps scrolling and repeated output cheap.
- Resize and wrap changes invalidate only the rows whose snapped visible text changes; unchanged snapped rows keep their cached analysis.
- Config reloads clear all overlay caches because rule sets, styles, and suppression mode may have changed.
- Alternate-screen mode, mouse-reporting mode, and volatile-repaint suppression return an empty overlay without preserving stale visible-row state.

## Overlay Profiling Hooks

- `HighlightOverlayEngine` keeps in-memory counters for build kind, analyzed rows, row-cache hits/misses, cache size, and build duration.
- Safe debug logging emits periodic perf summaries and always logs slow overlay builds.
- The instrumentation is renderer-local and lock-free; it does not add cross-thread contention to the render hot path.

## Overlay Tradeoffs

- The cache is intentionally row-local and does not try to infer multi-line semantic state, which keeps correctness aligned with the current per-line regex rule model.
- Trailing-space changes are ignored for cache keys so prompt redraws and wrap padding do not trigger pointless re-analysis.
- Overlay snapshots duplicate only the visible viewport state needed for rendering, trading a small amount of transient memory for less time spent holding the terminal engine mutex.

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