//! Legacy stream-oriented syntax highlighting.
//!
//! This module still powers the direct stdout-rewriting path in
//! `src/process/stream.rs`. Embedded terminal frontends should move toward
//! renderer overlays built from `src/terminal_core/highlight_overlay.rs`
//! instead of mutating the byte stream itself.

mod ansi;
mod matcher;
mod render;

use self::ansi::{analyze_rule_reset_mode, sync_color_state_for_chunk};
use self::matcher::{MatchCollectionContext, build_clean_chunk_no_ansi, build_index_mapping, collect_chunk_matches, top_rule_timing_summary};
use self::render::render_highlighted_chunk;
use crate::{debug_enabled, log_debug, log_debug_raw};
use regex::{Regex, RegexSet};
use std::{borrow::Cow, thread, time::Instant};

#[derive(Default)]
pub struct HighlightScratch {
    clean_chunk: String,
    mapping: Vec<usize>,
    matches: Vec<(usize, usize, usize)>,
    highlighted: String,
}

pub use ansi::AnsiColorState;
pub(crate) use ansi::RuleResetMode;

#[derive(Debug, Clone)]
pub(crate) struct CompiledHighlightRule {
    pub(crate) regex: Regex,
    pub(crate) style: String,
    pub(crate) reset_mode: RuleResetMode,
}

impl CompiledHighlightRule {
    pub(crate) fn new(regex: Regex, style: String) -> Self {
        let reset_mode = analyze_rule_reset_mode(&style);
        Self { regex, style, reset_mode }
    }
}

/// Processes a chunk using reusable scratch buffers to reduce per-chunk allocations.
pub(crate) fn process_chunk_with_scratch<'a>(
    chunk: &'a str,
    chunk_id: i32,
    rules: &[CompiledHighlightRule],
    rule_set: Option<&RegexSet>,
    reset_color: &str,
    color_state: &mut AnsiColorState,
    scratch: &'a mut HighlightScratch,
) -> Cow<'a, str> {
    if chunk.is_empty() {
        return Cow::Borrowed(chunk);
    }

    let has_ansi = chunk.as_bytes().contains(&0x1b);
    let should_scan_color_state = color_state.should_scan(has_ansi);
    if rules.is_empty() {
        if should_scan_color_state {
            sync_color_state_for_chunk(chunk, color_state);
        }
        return Cow::Borrowed(chunk);
    }

    let debug_logging = debug_enabled!();
    let thread_id = debug_logging.then(|| thread::current().id());
    let build_started_at = debug_logging.then(Instant::now);
    let has_newline_or_cr = chunk.as_bytes().iter().any(|byte| matches!(*byte, b'\n' | b'\r'));

    let (clean_chunk, use_mapping) = if has_ansi {
        build_index_mapping(chunk, &mut scratch.clean_chunk, &mut scratch.mapping);
        (scratch.clean_chunk.as_str(), true)
    } else if has_newline_or_cr {
        build_clean_chunk_no_ansi(chunk, &mut scratch.clean_chunk);
        scratch.mapping.clear();
        (scratch.clean_chunk.as_str(), false)
    } else {
        scratch.mapping.clear();
        (chunk, false)
    };

    if clean_chunk.is_empty() {
        if should_scan_color_state {
            sync_color_state_for_chunk(chunk, color_state);
        }
        return Cow::Borrowed(chunk);
    }

    let build_elapsed_us = build_started_at.map_or(0, |start| start.elapsed().as_micros());
    let match_stats = collect_chunk_matches(
        MatchCollectionContext {
            clean_chunk,
            chunk_len: chunk.len(),
            use_mapping,
            mapping: &scratch.mapping,
            rules,
            rule_set,
            debug_logging,
            thread_id,
            chunk_id,
        },
        &mut scratch.matches,
    );

    if scratch.matches.is_empty() {
        if should_scan_color_state {
            sync_color_state_for_chunk(chunk, color_state);
        }
        return Cow::Borrowed(chunk);
    }

    let format_started_at = debug_logging.then(Instant::now);
    let estimated_capacity = chunk
        .len()
        .saturating_add(scratch.matches.len().saturating_mul(reset_color.len().saturating_add(16)));
    scratch.highlighted.clear();
    scratch.highlighted.reserve(estimated_capacity);

    let accepted_match_count = render_highlighted_chunk(
        chunk,
        rules,
        &scratch.matches,
        reset_color,
        should_scan_color_state,
        color_state,
        &mut scratch.highlighted,
    );

    if let Some(thread_id) = thread_id {
        let format_elapsed_us = format_started_at.map_or(0, |start| start.elapsed().as_micros());
        let total_match_elapsed_us: u128 = match_stats
            .rule_timings_ns
            .as_ref()
            .map(|timings| timings.iter().copied().sum::<u128>() / 1000)
            .unwrap_or(0);

        let top_rules = match_stats
            .rule_timings_ns
            .as_ref()
            .map(|timings| top_rule_timing_summary(timings, 5))
            .unwrap_or_default();

        log_debug!(
            "[{:?}] Chunk[{:?}] Summary: raw_len={} clean_len={} matches={} accepted={} ansi={} mapping={} prefilter_used={} build={}us prefilter={}us match={}us format={}us top_rules={}",
            thread_id,
            chunk_id,
            chunk.len(),
            clean_chunk.len(),
            scratch.matches.len(),
            accepted_match_count,
            has_ansi,
            use_mapping,
            match_stats.prefilter_used,
            build_elapsed_us,
            match_stats.prefilter_elapsed_us,
            total_match_elapsed_us,
            format_elapsed_us,
            top_rules
        );
        log_debug_raw!("[{:?}] Chunk[{:?}] 1:Raw chunk: {:?}", thread_id, chunk_id, chunk);
        log_debug_raw!("[{:?}] Chunk[{:?}] 2:Clean chunk: {:?}", thread_id, chunk_id, clean_chunk);
        log_debug_raw!("[{:?}] Chunk[{:?}] 3:Matches: {:?}", thread_id, chunk_id, scratch.matches);
        log_debug_raw!("[{:?}] Chunk[{:?}] 4:Highlighted chunk: {:?}", thread_id, chunk_id, scratch.highlighted);
    }

    Cow::Borrowed(&scratch.highlighted)
}

#[cfg(test)]
#[path = "../test/highlighter.rs"]
mod tests;
