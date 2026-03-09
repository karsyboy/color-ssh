//! Renderer-oriented syntax highlight overlay planning.
//!
//! This module deliberately stays separate from process streaming. The current
//! direct interactive path still uses ANSI stdout rewriting, but embedded
//! terminal frontends should evolve toward consuming renderer overlays derived
//! from canonical terminal state instead of mutating the output stream.

#![allow(dead_code)]

use super::TerminalViewModel;
use crate::config;
use crate::highlighter::CompiledHighlightRule;
use regex::RegexSet;
use std::collections::HashMap;
use unicode_width::UnicodeWidthChar;

const MAX_RULES_FOR_REGEXSET_PREFILTER: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HighlightCellRange {
    pub(crate) start_col: u16,
    pub(crate) end_col: u16,
    pub(crate) rule_index: usize,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct HighlightOverlay {
    pub(crate) row_ranges: HashMap<i64, Vec<HighlightCellRange>>,
    pub(crate) config_version: u64,
}

#[derive(Debug, Default)]
pub(crate) struct HighlightOverlayEngine {
    rules: Vec<CompiledHighlightRule>,
    rule_set: Option<RegexSet>,
    config_version: u64,
}

impl HighlightOverlayEngine {
    /// Create an overlay engine loaded from the current runtime configuration.
    pub(crate) fn new() -> Self {
        let mut engine = Self::default();
        engine.reload_rules();
        engine
    }

    /// Rebuild overlay ranges for the currently visible terminal rows.
    ///
    /// This is intentionally viewport-scoped for now. The current phase is only
    /// introducing the overlay boundary, not replacing the legacy highlighter's
    /// full-session behavior yet.
    pub(crate) fn build_visible_overlay(&mut self, view: &TerminalViewModel<'_>) -> HighlightOverlay {
        self.refresh_rules_if_needed();

        let mut overlay = HighlightOverlay {
            row_ranges: HashMap::new(),
            config_version: self.config_version,
        };

        if self.rules.is_empty() {
            return overlay;
        }

        for (absolute_row, line_text) in view.visible_row_texts() {
            if line_text.is_empty() {
                continue;
            }

            let mut row_ranges = Vec::new();
            let use_prefilter = self.rule_set.is_some() && self.rules.len() <= MAX_RULES_FOR_REGEXSET_PREFILTER;

            if use_prefilter {
                if let Some(rule_set) = self.rule_set.as_ref() {
                    for rule_index in rule_set.matches(&line_text).iter() {
                        self.collect_row_ranges(&line_text, rule_index, &mut row_ranges);
                    }
                }
            } else {
                for rule_index in 0..self.rules.len() {
                    self.collect_row_ranges(&line_text, rule_index, &mut row_ranges);
                }
            }

            row_ranges.sort_unstable_by(|left, right| {
                left.start_col
                    .cmp(&right.start_col)
                    .then(left.rule_index.cmp(&right.rule_index))
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
                overlay.row_ranges.insert(absolute_row, accepted);
            }
        }

        overlay
    }

    fn collect_row_ranges(&self, line_text: &str, rule_index: usize, out: &mut Vec<HighlightCellRange>) {
        for matched in self.rules[rule_index].regex.find_iter(line_text) {
            let (start_col, end_col) = byte_range_to_cell_columns(line_text, matched.start(), matched.end());
            if start_col == end_col {
                continue;
            }
            out.push(HighlightCellRange {
                start_col,
                end_col,
                rule_index,
            });
        }
    }

    fn refresh_rules_if_needed(&mut self) {
        let current_version = config::current_config_version();
        if current_version != self.config_version {
            self.reload_rules();
        }
    }

    fn reload_rules(&mut self) {
        let (rules, rule_set) = config::with_current_config("loading highlight overlay rules", |cfg| {
            (cfg.metadata.compiled_rules.clone(), cfg.metadata.compiled_rule_set.clone())
        });
        self.rules = rules;
        self.rule_set = rule_set;
        self.config_version = config::current_config_version();
    }
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
