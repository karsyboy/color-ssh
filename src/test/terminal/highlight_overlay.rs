use super::{HighlightCompatibilityAction, HighlightOverlayBuildKind, HighlightOverlayEngine, HighlightSuppressionReason, viewport_changed_aggressively};
use crate::config::{CompiledHighlightRule, HighlightOverlayAutoPolicy, HighlightOverlayMode, InteractiveProfileSnapshot};
use crate::terminal::{AnsiColor, TerminalEngine};
use alacritty_terminal::vte::ansi::Rgb;
use regex::Regex;

#[path = "highlight_overlay/cache_behavior.rs"]
mod cache_behavior;
#[path = "highlight_overlay/rendering.rs"]
mod rendering;
#[path = "highlight_overlay/suppression_policy.rs"]
mod suppression_policy;

fn compiled_rule(pattern: &str, style: &str) -> CompiledHighlightRule {
    CompiledHighlightRule::new(Regex::new(pattern).expect("regex"), style.to_string())
}

fn build_overlay_for_engine(
    terminal_engine: &mut TerminalEngine,
    overlay_engine: &mut HighlightOverlayEngine,
    render_epoch: u64,
    display_scrollback: usize,
) -> super::HighlightOverlay {
    terminal_engine.set_display_scrollback(display_scrollback);
    let view = terminal_engine.view_model();
    let (rows, cols) = view.size();
    view.frontend_snapshot(rows, cols).build_highlight_overlay(overlay_engine, render_epoch)
}

fn render_dense_lines(terminal_engine: &mut TerminalEngine, line_count: usize, prefix: &str) {
    let mut bytes = String::new();
    for row_idx in 0..line_count {
        if row_idx > 0 {
            bytes.push_str("\r\n");
        }
        bytes.push_str(&format!("{prefix}{row_idx:02} error error"));
    }
    terminal_engine.process_output(bytes.as_bytes());
}
