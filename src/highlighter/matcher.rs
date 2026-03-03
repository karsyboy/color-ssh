use super::CompiledHighlightRule;
use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};
use std::{thread, time::Instant};

const MAX_RULES_FOR_REGEXSET_PREFILTER: usize = 24;

static ANSI_ESCAPE_REGEX: Lazy<Option<Regex>> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        \x1B\[[\x30-\x3F]*[\x20-\x2F]*[\x40-\x7E]
        |\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)
        |\x1B[PX^_].*?\x1B\\
        |\x1B.
        |\x1B
    ",
    )
    .ok()
});

#[derive(Debug, Default)]
pub(super) struct MatchPhaseStats {
    pub(super) prefilter_elapsed_us: u128,
    pub(super) rule_timings_ns: Option<Vec<u128>>,
    pub(super) prefilter_used: bool,
}

pub(super) struct MatchCollectionContext<'a> {
    pub(super) clean_chunk: &'a str,
    pub(super) chunk_len: usize,
    pub(super) use_mapping: bool,
    pub(super) mapping: &'a [usize],
    pub(super) rules: &'a [CompiledHighlightRule],
    pub(super) rule_set: Option<&'a RegexSet>,
    pub(super) debug_logging: bool,
    pub(super) thread_id: Option<thread::ThreadId>,
    pub(super) chunk_id: i32,
}

pub(super) fn collect_chunk_matches(context: MatchCollectionContext<'_>, matches: &mut Vec<(usize, usize, usize)>) -> MatchPhaseStats {
    let MatchCollectionContext {
        clean_chunk,
        chunk_len,
        use_mapping,
        mapping,
        rules,
        rule_set,
        debug_logging,
        thread_id,
        chunk_id,
    } = context;

    matches.clear();
    let mut stats = MatchPhaseStats {
        rule_timings_ns: debug_logging.then(|| vec![0u128; rules.len()]),
        ..MatchPhaseStats::default()
    };

    let mut push_matches = |rule_idx: usize, regex: &Regex| {
        let rule_started_at = debug_logging.then(Instant::now);

        for mat in regex.find_iter(clean_chunk) {
            let clean_start = mat.start();
            let clean_end = mat.end();

            let (raw_start, raw_end) = if use_mapping {
                map_clean_range_to_raw(clean_start, clean_end, mapping, chunk_len, thread_id, chunk_id)
            } else {
                (clean_start, clean_end)
            };

            matches.push((raw_start, raw_end, rule_idx));
        }

        if let (Some(start), Some(rule_timings)) = (rule_started_at, stats.rule_timings_ns.as_mut()) {
            rule_timings[rule_idx] = rule_timings[rule_idx].saturating_add(start.elapsed().as_nanos());
        }
    };

    stats.prefilter_used = rule_set.is_some() && rules.len() <= MAX_RULES_FOR_REGEXSET_PREFILTER;
    if stats.prefilter_used {
        if let Some(prefilter) = rule_set {
            let prefilter_started_at = debug_logging.then(Instant::now);
            let matched_rules = prefilter.matches(clean_chunk);
            for rule_idx in matched_rules.iter() {
                push_matches(rule_idx, &rules[rule_idx].regex);
            }
            stats.prefilter_elapsed_us = prefilter_started_at.map_or(0, |start| start.elapsed().as_micros());
        } else {
            for (rule_idx, rule) in rules.iter().enumerate() {
                push_matches(rule_idx, &rule.regex);
            }
        }
    } else {
        for (rule_idx, rule) in rules.iter().enumerate() {
            push_matches(rule_idx, &rule.regex);
        }
    }

    matches.sort_unstable_by(|left, right| left.0.cmp(&right.0).then(left.2.cmp(&right.2)).then(left.1.cmp(&right.1)));
    stats
}

fn map_clean_range_to_raw(
    clean_start: usize,
    clean_end: usize,
    mapping: &[usize],
    raw_len: usize,
    thread_id: Option<thread::ThreadId>,
    chunk_id: i32,
) -> (usize, usize) {
    let raw_start = if clean_start < mapping.len() {
        mapping[clean_start]
    } else {
        if let Some(thread_id) = thread_id {
            crate::log_debug!(
                "[{:?}] Chunk[{:?}] Index mapping fallback: clean_start {} >= mapping.len() {}",
                thread_id,
                chunk_id,
                clean_start,
                mapping.len()
            );
        }
        0
    };

    let raw_end = if clean_end < mapping.len() {
        mapping[clean_end]
    } else {
        if let Some(thread_id) = thread_id {
            crate::log_debug!(
                "[{:?}] Chunk[{:?}] Index mapping fallback: clean_end {} >= mapping.len() {}",
                thread_id,
                chunk_id,
                clean_end,
                mapping.len()
            );
        }
        raw_len
    };

    (raw_start, raw_end)
}

pub(super) fn build_clean_chunk_no_ansi(raw: &str, clean_chunk: &mut String) {
    clean_chunk.clear();
    clean_chunk.reserve(raw.len());

    for ch in raw.chars() {
        if ch == '\n' || ch == '\r' {
            clean_chunk.push(' ');
        } else {
            clean_chunk.push(ch);
        }
    }
}

pub(super) fn build_index_mapping(raw: &str, clean_chunk: &mut String, mapping: &mut Vec<usize>) {
    let Some(ansi_escape_regex) = ANSI_ESCAPE_REGEX.as_ref() else {
        build_clean_chunk_no_ansi(raw, clean_chunk);
        mapping.clear();
        mapping.reserve(raw.len().saturating_add(1));
        let mut raw_idx = 0usize;
        for ch in raw.chars() {
            let ch_len = ch.len_utf8();
            for _ in 0..ch_len {
                mapping.push(raw_idx);
            }
            raw_idx = raw_idx.saturating_add(ch_len);
        }
        mapping.push(raw_idx);
        return;
    };

    clean_chunk.clear();
    mapping.clear();
    clean_chunk.reserve(raw.len());
    mapping.reserve(raw.len().saturating_add(1));

    let mut raw_idx = 0usize;
    let mut ansi_iter = ansi_escape_regex.find_iter(raw).peekable();

    for ch in raw.chars() {
        let ch_len = ch.len_utf8();

        while let Some(escape_match) = ansi_iter.peek() {
            if raw_idx >= escape_match.end() {
                ansi_iter.next();
            } else {
                break;
            }
        }

        let in_ansi = ansi_iter
            .peek()
            .map(|escape_match| raw_idx >= escape_match.start() && raw_idx < escape_match.end())
            .unwrap_or(false);

        if !in_ansi {
            let clean_byte_pos = clean_chunk.len();
            if ch == '\n' || ch == '\r' {
                clean_chunk.push(' ');
            } else {
                clean_chunk.push(ch);
            }

            let clean_char_len = clean_chunk.len().saturating_sub(clean_byte_pos);
            for _ in 0..clean_char_len {
                mapping.push(raw_idx);
            }
        }

        raw_idx = raw_idx.saturating_add(ch_len);
    }

    mapping.push(raw_idx);
}

pub(super) fn top_rule_timing_summary(rule_timings_ns: &[u128], limit: usize) -> String {
    let mut indexed: Vec<(usize, u128)> = rule_timings_ns.iter().copied().enumerate().filter(|(_, elapsed_ns)| *elapsed_ns > 0).collect();
    indexed.sort_unstable_by(|left, right| right.1.cmp(&left.1));

    indexed
        .into_iter()
        .take(limit)
        .map(|(idx, elapsed_ns)| format!("r{}={}us", idx, elapsed_ns / 1000))
        .collect::<Vec<_>>()
        .join(",")
}
