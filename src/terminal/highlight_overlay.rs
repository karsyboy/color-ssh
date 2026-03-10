//! Renderer-side syntax highlight overlays.
//!
//! This module keeps Color-SSH's semantic highlighting on the renderer side.
//! PTY bytes and canonical terminal state remain untouched; frontends consume
//! viewport text from `TerminalViewModel`, build highlight spans, and paint
//! them additively during rendering.

use super::{AnsiColor, MouseProtocolMode, TerminalViewport};
use crate::config::{self, HighlightOverlayAutoPolicy, HighlightOverlayMode};
use crate::terminal::CompiledHighlightRule;
use crate::{debug_enabled, log_debug};
use alacritty_terminal::vte::ansi::Rgb;
use regex::RegexSet;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;

const MAX_RULES_FOR_REGEXSET_PREFILTER: usize = 24;
const MIN_ROW_ANALYSIS_CACHE_ENTRIES: usize = 128;
const MAX_ROW_ANALYSIS_CACHE_ENTRIES: usize = 1024;
const PRIMARY_SCREEN_FULLSCREEN_DENSE_FILL_NUMERATOR: usize = 3;
const PRIMARY_SCREEN_FULLSCREEN_DENSE_FILL_DENOMINATOR: usize = 5;
const PRIMARY_SCREEN_FULLSCREEN_DENSE_ROW_RATIO_NUMERATOR: usize = 3;
const PRIMARY_SCREEN_FULLSCREEN_DENSE_ROW_RATIO_DENOMINATOR: usize = 5;
const PRIMARY_SCREEN_FULLSCREEN_MIN_COLS: usize = 20;
const PRIMARY_SCREEN_FULLSCREEN_MIN_ROWS: usize = 6;
const PRIMARY_SCREEN_FULLSCREEN_NON_EMPTY_RATIO_NUMERATOR: usize = 4;
const PRIMARY_SCREEN_FULLSCREEN_NON_EMPTY_RATIO_DENOMINATOR: usize = 5;
const ROW_ANALYSIS_CACHE_VIEWPORT_MULTIPLIER: usize = 8;
const REDUCED_COMPAT_TRAILING_ROWS: usize = 4;
const PERF_LOG_INTERVAL_BUILDS: u64 = 120;
const PERF_SLOW_BUILD_THRESHOLD: Duration = Duration::from_millis(8);
const VOLATILE_REPAINT_INTERVAL: Duration = Duration::from_millis(10);
const VOLATILE_REPAINT_SUPPRESS_FOR: Duration = Duration::from_millis(20);
const VOLATILE_REPAINT_MIN_ROWS: usize = 6;
const VOLATILE_REPAINT_STREAK_THRESHOLD: u8 = 3;
const VOLATILE_REPAINT_RATIO_NUMERATOR: usize = 7;
const VOLATILE_REPAINT_RATIO_DENOMINATOR: usize = 10;

pub(crate) struct HighlightOverlayViewport<'a> {
    viewport: &'a TerminalViewport,
    alternate_screen: bool,
    cursor_hidden: bool,
    mouse_mode: MouseProtocolMode,
}

impl<'a> HighlightOverlayViewport<'a> {
    pub(crate) fn new(viewport: &'a TerminalViewport, alternate_screen: bool, mouse_mode: MouseProtocolMode, cursor_hidden: bool) -> Self {
        Self {
            viewport,
            alternate_screen,
            cursor_hidden,
            mouse_mode,
        }
    }

    fn visible_row_count(&self) -> usize {
        self.viewport.rows().len()
    }

    fn visible_col_count(&self) -> usize {
        self.viewport.size().1 as usize
    }

    fn visible_row_texts(&self) -> Vec<(i64, String)> {
        self.viewport.rows().iter().map(|row| (row.absolute_row(), row.display_text())).collect()
    }

    fn is_alternate_screen(&self) -> bool {
        self.alternate_screen
    }

    fn cursor_hidden(&self) -> bool {
        self.cursor_hidden
    }

    fn mouse_protocol_mode(&self) -> MouseProtocolMode {
        self.mouse_mode
    }
}

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
    PrimaryScreenFullscreen,
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

#[allow(dead_code)]
impl HighlightCellRange {
    pub(crate) fn start_col(&self) -> u16 {
        self.start_col
    }

    pub(crate) fn end_col(&self) -> u16 {
        self.end_col
    }

    pub(crate) fn style_index(&self) -> usize {
        self.style_index
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct HighlightOverlay {
    row_ranges: HashMap<i64, Arc<[HighlightCellRange]>>,
    styles: Vec<HighlightOverlayStyle>,
    suppression_reason: Option<HighlightSuppressionReason>,
    pub(crate) config_version: u64,
}

#[allow(dead_code)]
impl HighlightOverlay {
    pub(crate) fn style_for_cell(&self, absolute_row: i64, col: u16) -> Option<&HighlightOverlayStyle> {
        let row_ranges = self.row_ranges.get(&absolute_row)?;
        let range = row_ranges.iter().find(|range| col >= range.start_col && col < range.end_col)?;
        self.styles.get(range.style_index)
    }

    pub(crate) fn ranges_for_row(&self, absolute_row: i64) -> Option<&[HighlightCellRange]> {
        self.row_ranges.get(&absolute_row).map(|ranges| ranges.as_ref())
    }

    pub(crate) fn styles(&self) -> &[HighlightOverlayStyle] {
        &self.styles
    }

    pub(crate) fn suppression_reason(&self) -> Option<HighlightSuppressionReason> {
        self.suppression_reason
    }

    pub(crate) fn config_version(&self) -> u64 {
        self.config_version
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct OverlayVisibleRow {
    absolute_row: i64,
    text: String,
}

#[derive(Debug, Clone)]
struct CachedRowAnalysis {
    ranges: Arc<[HighlightCellRange]>,
    last_used_generation: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum HighlightCompatibilityAction {
    #[default]
    Full,
    ReduceToTrailingRows(usize),
    Disable(HighlightSuppressionReason),
}

impl HighlightCompatibilityAction {
    fn suppression_reason(self) -> Option<HighlightSuppressionReason> {
        match self {
            Self::Disable(reason) => Some(reason),
            Self::Full | Self::ReduceToTrailingRows(_) => None,
        }
    }

    fn trailing_row_limit(self) -> Option<usize> {
        match self {
            Self::ReduceToTrailingRows(rows) => Some(rows),
            Self::Full | Self::Disable(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum HighlightOverlayBuildKind {
    StrictReuse,
    SnapshotReuse,
    #[default]
    IncrementalAnalysis,
    EmptyOverlay,
}

#[derive(Debug, Clone, Default)]
struct HighlightOverlayBuildMetrics {
    kind: HighlightOverlayBuildKind,
    compatibility_action: HighlightCompatibilityAction,
    visible_rows: usize,
    analyzed_rows: usize,
    row_cache_hits: usize,
    row_cache_misses: usize,
    cache_entries: usize,
    duration: Duration,
    suppression_reason: Option<HighlightSuppressionReason>,
}

#[derive(Debug, Default)]
struct HighlightOverlayProfiler {
    build_count: u64,
    strict_reuse_count: u64,
    snapshot_reuse_count: u64,
    incremental_analysis_count: u64,
    empty_overlay_count: u64,
    total_visible_rows: u64,
    total_analyzed_rows: u64,
    total_row_cache_hits: u64,
    total_row_cache_misses: u64,
    total_duration: Duration,
    last_logged_build: u64,
    last_build: HighlightOverlayBuildMetrics,
}

impl HighlightOverlayProfiler {
    fn record_build(&mut self, metrics: HighlightOverlayBuildMetrics) {
        self.build_count = self.build_count.saturating_add(1);
        self.total_visible_rows = self.total_visible_rows.saturating_add(metrics.visible_rows as u64);
        self.total_analyzed_rows = self.total_analyzed_rows.saturating_add(metrics.analyzed_rows as u64);
        self.total_row_cache_hits = self.total_row_cache_hits.saturating_add(metrics.row_cache_hits as u64);
        self.total_row_cache_misses = self.total_row_cache_misses.saturating_add(metrics.row_cache_misses as u64);
        self.total_duration += metrics.duration;

        match metrics.kind {
            HighlightOverlayBuildKind::StrictReuse => self.strict_reuse_count = self.strict_reuse_count.saturating_add(1),
            HighlightOverlayBuildKind::SnapshotReuse => self.snapshot_reuse_count = self.snapshot_reuse_count.saturating_add(1),
            HighlightOverlayBuildKind::IncrementalAnalysis => self.incremental_analysis_count = self.incremental_analysis_count.saturating_add(1),
            HighlightOverlayBuildKind::EmptyOverlay => self.empty_overlay_count = self.empty_overlay_count.saturating_add(1),
        }

        self.last_build = metrics;
        self.maybe_log();
    }

    fn maybe_log(&mut self) {
        if !debug_enabled!() {
            return;
        }

        let should_log =
            self.last_build.duration >= PERF_SLOW_BUILD_THRESHOLD || self.build_count.saturating_sub(self.last_logged_build) >= PERF_LOG_INTERVAL_BUILDS;
        if !should_log {
            return;
        }

        self.last_logged_build = self.build_count;
        log_debug!(
            "Highlight overlay perf: kind={:?} compatibility={:?} suppression={:?} visible_rows={} analyzed_rows={} row_cache_hits={} row_cache_misses={} cache_entries={} duration_us={} totals(builds={}, strict_reuse={}, snapshot_reuse={}, active={}, empty={}, analyzed_rows={}, row_cache_hits={}, row_cache_misses={})",
            self.last_build.kind,
            self.last_build.compatibility_action,
            self.last_build.suppression_reason,
            self.last_build.visible_rows,
            self.last_build.analyzed_rows,
            self.last_build.row_cache_hits,
            self.last_build.row_cache_misses,
            self.last_build.cache_entries,
            self.last_build.duration.as_micros(),
            self.build_count,
            self.strict_reuse_count,
            self.snapshot_reuse_count,
            self.incremental_analysis_count,
            self.empty_overlay_count,
            self.total_analyzed_rows,
            self.total_row_cache_hits,
            self.total_row_cache_misses,
        );
    }
}

#[derive(Debug, Default)]
pub(crate) struct HighlightOverlayEngine {
    rules: Vec<CompiledHighlightRule>,
    rule_set: Option<RegexSet>,
    styles: Vec<HighlightOverlayStyle>,
    rule_style_indexes: Vec<Option<usize>>,
    mode: HighlightOverlayMode,
    auto_policy: HighlightOverlayAutoPolicy,
    config_version: u64,
    refresh_from_current_config: bool,
    cached_overlay: HighlightOverlay,
    cached_render_epoch: Option<u64>,
    cached_display_scrollback: usize,
    last_compatibility_action: HighlightCompatibilityAction,
    last_overlay_rows: Vec<OverlayVisibleRow>,
    row_analysis_cache: HashMap<String, CachedRowAnalysis>,
    row_cache_generation: u64,
    last_visible_rows: Vec<(i64, String)>,
    last_analysis_at: Option<Instant>,
    volatile_repaint_streak: u8,
    volatile_suppressed_until: Option<Instant>,
    profiler: HighlightOverlayProfiler,
}

impl HighlightOverlayEngine {
    /// Create an overlay engine loaded from the current runtime configuration.
    pub(crate) fn new() -> Self {
        let mut engine = Self {
            refresh_from_current_config: true,
            ..Self::default()
        };
        engine.reload_rules();
        engine
    }

    /// Create an overlay engine from a profile snapshot captured at launch time.
    pub(crate) fn from_snapshot(snapshot: &config::InteractiveProfileSnapshot) -> Self {
        let (styles, rule_style_indexes) = build_overlay_styles(&snapshot.overlay_rules);
        Self {
            rules: snapshot.overlay_rules.clone(),
            rule_set: snapshot.overlay_rule_set.clone(),
            styles,
            rule_style_indexes,
            mode: snapshot.overlay_mode,
            auto_policy: snapshot.overlay_auto_policy,
            config_version: snapshot.config_version,
            refresh_from_current_config: false,
            cached_overlay: HighlightOverlay::default(),
            cached_render_epoch: None,
            cached_display_scrollback: 0,
            last_compatibility_action: HighlightCompatibilityAction::Full,
            last_overlay_rows: Vec::new(),
            row_analysis_cache: HashMap::new(),
            row_cache_generation: 0,
            last_visible_rows: Vec::new(),
            last_analysis_at: None,
            volatile_repaint_streak: 0,
            volatile_suppressed_until: None,
            profiler: HighlightOverlayProfiler::default(),
        }
    }

    /// Rebuild renderer-side highlight spans for the currently visible rows.
    pub(crate) fn build_visible_overlay(&mut self, view: &HighlightOverlayViewport<'_>, context: HighlightOverlayContext) -> HighlightOverlay {
        let build_started_at = Instant::now();
        self.refresh_rules_if_needed();
        let now = build_started_at;

        if self.can_reuse_cached(context, now) {
            return self.finish_cached_reuse(
                context,
                HighlightOverlayBuildMetrics {
                    kind: HighlightOverlayBuildKind::StrictReuse,
                    compatibility_action: self.last_compatibility_action,
                    visible_rows: self.last_overlay_rows.len(),
                    ..HighlightOverlayBuildMetrics::default()
                },
                build_started_at,
            );
        }

        if self.mode == HighlightOverlayMode::Off {
            self.clear_volatility_tracking();
            return self.finish_overlay_build(
                self.empty_overlay(Some(HighlightSuppressionReason::DisabledByConfig)),
                &[],
                context,
                HighlightCompatibilityAction::Disable(HighlightSuppressionReason::DisabledByConfig),
                HighlightOverlayBuildMetrics {
                    kind: HighlightOverlayBuildKind::EmptyOverlay,
                    compatibility_action: HighlightCompatibilityAction::Disable(HighlightSuppressionReason::DisabledByConfig),
                    visible_rows: view.visible_row_count(),
                    suppression_reason: Some(HighlightSuppressionReason::DisabledByConfig),
                    ..HighlightOverlayBuildMetrics::default()
                },
                build_started_at,
            );
        }

        if self.styles.is_empty() {
            self.clear_volatility_tracking();
            return self.finish_overlay_build(
                self.empty_overlay(None),
                &[],
                context,
                HighlightCompatibilityAction::Full,
                HighlightOverlayBuildMetrics {
                    kind: HighlightOverlayBuildKind::EmptyOverlay,
                    compatibility_action: HighlightCompatibilityAction::Full,
                    visible_rows: view.visible_row_count(),
                    ..HighlightOverlayBuildMetrics::default()
                },
                build_started_at,
            );
        }

        if self.mode == HighlightOverlayMode::Auto {
            if view.is_alternate_screen() {
                self.clear_volatility_tracking();
                return self.finish_overlay_build(
                    self.empty_overlay(Some(HighlightSuppressionReason::AlternateScreen)),
                    &[],
                    context,
                    HighlightCompatibilityAction::Disable(HighlightSuppressionReason::AlternateScreen),
                    HighlightOverlayBuildMetrics {
                        kind: HighlightOverlayBuildKind::EmptyOverlay,
                        compatibility_action: HighlightCompatibilityAction::Disable(HighlightSuppressionReason::AlternateScreen),
                        visible_rows: view.visible_row_count(),
                        suppression_reason: Some(HighlightSuppressionReason::AlternateScreen),
                        ..HighlightOverlayBuildMetrics::default()
                    },
                    build_started_at,
                );
            }

            if view.mouse_protocol_mode() != MouseProtocolMode::None {
                self.clear_volatility_tracking();
                return self.finish_overlay_build(
                    self.empty_overlay(Some(HighlightSuppressionReason::MouseReporting)),
                    &[],
                    context,
                    HighlightCompatibilityAction::Disable(HighlightSuppressionReason::MouseReporting),
                    HighlightOverlayBuildMetrics {
                        kind: HighlightOverlayBuildKind::EmptyOverlay,
                        compatibility_action: HighlightCompatibilityAction::Disable(HighlightSuppressionReason::MouseReporting),
                        visible_rows: view.visible_row_count(),
                        suppression_reason: Some(HighlightSuppressionReason::MouseReporting),
                        ..HighlightOverlayBuildMetrics::default()
                    },
                    build_started_at,
                );
            }
        }

        let visible_rows = view.visible_row_texts();
        let overlay_rows = normalized_visible_rows(&visible_rows);
        let compatibility_action = self.compatibility_action(view, &overlay_rows);

        if compatibility_action != HighlightCompatibilityAction::Full {
            self.clear_volatility_tracking();
        }

        let suppression_reason = compatibility_action
            .suppression_reason()
            .or_else(|| (compatibility_action == HighlightCompatibilityAction::Full).then(|| self.suppression_reason(&visible_rows, context, now))?);

        if self.can_reuse_visible_overlay(&overlay_rows, suppression_reason, compatibility_action) {
            return self.finish_cached_reuse(
                context,
                HighlightOverlayBuildMetrics {
                    kind: HighlightOverlayBuildKind::SnapshotReuse,
                    compatibility_action,
                    visible_rows: overlay_rows.len(),
                    suppression_reason,
                    ..HighlightOverlayBuildMetrics::default()
                },
                build_started_at,
            );
        }

        let overlay = if suppression_reason.is_some() || self.styles.is_empty() {
            self.empty_overlay(suppression_reason)
        } else {
            let mut metrics = HighlightOverlayBuildMetrics {
                kind: HighlightOverlayBuildKind::IncrementalAnalysis,
                compatibility_action,
                visible_rows: overlay_rows.len(),
                suppression_reason,
                ..HighlightOverlayBuildMetrics::default()
            };
            let overlay = self.build_active_overlay(&overlay_rows, compatibility_action.trailing_row_limit(), &mut metrics);
            return self.finish_overlay_build(overlay, &overlay_rows, context, compatibility_action, metrics, build_started_at);
        };

        self.finish_overlay_build(
            overlay,
            &overlay_rows,
            context,
            compatibility_action,
            HighlightOverlayBuildMetrics {
                kind: HighlightOverlayBuildKind::EmptyOverlay,
                compatibility_action,
                visible_rows: overlay_rows.len(),
                suppression_reason,
                ..HighlightOverlayBuildMetrics::default()
            },
            build_started_at,
        )
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

    fn can_reuse_visible_overlay(
        &self,
        visible_rows: &[OverlayVisibleRow],
        suppression_reason: Option<HighlightSuppressionReason>,
        compatibility_action: HighlightCompatibilityAction,
    ) -> bool {
        self.cached_overlay.config_version == self.config_version
            && self.cached_overlay.suppression_reason == suppression_reason
            && self.last_compatibility_action == compatibility_action
            && self.last_overlay_rows == visible_rows
    }

    fn build_active_overlay(
        &mut self,
        visible_rows: &[OverlayVisibleRow],
        trailing_row_limit: Option<usize>,
        metrics: &mut HighlightOverlayBuildMetrics,
    ) -> HighlightOverlay {
        let mut overlay = HighlightOverlay {
            row_ranges: HashMap::with_capacity(visible_rows.len()),
            styles: self.styles.clone(),
            suppression_reason: None,
            config_version: self.config_version,
        };

        let visible_row_start = trailing_row_limit.map_or(0, |limit| visible_rows.len().saturating_sub(limit));
        for row in &visible_rows[visible_row_start..] {
            if row.text.is_empty() {
                continue;
            }

            let row_ranges = self.row_analysis_for_text(&row.text, metrics);
            if !row_ranges.is_empty() {
                overlay.row_ranges.insert(row.absolute_row, row_ranges);
            }
        }

        self.prune_row_analysis_cache(visible_rows.len());
        metrics.cache_entries = self.row_analysis_cache.len();

        overlay
    }

    fn compatibility_action(&self, view: &HighlightOverlayViewport<'_>, visible_rows: &[OverlayVisibleRow]) -> HighlightCompatibilityAction {
        if self.mode != HighlightOverlayMode::Auto || visible_rows.is_empty() {
            return HighlightCompatibilityAction::Full;
        }

        if !self.is_primary_screen_fullscreen_like(view, visible_rows) {
            return HighlightCompatibilityAction::Full;
        }

        match self.auto_policy {
            HighlightOverlayAutoPolicy::Safe => HighlightCompatibilityAction::Disable(HighlightSuppressionReason::PrimaryScreenFullscreen),
            HighlightOverlayAutoPolicy::Reduced => HighlightCompatibilityAction::ReduceToTrailingRows(REDUCED_COMPAT_TRAILING_ROWS.min(visible_rows.len())),
            HighlightOverlayAutoPolicy::Relaxed => HighlightCompatibilityAction::Full,
        }
    }

    fn is_primary_screen_fullscreen_like(&self, view: &HighlightOverlayViewport<'_>, visible_rows: &[OverlayVisibleRow]) -> bool {
        if !view.cursor_hidden() {
            return false;
        }

        let total_rows = visible_rows.len();
        if total_rows < PRIMARY_SCREEN_FULLSCREEN_MIN_ROWS {
            return false;
        }

        let cols = view.visible_col_count();
        if cols < PRIMARY_SCREEN_FULLSCREEN_MIN_COLS {
            return false;
        }

        let non_empty_rows = visible_rows.iter().filter(|row| !row.text.is_empty()).count();
        if non_empty_rows == 0
            || non_empty_rows.saturating_mul(PRIMARY_SCREEN_FULLSCREEN_NON_EMPTY_RATIO_DENOMINATOR)
                < total_rows.saturating_mul(PRIMARY_SCREEN_FULLSCREEN_NON_EMPTY_RATIO_NUMERATOR)
        {
            return false;
        }

        let dense_width_threshold = cols
            .saturating_mul(PRIMARY_SCREEN_FULLSCREEN_DENSE_FILL_NUMERATOR)
            .checked_div(PRIMARY_SCREEN_FULLSCREEN_DENSE_FILL_DENOMINATOR)
            .unwrap_or(0)
            .max(1);
        let dense_rows = visible_rows.iter().filter(|row| text_cell_width(&row.text) >= dense_width_threshold).count();

        dense_rows.saturating_mul(PRIMARY_SCREEN_FULLSCREEN_DENSE_ROW_RATIO_DENOMINATOR)
            >= non_empty_rows.saturating_mul(PRIMARY_SCREEN_FULLSCREEN_DENSE_ROW_RATIO_NUMERATOR)
    }

    fn row_analysis_for_text(&mut self, line_text: &str, metrics: &mut HighlightOverlayBuildMetrics) -> Arc<[HighlightCellRange]> {
        let generation = self.next_row_cache_generation();
        if let Some(cached) = self.row_analysis_cache.get_mut(line_text) {
            cached.last_used_generation = generation;
            metrics.row_cache_hits = metrics.row_cache_hits.saturating_add(1);
            return cached.ranges.clone();
        }

        metrics.row_cache_misses = metrics.row_cache_misses.saturating_add(1);
        metrics.analyzed_rows = metrics.analyzed_rows.saturating_add(1);

        let ranges = self.analyze_row_ranges(line_text);
        self.row_analysis_cache.insert(
            line_text.to_owned(),
            CachedRowAnalysis {
                ranges: ranges.clone(),
                last_used_generation: generation,
            },
        );
        ranges
    }

    fn analyze_row_ranges(&self, line_text: &str) -> Arc<[HighlightCellRange]> {
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

        accepted.into()
    }

    fn next_row_cache_generation(&mut self) -> u64 {
        self.row_cache_generation = self.row_cache_generation.wrapping_add(1);
        self.row_cache_generation
    }

    fn prune_row_analysis_cache(&mut self, visible_rows: usize) {
        let target_entries = visible_rows
            .saturating_mul(ROW_ANALYSIS_CACHE_VIEWPORT_MULTIPLIER)
            .clamp(MIN_ROW_ANALYSIS_CACHE_ENTRIES, MAX_ROW_ANALYSIS_CACHE_ENTRIES);

        if self.row_analysis_cache.len() <= target_entries {
            return;
        }

        let min_generation = self.row_cache_generation.saturating_sub(target_entries as u64);
        self.row_analysis_cache.retain(|_, cached| cached.last_used_generation >= min_generation);
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

    fn suppression_reason(&mut self, visible_rows: &[(i64, String)], context: HighlightOverlayContext, now: Instant) -> Option<HighlightSuppressionReason> {
        match self.mode {
            HighlightOverlayMode::Off => {
                self.clear_volatility_tracking();
                Some(HighlightSuppressionReason::DisabledByConfig)
            }
            HighlightOverlayMode::Always => {
                self.reset_volatility_tracking(visible_rows, now);
                None
            }
            HighlightOverlayMode::Auto => {
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

    fn clear_volatility_tracking(&mut self) {
        self.volatile_repaint_streak = 0;
        self.volatile_suppressed_until = None;
        self.last_visible_rows.clear();
        self.last_analysis_at = None;
    }

    fn refresh_rules_if_needed(&mut self) {
        if !self.refresh_from_current_config {
            return;
        }

        let current_version = config::current_config_version();
        if current_version != self.config_version {
            self.reload_rules();
        }
    }

    fn reload_rules(&mut self) {
        let (rules, rule_set, mode, auto_policy) = config::with_current_config("loading highlight overlay rules", |cfg| {
            let interactive = cfg.interactive_settings.as_ref();
            (
                cfg.metadata.compiled_rules.clone(),
                cfg.metadata.compiled_rule_set.clone(),
                interactive.map(|interactive| interactive.overlay_highlighting).unwrap_or_default(),
                interactive.map(|interactive| interactive.overlay_auto_policy).unwrap_or_default(),
            )
        });

        let (styles, rule_style_indexes) = build_overlay_styles(&rules);

        self.rules = rules;
        self.rule_set = rule_set;
        self.styles = styles;
        self.rule_style_indexes = rule_style_indexes;
        self.mode = mode;
        self.auto_policy = auto_policy;
        self.config_version = config::current_config_version();
        self.refresh_from_current_config = true;
        self.cached_overlay = HighlightOverlay::default();
        self.cached_render_epoch = None;
        self.cached_display_scrollback = 0;
        self.last_compatibility_action = HighlightCompatibilityAction::Full;
        self.last_overlay_rows.clear();
        self.row_analysis_cache.clear();
        self.row_cache_generation = 0;
        self.volatile_repaint_streak = 0;
        self.volatile_suppressed_until = None;
        self.last_visible_rows.clear();
        self.last_analysis_at = None;
        self.profiler = HighlightOverlayProfiler::default();
    }

    #[cfg(test)]
    fn with_rules(rules: Vec<CompiledHighlightRule>, mode: HighlightOverlayMode) -> Self {
        Self::with_rules_and_policy(rules, mode, HighlightOverlayAutoPolicy::default())
    }

    #[cfg(test)]
    fn with_rules_and_policy(rules: Vec<CompiledHighlightRule>, mode: HighlightOverlayMode, auto_policy: HighlightOverlayAutoPolicy) -> Self {
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
            auto_policy,
            config_version,
            refresh_from_current_config: false,
            cached_overlay: HighlightOverlay::default(),
            cached_render_epoch: None,
            cached_display_scrollback: 0,
            last_compatibility_action: HighlightCompatibilityAction::Full,
            last_overlay_rows: Vec::new(),
            row_analysis_cache: HashMap::new(),
            row_cache_generation: 0,
            last_visible_rows: Vec::new(),
            last_analysis_at: None,
            volatile_repaint_streak: 0,
            volatile_suppressed_until: None,
            profiler: HighlightOverlayProfiler::default(),
        }
    }
}

impl HighlightOverlayEngine {
    fn empty_overlay(&self, suppression_reason: Option<HighlightSuppressionReason>) -> HighlightOverlay {
        HighlightOverlay {
            row_ranges: HashMap::new(),
            styles: self.styles.clone(),
            suppression_reason,
            config_version: self.config_version,
        }
    }

    fn finish_cached_reuse(
        &mut self,
        context: HighlightOverlayContext,
        mut metrics: HighlightOverlayBuildMetrics,
        build_started_at: Instant,
    ) -> HighlightOverlay {
        self.cached_render_epoch = Some(context.render_epoch);
        self.cached_display_scrollback = context.display_scrollback;
        metrics.cache_entries = self.row_analysis_cache.len();
        metrics.duration = build_started_at.elapsed();
        self.profiler.record_build(metrics);
        self.cached_overlay.clone()
    }

    fn finish_overlay_build(
        &mut self,
        overlay: HighlightOverlay,
        visible_rows: &[OverlayVisibleRow],
        context: HighlightOverlayContext,
        compatibility_action: HighlightCompatibilityAction,
        mut metrics: HighlightOverlayBuildMetrics,
        build_started_at: Instant,
    ) -> HighlightOverlay {
        self.cached_overlay = overlay.clone();
        self.cached_render_epoch = Some(context.render_epoch);
        self.cached_display_scrollback = context.display_scrollback;
        self.last_compatibility_action = compatibility_action;
        self.last_overlay_rows.clear();
        self.last_overlay_rows.extend_from_slice(visible_rows);
        metrics.cache_entries = self.row_analysis_cache.len();
        metrics.duration = build_started_at.elapsed();
        self.profiler.record_build(metrics);
        overlay
    }
}

fn normalized_visible_rows(visible_rows: &[(i64, String)]) -> Vec<OverlayVisibleRow> {
    visible_rows
        .iter()
        .map(|(absolute_row, text)| OverlayVisibleRow {
            absolute_row: *absolute_row,
            text: text.trim_end_matches(' ').to_string(),
        })
        .collect()
}

fn text_cell_width(text: &str) -> usize {
    text.chars().map(|ch| ch.width().unwrap_or(0)).sum()
}

fn build_overlay_styles(rules: &[CompiledHighlightRule]) -> (Vec<HighlightOverlayStyle>, Vec<Option<usize>>) {
    let mut styles = Vec::new();
    let mut rule_style_indexes = Vec::with_capacity(rules.len());

    for rule in rules {
        if let Some(style) = parse_overlay_style(&rule.ansi_style) {
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
