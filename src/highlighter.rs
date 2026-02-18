mod errors;

pub use errors::HighlightError;

use crate::{debug_enabled, log_debug};
use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};
use std::thread;

// Compiled regex for stripping ANSI escape sequences before pattern matching
static ANSI_ESCAPE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?x)
        \x1B\[[\x30-\x3F]*[\x20-\x2F]*[\x40-\x7E]    # CSI: ESC [ params intermediates final
        |\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)           # OSC: ESC ] ... (BEL or ESC \)
        |\x1B[PX^_].*?\x1B\\                         # DCS/SOS/PM/APC: ESC P/X/^/_ ... ESC \
        |\x1B.                                       # Other ESC sequences (2 bytes)
        |\x1B                                        # Stray ESC character
    ",
    )
    .unwrap()
});

/// Processes a chunk of text by applying syntax highlighting based on the provided rules and returns the highlighted string.
pub fn process_chunk(chunk: String, chunk_id: i32, rules: &[(Regex, String)], rule_set: Option<&RegexSet>, reset_color: &str) -> String {
    if chunk.is_empty() || rules.is_empty() {
        return chunk;
    }

    let debug_logging = debug_enabled!();
    let thread_id = debug_logging.then(|| thread::current().id());

    // Clean up the chunk and build the index mapping
    let (clean_chunk, mapping) = build_index_mapping(&chunk);
    if clean_chunk.is_empty() {
        return chunk;
    }

    let mut matches: Vec<(usize, usize, usize)> = Vec::new();
    let mut push_matches = |rule_idx: usize, regex: &Regex| {
        for mat in regex.find_iter(&clean_chunk) {
            let clean_start = mat.start();
            let clean_end = mat.end();

            // Map clean indices back to raw chunk indices.
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
                chunk.len()
            };

            matches.push((raw_start, raw_end, rule_idx));
        }
    };

    if let Some(prefilter) = rule_set {
        for rule_idx in prefilter.matches(&clean_chunk).iter() {
            let regex = &rules[rule_idx].0;
            push_matches(rule_idx, regex);
        }
    } else {
        for (rule_idx, (regex, _)) in rules.iter().enumerate() {
            push_matches(rule_idx, regex);
        }
    }

    // Sort matches by starting position to handle overlaps
    matches.sort_by_key(|&(start, _, _)| start);

    // Apply the color formatting to the chunk based on the matches
    let estimated_capacity = chunk.len() + (matches.len() * (reset_color.len() + 16));
    let mut highlighted = String::with_capacity(estimated_capacity);
    let mut last_index = 0;
    let mut accepted_match_count = 0usize;

    for (start, end, rule_idx) in matches.iter().copied() {
        // Skip overlapping matches the first match wins
        if last_index > start {
            continue;
        }
        accepted_match_count = accepted_match_count.saturating_add(1);

        // Append the text between the last match and the current match
        highlighted.push_str(&chunk[last_index..start]);

        // Append the matched text with color formatting
        highlighted.push_str(&rules[rule_idx].1);
        highlighted.push_str(&chunk[start..end]);
        highlighted.push_str(reset_color);
        last_index = end;
    }

    // Append the remaining text after the last match
    highlighted.push_str(&chunk[last_index..]);

    // Debug logging for detailed highlighting analysis.
    if let Some(thread_id) = thread_id {
        log_debug!("[{:?}] Chunk[{:?}] 1:Raw chunk: {:?}", thread_id, chunk_id, chunk);
        log_debug!("[{:?}] Chunk[{:?}] 2:Clean chunk: {:?}", thread_id, chunk_id, clean_chunk);
        log_debug!("[{:?}] Chunk[{:?}] 3:Matches: {:?}", thread_id, chunk_id, matches);
        log_debug!(
            "[{:?}] Chunk[{:?}] 4:Accepted matches: {}/{}",
            thread_id,
            chunk_id,
            accepted_match_count,
            matches.len()
        );
        log_debug!("[{:?}] Chunk[{:?}] 5:Highlighted chunk: {:?}", thread_id, chunk_id, highlighted);
    }

    highlighted
}

/// Build a mapping of the original string to a cleaned version with ANSI sequences and newlines removed and return both the clean string and mapping.
fn build_index_mapping(raw: &str) -> (String, Vec<usize>) {
    // First, identify all ANSI escape sequence positions in the raw string
    let ansi_ranges: Vec<(usize, usize)> = ANSI_ESCAPE_REGEX
        .find_iter(raw)
        .map(|escape_match| (escape_match.start(), escape_match.end()))
        .collect();

    let mut clean_chunk = String::with_capacity(raw.len());
    let mut mapping = Vec::with_capacity(raw.len());

    let mut raw_idx = 0;
    let mut ansi_iter = ansi_ranges.iter().peekable();

    for ch in raw.chars() {
        let ch_len = ch.len_utf8();

        // Skip characters that are part of ANSI escape sequences
        let mut in_ansi = false;
        while let Some((start, end)) = ansi_iter.peek() {
            if raw_idx >= *end {
                ansi_iter.next();
            } else if raw_idx >= *start && raw_idx < *end {
                in_ansi = true;
                break;
            } else {
                break;
            }
        }

        if !in_ansi {
            // Track the byte position in clean string for each byte of the character
            let clean_byte_pos = clean_chunk.len();

            if ch == '\n' || ch == '\r' {
                clean_chunk.push(' ');
            } else {
                clean_chunk.push(ch);
            }

            // Map each byte position in the clean string to the corresponding byte position in raw
            let clean_char_len = clean_chunk.len() - clean_byte_pos;
            for _ in 0..clean_char_len {
                mapping.push(raw_idx);
            }
        }

        raw_idx += ch_len;
    }

    mapping.push(raw_idx);
    (clean_chunk, mapping)
}

#[cfg(test)]
mod tests {
    use super::process_chunk;
    use regex::{Regex, RegexSet};

    #[test]
    fn highlights_text_when_match_exists_inside_ansi_sequences() {
        let rules = vec![(Regex::new("error").expect("regex"), "<red>".to_string())];
        let chunk = "\x1b[31merror\x1b[0m".to_string();

        let output = process_chunk(chunk, 0, &rules, None, "</red>");
        assert!(output.contains("<red>error"));
        assert!(output.ends_with("</red>"));
    }

    #[test]
    fn keeps_first_match_when_ranges_overlap() {
        let rules = vec![
            (Regex::new("ab").expect("regex"), "<a>".to_string()),
            (Regex::new("abc").expect("regex"), "<b>".to_string()),
        ];

        let output = process_chunk("abc".to_string(), 1, &rules, None, "</>");
        assert_eq!(output, "<a>ab</>c");
    }

    #[test]
    fn maps_newlines_as_spaces_for_matching_but_preserves_raw_text() {
        let rules = vec![(Regex::new("a b").expect("regex"), "<x>".to_string())];

        let output = process_chunk("a\nb".to_string(), 2, &rules, None, "</x>");
        assert_eq!(output, "<x>a\nb</x>");
    }

    #[test]
    fn returns_original_chunk_when_no_rules_exist() {
        let output = process_chunk("plain text".to_string(), 3, &[], None, "</>");
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
        let chunk = "warn and error and ok".to_string();

        let with_prefilter = process_chunk(chunk.clone(), 4, &rules, Some(&rule_set), "</>");
        let without_prefilter = process_chunk(chunk, 4, &rules, None, "</>");
        assert_eq!(with_prefilter, without_prefilter);
    }
}
