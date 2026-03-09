//! Renderer-side syntax highlight overlays.
//!
//! This module keeps Color-SSH's semantic highlighting on the renderer side.
//! PTY bytes and canonical terminal state remain untouched; frontends consume
//! viewport text from `TerminalViewModel`, build highlight spans, and paint
//! them additively during rendering.

use super::{AnsiColor, MouseProtocolMode, TerminalViewModel};
use crate::config::{self, HighlightOverlayMode};
use crate::highlighter::CompiledHighlightRule;
use alacritty_terminal::vte::ansi::Rgb;
use regex::RegexSet;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

const MAX_RULES_FOR_REGEXSET_PREFILTER: usize = 24;
const VOLATILE_REPAINT_INTERVAL: Duration = Duration::from_millis(120);
const VOLATILE_REPAINT_SUPPRESS_FOR: Duration = Duration::from_secs(2);
const VOLATILE_REPAINT_MIN_ROWS: usize = 6;
const VOLATILE_REPAINT_STREAK_THRESHOLD: u8 = 3;
const VOLATILE_REPAINT_RATIO_NUMERATOR: usize = 7;
const VOLATILE_REPAINT_RATIO_DENOMINATOR: usize = 10;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct HighlightOverlayContext {
    pub(crate) render_epoch: u64,
    pub(crate) display_scrollback: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HighlightSuppressionReason {
    DisabledByConfig,
    AlternateScreen,
    MouseReporting,
    VolatileRepaint,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct HighlightOverlayStyle {
    fg_color: Option<AnsiColor>,
    bg_color: Option<AnsiColor>,
    bold: bool,
    italic: bool,
    underline: bool,
}

impl HighlightOverlayStyle {
    pub(crate) fn fg_color(&self) -> Option<AnsiColor> {
        self.fg_color
    }

    pub(crate) fn bg_color(&self) -> Option<AnsiColor> {
        self.bg_color
    }

    pub(crate) fn bold(&self) -> bool {
        self.bold
    }

    pub(crate) fn italic(&self) -> bool {
        self.italic
    }

    pub(crate) fn underline(&self) -> bool {
        self.underline
    }

    fn is_noop(&self) -> bool {
        self.fg_color.is_none() && self.bg_color.is_none() && !self.bold && !self.italic && !self.underline
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HighlightCellRange {
    pub(crate) start_col: u16,
    pub(crate) end_col: u16,
    pub(crate) style_index: usize,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct HighlightOverlay {
    row_ranges: HashMap<i64, Vec<HighlightCellRange>>,
    styles: Vec<HighlightOverlayStyle>,
    suppression_reason: Option<HighlightSuppressionReason>,
    pub(crate) config_version: u64,
}

impl HighlightOverlay {
    pub(crate) fn style_for_cell(&self, absolute_row: i64, col: u16) -> Option<&HighlightOverlayStyle> {
        let row_ranges = self.row_ranges.get(&absolute_row)?;
        let range = row_ranges.iter().find(|range| col >= range.start_col && col < range.end_col)?;
        self.styles.get(range.style_index)
    }
}

#[derive(Debug, Default)]
pub(crate) struct HighlightOverlayEngine {
    rules: Vec<CompiledHighlightRule>,
    rule_set: Option<RegexSet>,
    styles: Vec<HighlightOverlayStyle>,
    rule_style_indexes: Vec<Option<usize>>,
    mode: HighlightOverlayMode,
    config_version: u64,
    cached_overlay: HighlightOverlay,
    cached_render_epoch: Option<u64>,
    cached_display_scrollback: usize,
    last_visible_rows: Vec<(i64, String)>,
    last_analysis_at: Option<Instant>,
    volatile_repaint_streak: u8,
    volatile_suppressed_until: Option<Instant>,
}

impl HighlightOverlayEngine {
    /// Create an overlay engine loaded from the current runtime configuration.
    pub(crate) fn new() -> Self {
        let mut engine = Self::default();
        engine.reload_rules();
        engine
    }

    /// Rebuild renderer-side highlight spans for the currently visible rows.
    pub(crate) fn build_visible_overlay(&mut self, view: &TerminalViewModel<'_>, context: HighlightOverlayContext) -> HighlightOverlay {
        self.refresh_rules_if_needed();
        let now = Instant::now();

        if self.can_reuse_cached(context, now) {
            return self.cached_overlay.clone();
        }

        let visible_rows = view.visible_row_texts();
        let suppression_reason = self.suppression_reason(view, &visible_rows, context, now);

        let overlay = if suppression_reason.is_some() || self.styles.is_empty() {
            HighlightOverlay {
                row_ranges: HashMap::new(),
                styles: self.styles.clone(),
                suppression_reason,
                config_version: self.config_version,
            }
        } else {
            self.build_active_overlay(&visible_rows)
        };

        self.cached_overlay = overlay.clone();
        self.cached_render_epoch = Some(context.render_epoch);
        self.cached_display_scrollback = context.display_scrollback;
        overlay
    }

    fn can_reuse_cached(&self, context: HighlightOverlayContext, now: Instant) -> bool {
        if self.cached_render_epoch != Some(context.render_epoch) {
            return false;
        }
        if self.cached_display_scrollback != context.display_scrollback {
            return false;
        }
        if self.cached_overlay.config_version != self.config_version {
            return false;
        }
        if self.cached_overlay.suppression_reason == Some(HighlightSuppressionReason::VolatileRepaint)
            && self.volatile_suppressed_until.is_some_and(|until| now >= until)
        {
            return false;
        }
        true
    }

    fn build_active_overlay(&self, visible_rows: &[(i64, String)]) -> HighlightOverlay {
        let mut overlay = HighlightOverlay {
            row_ranges: HashMap::new(),
            styles: self.styles.clone(),
            suppression_reason: None,
            config_version: self.config_version,
        };

        for (absolute_row, line_text) in visible_rows {
            let line_text = line_text.trim_end_matches(' ');
            if line_text.is_empty() {
                continue;
            }

            let mut row_ranges = Vec::new();
            let use_prefilter = self.rule_set.is_some() && self.rules.len() <= MAX_RULES_FOR_REGEXSET_PREFILTER;

            if use_prefilter {
                if let Some(rule_set) = self.rule_set.as_ref() {
                    for rule_index in rule_set.matches(line_text).iter() {
                        self.collect_row_ranges(line_text, rule_index, &mut row_ranges);
                    }
                }
            } else {
                for rule_index in 0..self.rules.len() {
                    self.collect_row_ranges(line_text, rule_index, &mut row_ranges);
                }
            }

            row_ranges.sort_unstable_by(|left, right| {
                left.start_col
                    .cmp(&right.start_col)
                    .then(left.style_index.cmp(&right.style_index))
                    .then(left.end_col.cmp(&right.end_col))
            });

            let mut accepted = Vec::with_capacity(row_ranges.len());
            let mut last_end = 0u16;
            for range in row_ranges {
                if range.start_col < last_end {
                    continue;
                }
                last_end = range.end_col;
                accepted.push(range);
            }

            if !accepted.is_empty() {
                overlay.row_ranges.insert(*absolute_row, accepted);
            }
        }

        overlay
    }

    fn collect_row_ranges(&self, line_text: &str, rule_index: usize, out: &mut Vec<HighlightCellRange>) {
        let Some(style_index) = self.rule_style_indexes.get(rule_index).and_then(|style_index| *style_index) else {
            return;
        };

        for matched in self.rules[rule_index].regex.find_iter(line_text) {
            let (start_col, end_col) = byte_range_to_cell_columns(line_text, matched.start(), matched.end());
            if start_col == end_col {
                continue;
            }
            out.push(HighlightCellRange {
                start_col,
                end_col,
                style_index,
            });
        }
    }

    fn suppression_reason(
        &mut self,
        view: &TerminalViewModel<'_>,
        visible_rows: &[(i64, String)],
        context: HighlightOverlayContext,
        now: Instant,
    ) -> Option<HighlightSuppressionReason> {
        match self.mode {
            HighlightOverlayMode::Off => {
                self.reset_volatility_tracking(visible_rows, now);
                Some(HighlightSuppressionReason::DisabledByConfig)
            }
            HighlightOverlayMode::Always => {
                self.reset_volatility_tracking(visible_rows, now);
                None
            }
            HighlightOverlayMode::Auto => {
                if view.is_alternate_screen() {
                    self.reset_volatility_tracking(visible_rows, now);
                    return Some(HighlightSuppressionReason::AlternateScreen);
                }

                if view.mouse_protocol().0 != MouseProtocolMode::None {
                    self.reset_volatility_tracking(visible_rows, now);
                    return Some(HighlightSuppressionReason::MouseReporting);
                }

                if context.display_scrollback == 0 && self.should_suppress_for_volatile_repaints(visible_rows, now) {
                    return Some(HighlightSuppressionReason::VolatileRepaint);
                }

                None
            }
        }
    }

    fn should_suppress_for_volatile_repaints(&mut self, visible_rows: &[(i64, String)], now: Instant) -> bool {
        if self.volatile_suppressed_until.is_some_and(|until| now < until) {
            self.record_visible_rows(visible_rows, now);
            return true;
        }

        if self.volatile_suppressed_until.is_some_and(|until| now >= until) {
            self.volatile_suppressed_until = None;
            self.volatile_repaint_streak = 0;
        }

        let is_rapid_repaint = self
            .last_analysis_at
            .is_some_and(|last_analysis_at| now.saturating_duration_since(last_analysis_at) <= VOLATILE_REPAINT_INTERVAL);
        let changed_aggressively = viewport_changed_aggressively(&self.last_visible_rows, visible_rows);

        if is_rapid_repaint && changed_aggressively {
            self.volatile_repaint_streak = self.volatile_repaint_streak.saturating_add(1);
        } else {
            self.volatile_repaint_streak = 0;
        }

        self.record_visible_rows(visible_rows, now);

        if self.volatile_repaint_streak >= VOLATILE_REPAINT_STREAK_THRESHOLD {
            self.volatile_suppressed_until = Some(now + VOLATILE_REPAINT_SUPPRESS_FOR);
            return true;
        }

        false
    }

    fn record_visible_rows(&mut self, visible_rows: &[(i64, String)], now: Instant) {
        self.last_visible_rows.clear();
        self.last_visible_rows.extend(visible_rows.iter().cloned());
        self.last_analysis_at = Some(now);
    }

    fn reset_volatility_tracking(&mut self, visible_rows: &[(i64, String)], now: Instant) {
        self.volatile_repaint_streak = 0;
        self.volatile_suppressed_until = None;
        self.record_visible_rows(visible_rows, now);
    }

    fn refresh_rules_if_needed(&mut self) {
        let current_version = config::current_config_version();
        if current_version != self.config_version {
            self.reload_rules();
        }
    }

    fn reload_rules(&mut self) {
        let (rules, rule_set, mode) = config::with_current_config("loading highlight overlay rules", |cfg| {
            (
                cfg.metadata.compiled_rules.clone(),
                cfg.metadata.compiled_rule_set.clone(),
                cfg.interactive_settings
                    .as_ref()
                    .map(|interactive| interactive.overlay_highlighting)
                    .unwrap_or_default(),
            )
        });

        let (styles, rule_style_indexes) = build_overlay_styles(&rules);

        self.rules = rules;
        self.rule_set = rule_set;
        self.styles = styles;
        self.rule_style_indexes = rule_style_indexes;
        self.mode = mode;
        self.config_version = config::current_config_version();
        self.cached_overlay = HighlightOverlay::default();
        self.cached_render_epoch = None;
        self.cached_display_scrollback = 0;
        self.volatile_repaint_streak = 0;
        self.volatile_suppressed_until = None;
        self.last_visible_rows.clear();
        self.last_analysis_at = None;
    }

    #[cfg(test)]
    fn with_rules(rules: Vec<CompiledHighlightRule>, mode: HighlightOverlayMode) -> Self {
        let rule_set = (!rules.is_empty()).then(|| {
            let patterns: Vec<&str> = rules.iter().map(|rule| rule.regex.as_str()).collect();
            RegexSet::new(patterns).expect("rule set")
        });
        let (styles, rule_style_indexes) = build_overlay_styles(&rules);
        let config_version = config::current_config_version();
        Self {
            rules,
            rule_set,
            styles,
            rule_style_indexes,
            mode,
            config_version,
            cached_overlay: HighlightOverlay::default(),
            cached_render_epoch: None,
            cached_display_scrollback: 0,
            last_visible_rows: Vec::new(),
            last_analysis_at: None,
            volatile_repaint_streak: 0,
            volatile_suppressed_until: None,
        }
    }
}

fn build_overlay_styles(rules: &[CompiledHighlightRule]) -> (Vec<HighlightOverlayStyle>, Vec<Option<usize>>) {
    let mut styles = Vec::new();
    let mut rule_style_indexes = Vec::with_capacity(rules.len());

    for rule in rules {
        if let Some(style) = parse_overlay_style(&rule.style) {
            let style_index = styles.len();
            styles.push(style);
            rule_style_indexes.push(Some(style_index));
        } else {
            rule_style_indexes.push(None);
        }
    }

    (styles, rule_style_indexes)
}

fn parse_overlay_style(style: &str) -> Option<HighlightOverlayStyle> {
    if !style.starts_with("\x1b[") || !style.ends_with('m') {
        return None;
    }

    let params = &style[2..style.len().saturating_sub(1)];
    if params.is_empty() {
        return None;
    }

    let values: Vec<u16> = params.split(';').filter_map(|value| value.parse::<u16>().ok()).collect();
    if values.is_empty() {
        return None;
    }

    let mut overlay_style = HighlightOverlayStyle::default();
    let mut idx = 0usize;
    while idx < values.len() {
        match values[idx] {
            0 => overlay_style = HighlightOverlayStyle::default(),
            1 => overlay_style.bold = true,
            3 => overlay_style.italic = true,
            4 => overlay_style.underline = true,
            22 => overlay_style.bold = false,
            23 => overlay_style.italic = false,
            24 => overlay_style.underline = false,
            30..=37 => overlay_style.fg_color = Some(AnsiColor::Indexed((values[idx] - 30) as u8)),
            40..=47 => overlay_style.bg_color = Some(AnsiColor::Indexed((values[idx] - 40) as u8)),
            90..=97 => overlay_style.fg_color = Some(AnsiColor::Indexed((values[idx] - 90 + 8) as u8)),
            100..=107 => overlay_style.bg_color = Some(AnsiColor::Indexed((values[idx] - 100 + 8) as u8)),
            38 => {
                if let Some((color, consumed)) = parse_extended_color(&values[idx..]) {
                    overlay_style.fg_color = Some(color);
                    idx = idx.saturating_add(consumed.saturating_sub(1));
                }
            }
            48 => {
                if let Some((color, consumed)) = parse_extended_color(&values[idx..]) {
                    overlay_style.bg_color = Some(color);
                    idx = idx.saturating_add(consumed.saturating_sub(1));
                }
            }
            39 => overlay_style.fg_color = None,
            49 => overlay_style.bg_color = None,
            _ => {}
        }
        idx += 1;
    }

    (!overlay_style.is_noop()).then_some(overlay_style)
}

fn parse_extended_color(params: &[u16]) -> Option<(AnsiColor, usize)> {
    if params.len() < 2 {
        return None;
    }

    match params[1] {
        2 if params.len() >= 5 => Some((
            AnsiColor::Spec(Rgb {
                r: params[2] as u8,
                g: params[3] as u8,
                b: params[4] as u8,
            }),
            5,
        )),
        5 if params.len() >= 3 => Some((AnsiColor::Indexed(params[2] as u8), 3)),
        _ => None,
    }
}

fn viewport_changed_aggressively(previous_rows: &[(i64, String)], visible_rows: &[(i64, String)]) -> bool {
    let total_rows = previous_rows.len().max(visible_rows.len());
    if total_rows < VOLATILE_REPAINT_MIN_ROWS {
        return false;
    }

    let changed_rows = (0..total_rows).filter(|idx| previous_rows.get(*idx) != visible_rows.get(*idx)).count();

    changed_rows.saturating_mul(VOLATILE_REPAINT_RATIO_DENOMINATOR) >= total_rows.saturating_mul(VOLATILE_REPAINT_RATIO_NUMERATOR)
}

fn byte_range_to_cell_columns(text: &str, start: usize, end: usize) -> (u16, u16) {
    let mut current_col = 0u16;
    let mut start_col = None;
    let mut end_col = None;

    for (byte_index, ch) in text.char_indices() {
        if start_col.is_none() && byte_index >= start {
            start_col = Some(current_col);
        }
        if end_col.is_none() && byte_index >= end {
            end_col = Some(current_col);
            break;
        }
        current_col = current_col.saturating_add(ch.width().unwrap_or(0) as u16);
    }

    let start_col = start_col.unwrap_or(current_col);
    let end_col = end_col.unwrap_or(current_col);
    (start_col, end_col)
}

#[cfg(test)]
#[path = "../test/terminal_core_highlight_overlay.rs"]
mod tests;
