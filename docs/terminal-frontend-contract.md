# Terminal Frontend Contract

This document defines the renderer-facing contract for Color-SSH's PTY-backed terminal core.

The goal is to let the current ratatui frontend and a future GUI frontend consume the same terminal state, cursor state, selection coordinates, scrollback metadata, and overlay highlight data without redesigning the backend.

## Adapter Boundary

The explicit frontend boundary now lives in `src/terminal_core`:

- `TerminalSession::snapshot_for_frontend(max_rows, max_cols, display_scrollback)`
  - Locks the session engine, projects the requested scrollback offset into a read-only `TerminalSessionSnapshot`, and returns it without mutating canonical terminal state.
- `TerminalSessionSnapshot`
  - Carries the `render_epoch` used by overlay caching.
  - Exposes `frontend()` and `viewport()` for renderer consumption.
  - Builds highlight overlays through `build_highlight_overlay(...)`.
- `TerminalFrontendSnapshot`
  - Owns the frontend-facing terminal payload for one frame.
  - Exposes viewport rows/cells, cursor state, alternate-screen state, mouse protocol state, and scrollback metadata.
- `TerminalSelection`
  - Stores selection endpoints in terminal coordinates instead of TUI-specific tuples.
  - Can be reused by any frontend that owns its own local selection UI.

This is the contract a future GUI terminal widget should consume first.

## Reusable Core Surface

The following pieces are renderer-neutral and intended to be shared by both ratatui and a future GUI:

- `TerminalSession`
  - Process and PTY lifecycle.
  - Input transport.
  - Render epoch tracking.
- `TerminalEngine`
  - Canonical `alacritty_terminal::Term` state.
  - Scrollback state.
  - Selection/search extraction in terminal coordinates.
- `TerminalFrontendSnapshot`
  - `viewport()` for visible rows and cells.
  - `cursor()` for hidden-vs-position cursor semantics.
  - `scrollback()` for `display_offset` and `max_offset`.
  - `is_alternate_screen()`.
  - `mouse_protocol()`.
- `HighlightOverlayEngine`
  - Builds renderer-side highlight overlays from frontend snapshots.
- `HighlightOverlay`
  - `ranges_for_row(...)` exposes row-local highlight spans.
  - `styles()` exposes the reusable style table.
  - `style_for_cell(...)` remains convenient for cell-by-cell renderers.
- `TerminalSelection` and `TerminalGridPoint`
  - Typed terminal-coordinate selection data.

## Still TUI-Specific

These parts intentionally remain ratatui/TUI-specific and should not move into `terminal_core` unless another frontend needs the same behavior in the same form:

- `src/terminal_ratatui.rs`
  - Ratatui `Buffer`, `Style`, `Color`, `Frame`, toast, and cursor painting.
- `src/tui/features/terminal_tabs/render.rs`
  - Search-result styling.
  - Selection styling colors.
  - Scrollbar painting.
  - Tab layout and pane composition.
- `src/process/pty_runtime.rs`
  - Direct-mode raw terminal ownership.
  - Crossterm input/event loop.
  - Host-terminal restoration.

Those layers consume the shared snapshots, but they remain presentation and platform glue rather than terminal-core responsibilities.

## Cursor And Scrollback Semantics

`TerminalViewport::cursor()` is still convenient for the ratatui path, but future frontends should prefer `TerminalFrontendSnapshot::cursor()` because it distinguishes:

- cursor hidden by terminal mode
- cursor position on the terminal surface
- cursor visibility inside a clipped viewport

`TerminalFrontendSnapshot::scrollback()` also reports both:

- `display_offset`: the active visible scrollback offset after clamping
- `max_offset`: the maximum available scrollback depth

This gives GUI frontends enough information to drive their own scrollbars without re-reading engine internals.

Cursor rendering is positional and style-driven only. Frontends should not replace blank cursor cells with placeholder glyphs; if the cursor lands on a blank cell, the underlying cell content remains blank and the frontend is responsible only for cursor visibility/styling.

## Overlay Contract

Highlight overlays stay renderer-side and additive.

- The PTY byte stream is never rewritten.
- The terminal engine remains the source of truth.
- Frontends may consume overlays either:
  - cell-by-cell through `style_for_cell(...)`, or
  - row-by-row through `ranges_for_row(...)` plus `styles()`.

The row-range form is the preferred contract for a future GUI renderer because it avoids recomputing span grouping in the widget layer.

## GPUI Integration Notes

A future GPUI terminal widget should follow this shape:

1. Own a `TerminalSession` plus a `HighlightOverlayEngine`.
2. On each paint or invalidation, call `snapshot_for_frontend(...)` with the widget's visible rows, columns, and current scrollback offset.
3. Paint `snapshot.viewport().rows()` into GPUI text/cell primitives.
4. Use `snapshot.cursor()` to decide whether the cursor is hidden and whether it falls inside the current clipped viewport.
5. Build overlay spans through `snapshot.build_highlight_overlay(...)`.
6. Apply overlay row ranges onto the base cell styling during paint.
7. Keep selection UI in GPUI state using `TerminalSelection`; call `TerminalSession::selection_text_for(...)` when exporting selected text.

GPUI should not:

- read PTY bytes directly for rendering
- parse ANSI sequences outside `TerminalEngine`
- reimplement overlay suppression heuristics
- depend on ratatui types or `terminal_ratatui.rs`

## Minimal Rendering Flow

```rust
let snapshot = session.snapshot_for_frontend(rows, cols, display_scrollback)?;
let overlay = snapshot.build_highlight_overlay(&mut highlight_overlay);

for row in snapshot.viewport().rows() {
    if let Some(ranges) = overlay.ranges_for_row(row.absolute_row()) {
        // map ranges onto GUI spans or cell runs
    }
}
```

The ratatui frontend already follows this shape now. A GUI frontend can reuse the same snapshots and overlay engine without changing the PTY or emulator layers.