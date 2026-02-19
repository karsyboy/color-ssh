use crate::{debug_enabled, log_debug};
use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};
use std::{borrow::Cow, thread, time::Instant};

// Compiled regex for stripping ANSI escape sequences before pattern matching.
static ANSI_ESCAPE_REGEX: Lazy<Option<Regex>> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        \x1B\[[\x30-\x3F]*[\x20-\x2F]*[\x40-\x7E]    # CSI: ESC [ params intermediates final
        |\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)           # OSC: ESC ] ... (BEL or ESC \)
        |\x1B[PX^_].*?\x1B\\                         # DCS/SOS/PM/APC: ESC P/X/^/_ ... ESC \
        |\x1B.                                       # Other ESC sequences (2 bytes)
        |\x1B                                        # Stray ESC character
    ",
    )
    .ok()
});

const MAX_RULES_FOR_REGEXSET_PREFILTER: usize = 24;

#[derive(Default)]
pub struct HighlightScratch {
    clean_chunk: String,
    mapping: Vec<usize>,
    matches: Vec<(usize, usize, usize)>,
    highlighted: String,
}

/// Processes a chunk using reusable scratch buffers to reduce per-chunk allocations.
pub fn process_chunk_with_scratch<'a>(
    chunk: &'a str,
    chunk_id: i32,
    rules: &[(Regex, String)],
    rule_set: Option<&RegexSet>,
    reset_color: &str,
    scratch: &'a mut HighlightScratch,
) -> Cow<'a, str> {
    if chunk.is_empty() || rules.is_empty() {
        return Cow::Borrowed(chunk);
    }

    let debug_logging = debug_enabled!();
    let thread_id = debug_logging.then(|| thread::current().id());

    let build_started_at = debug_logging.then(Instant::now);
    let has_ansi = chunk.as_bytes().contains(&0x1b);
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
        return Cow::Borrowed(chunk);
    }

    let build_elapsed_us = build_started_at.map_or(0, |start| start.elapsed().as_micros());

    scratch.matches.clear();
    let mut rule_timings_ns = debug_logging.then(|| vec![0u128; rules.len()]);
    let mut prefilter_elapsed_us = 0u128;

    let mut push_matches = |rule_idx: usize, regex: &Regex| {
        let rule_started_at = debug_logging.then(Instant::now);

        for mat in regex.find_iter(clean_chunk) {
            let clean_start = mat.start();
            let clean_end = mat.end();

            let (raw_start, raw_end) = if use_mapping {
                map_clean_range_to_raw(clean_start, clean_end, &scratch.mapping, chunk.len(), thread_id, chunk_id)
            } else {
                (clean_start, clean_end)
            };

            scratch.matches.push((raw_start, raw_end, rule_idx));
        }

        if let (Some(start), Some(rule_timings)) = (rule_started_at, rule_timings_ns.as_mut()) {
            rule_timings[rule_idx] = rule_timings[rule_idx].saturating_add(start.elapsed().as_nanos());
        }
    };

    let use_regexset_prefilter = rule_set.is_some() && rules.len() <= MAX_RULES_FOR_REGEXSET_PREFILTER;

    if use_regexset_prefilter {
        if let Some(prefilter) = rule_set {
            let prefilter_started_at = debug_logging.then(Instant::now);
            let matched_rules = prefilter.matches(clean_chunk);
            for rule_idx in matched_rules.iter() {
                push_matches(rule_idx, &rules[rule_idx].0);
            }
            prefilter_elapsed_us = prefilter_started_at.map_or(0, |start| start.elapsed().as_micros());
        } else {
            for (rule_idx, (regex, _)) in rules.iter().enumerate() {
                push_matches(rule_idx, regex);
            }
        }
    } else {
        for (rule_idx, (regex, _)) in rules.iter().enumerate() {
            push_matches(rule_idx, regex);
        }
    }

    if scratch.matches.is_empty() {
        return Cow::Borrowed(chunk);
    }

    // Sort matches by starting position to handle overlaps.
    scratch.matches.sort_unstable_by_key(|&(start, _, _)| start);

    let format_started_at = debug_logging.then(Instant::now);
    let estimated_capacity = chunk
        .len()
        .saturating_add(scratch.matches.len().saturating_mul(reset_color.len().saturating_add(16)));
    scratch.highlighted.clear();
    scratch.highlighted.reserve(estimated_capacity);

    let mut last_index = 0usize;
    let mut accepted_match_count = 0usize;

    for (start, end, rule_idx) in scratch.matches.iter().copied() {
        if last_index > start {
            continue;
        }
        accepted_match_count = accepted_match_count.saturating_add(1);

        scratch.highlighted.push_str(&chunk[last_index..start]);
        scratch.highlighted.push_str(&rules[rule_idx].1);
        scratch.highlighted.push_str(&chunk[start..end]);
        scratch.highlighted.push_str(reset_color);
        last_index = end;
    }

    scratch.highlighted.push_str(&chunk[last_index..]);

    if let Some(thread_id) = thread_id {
        let format_elapsed_us = format_started_at.map_or(0, |start| start.elapsed().as_micros());
        let total_match_elapsed_us: u128 = rule_timings_ns
            .as_ref()
            .map(|timings| timings.iter().copied().sum::<u128>() / 1000)
            .unwrap_or(0);

        let top_rules = rule_timings_ns.as_ref().map(|timings| top_rule_timing_summary(timings, 5)).unwrap_or_default();

        log_debug!("[{:?}] Chunk[{:?}] 1:Raw chunk: {:?}", thread_id, chunk_id, chunk);
        log_debug!("[{:?}] Chunk[{:?}] 2:Clean chunk: {:?}", thread_id, chunk_id, clean_chunk);
        log_debug!("[{:?}] Chunk[{:?}] 3:Matches: {:?}", thread_id, chunk_id, scratch.matches);
        log_debug!(
            "[{:?}] Chunk[{:?}] 4:Accepted matches: {}/{}",
            thread_id,
            chunk_id,
            accepted_match_count,
            scratch.matches.len()
        );
        log_debug!("[{:?}] Chunk[{:?}] 5:Highlighted chunk: {:?}", thread_id, chunk_id, scratch.highlighted);
        log_debug!(
            "[{:?}] Chunk[{:?}] 6:Timings build={}us prefilter={}us match={}us format={}us prefilter_used={} top_rules={}",
            thread_id,
            chunk_id,
            build_elapsed_us,
            prefilter_elapsed_us,
            total_match_elapsed_us,
            format_elapsed_us,
            use_regexset_prefilter,
            top_rules
        );
    }

    Cow::Borrowed(&scratch.highlighted)
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
            log_debug!(
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
            log_debug!(
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

fn build_clean_chunk_no_ansi(raw: &str, clean_chunk: &mut String) {
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

/// Build a mapping of the original string to a cleaned version with ANSI
/// sequences and newlines removed and return both in reusable buffers.
fn build_index_mapping(raw: &str, clean_chunk: &mut String, mapping: &mut Vec<usize>) {
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

fn top_rule_timing_summary(rule_timings_ns: &[u128], limit: usize) -> String {
    let mut indexed: Vec<(usize, u128)> = rule_timings_ns.iter().copied().enumerate().filter(|(_, elapsed_ns)| *elapsed_ns > 0).collect();
    indexed.sort_unstable_by(|left, right| right.1.cmp(&left.1));

    indexed
        .into_iter()
        .take(limit)
        .map(|(idx, elapsed_ns)| format!("r{}={}us", idx, elapsed_ns / 1000))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::{HighlightScratch, process_chunk_with_scratch};
    use regex::{Regex, RegexSet};

    fn process_chunk_once(chunk: &str, chunk_id: i32, rules: &[(Regex, String)], rule_set: Option<&RegexSet>, reset_color: &str) -> String {
        let mut scratch = HighlightScratch::default();
        process_chunk_with_scratch(chunk, chunk_id, rules, rule_set, reset_color, &mut scratch).into_owned()
    }

    #[test]
    fn highlights_text_when_match_exists_inside_ansi_sequences() {
        let rules = vec![(Regex::new("error").expect("regex"), "<red>".to_string())];
        let chunk = "\x1b[31merror\x1b[0m".to_string();

        let output = process_chunk_once(&chunk, 0, &rules, None, "</red>");
        assert!(output.contains("<red>error"));
        assert!(output.ends_with("</red>"));
    }

    #[test]
    fn keeps_first_match_when_ranges_overlap() {
        let rules = vec![
            (Regex::new("ab").expect("regex"), "<a>".to_string()),
            (Regex::new("abc").expect("regex"), "<b>".to_string()),
        ];

        let output = process_chunk_once("abc", 1, &rules, None, "</>");
        assert_eq!(output, "<a>ab</>c");
    }

    #[test]
    fn maps_newlines_as_spaces_for_matching_but_preserves_raw_text() {
        let rules = vec![(Regex::new("a b").expect("regex"), "<x>".to_string())];

        let output = process_chunk_once("a\nb", 2, &rules, None, "</x>");
        assert_eq!(output, "<x>a\nb</x>");
    }

    #[test]
    fn returns_original_chunk_when_no_rules_exist() {
        let output = process_chunk_once("plain text", 3, &[], None, "</>");
        assert_eq!(output, "plain text");
    }

    #[test]
    fn prefilter_rule_set_matches_same_output_as_full_scan() {
        let rules = vec![
            (Regex::new("error").expect("regex"), "<r>".to_string()),
            (Regex::new("warn").expect("regex"), "<y>".to_string()),
            (Regex::new("ok").expect("regex"), "<g>".to_string()),
        ];
        let patterns: Vec<&str> = rules.iter().map(|(regex, _)| regex.as_str()).collect();
        let rule_set = RegexSet::new(patterns).expect("regex set");
        let chunk = "warn and error and ok";

        let with_prefilter = process_chunk_once(chunk, 4, &rules, Some(&rule_set), "</>");
        let without_prefilter = process_chunk_once(chunk, 4, &rules, None, "</>");
        assert_eq!(with_prefilter, without_prefilter);
    }

    #[test]
    fn scratch_path_matches_single_shot_output_for_plain_text() {
        let rules = vec![(Regex::new("status").expect("regex"), "<c>".to_string())];
        let chunk = "status ok".to_string();

        let single_shot = process_chunk_once(&chunk, 5, &rules, None, "</c>");

        let mut scratch = HighlightScratch::default();
        let from_scratch = process_chunk_with_scratch(&chunk, 5, &rules, None, "</c>", &mut scratch).into_owned();

        assert_eq!(single_shot, from_scratch);
    }

    #[test]
    fn scratch_path_matches_single_shot_output_for_ansi_text() {
        let rules = vec![(Regex::new("error").expect("regex"), "<e>".to_string())];
        let chunk = "\x1b[31merror\x1b[0m happened".to_string();

        let single_shot = process_chunk_once(&chunk, 6, &rules, None, "</e>");

        let mut scratch = HighlightScratch::default();
        let from_scratch = process_chunk_with_scratch(&chunk, 6, &rules, None, "</e>", &mut scratch).into_owned();

        assert_eq!(single_shot, from_scratch);
    }
}
