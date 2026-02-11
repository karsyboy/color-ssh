//! Syntax highlighting engine
//!
//! Applies ANSI color codes to text based on regex pattern matching.
//! Handles:
//! - Regex pattern compilation and caching
//! - Match detection and overlap resolution
//! - ANSI escape sequence application
//! - Newline character handling for regex matching

mod errors;

pub use errors::HighlightError;

use crate::log_debug;
use once_cell::sync::Lazy;
use regex::Regex;
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

/// Processes a chunk of text by applying syntax highlighting based on the provided rules.
///
/// This function:
/// 1. Cleans the chunk by replacing newlines with spaces for regex matching
/// 2. Applies all regex patterns to find matches
/// 3. Resolves overlapping matches (first match wins)
/// 4. Applies ANSI color codes to matched segments
///
/// # Arguments
/// * `chunk` - The input text to highlight
/// * `chunk_id` - Unique identifier for this chunk (used in debug logging)
/// * `rules` - Slice of tuples containing (Regex pattern, ANSI color code)
/// * `reset_color` - ANSI escape sequence to reset color after highlights
///
/// # Returns
/// The processed text with ANSI color sequences applied
///
/// # Performance Notes
/// - All regex patterns are applied to the entire chunk
/// - For large outputs, consider processing line-by-line
/// - Regex compilation is done at config load time, not here
pub fn process_chunk(chunk: String, chunk_id: i32, rules: &[(Regex, String)], reset_color: &str) -> String {
    let thread_id = thread::current().id();

    // Clean up the chunk and build the index mapping
    // This replaces newlines with spaces so regex patterns can match across lines
    let (clean_chunk, mapping) = build_index_mapping(&chunk);

    let mut matches: Vec<(usize, usize, String, String)> = Vec::new();

    // Find all matches in the chunk using the provided regex rules
    for (regex, color) in rules {
        for mat in regex.find_iter(&clean_chunk) {
            let clean_start = mat.start();
            let clean_end = mat.end();

            // Map clean indices back to raw chunk indices
            // Ensure indices are within bounds before using them
            let raw_start = if clean_start < mapping.len() {
                mapping[clean_start]
            } else {
                log_debug!(
                    "[{:?}] Chunk[{:?}] Index mapping fallback: clean_start {} >= mapping.len() {}",
                    thread_id,
                    chunk_id,
                    clean_start,
                    mapping.len()
                );
                0 // Fallback to 0 if clean_start is out of bounds
            };

            let raw_end = if clean_end < mapping.len() {
                mapping[clean_end]
            } else {
                log_debug!(
                    "[{:?}] Chunk[{:?}] Index mapping fallback: clean_end {} >= mapping.len() {}",
                    thread_id,
                    chunk_id,
                    clean_end,
                    mapping.len()
                );
                chunk.len() // Fallback to the full length of the chunk if clean_end is out of bounds
            };

            // Extract the matched text and store it with the color
            let matched_text = chunk[raw_start..raw_end].to_owned();
            matches.push((raw_start, raw_end, matched_text, color.clone()));
        }
    }

    // Sort matches by starting position to handle overlaps
    matches.sort_by_key(|&(start, _, _, _)| start);

    // Apply the color formatting to the chunk based on the matches
    // Reserve extra capacity for ANSI escape sequences (approximately 20 bytes per match)
    let estimated_capacity = chunk.len() + (matches.len() * 20);
    let mut highlighted = String::with_capacity(estimated_capacity);
    let mut last_index = 0;

    for (start, end, matched_text, color) in matches.clone() {
        // Skip overlapping matches (first match wins)
        if last_index > start {
            continue;
        }

        // Append the text between the last match and the current match (unhighlighted)
        highlighted.push_str(&chunk[last_index..start]);

        // Append the matched text with color formatting
        highlighted.push_str(&format!("{}{}{}", color, matched_text, reset_color));
        last_index = end;
    }

    // Append the remaining text after the last match
    highlighted.push_str(&chunk[last_index..]);

    // Debug logging for detailed highlighting analysis
    log_debug!("[{:?}] Chunk[{:?}] 1:Raw chunk: {:?}", thread_id, chunk_id, chunk);
    log_debug!("[{:?}] Chunk[{:?}] 2:Clean chunk: {:?}", thread_id, chunk_id, clean_chunk);
    log_debug!("[{:?}] Chunk[{:?}] 3:Matches: {:?}", thread_id, chunk_id, matches);
    log_debug!("[{:?}] Chunk[{:?}] 4:Filtered matches: {:?}", thread_id, chunk_id, matches);
    log_debug!("[{:?}] Chunk[{:?}] 5:Highlighted chunk: {:?}", thread_id, chunk_id, highlighted);

    highlighted
}

/// Build a mapping of the original string to a cleaned version with ANSI sequences and newlines removed.
///
/// This is necessary because:
/// 1. ANSI escape sequences (colors, window title commands, etc.) interfere with pattern matching
/// 2. Regex patterns typically don't match across newline boundaries
///
/// By removing these from the clean version, we can apply regex patterns correctly while
/// maintaining the ability to map matches back to the original string positions.
///
/// # Arguments
/// * `raw` - The original string with ANSI sequences and newlines
///
/// # Returns
/// A tuple containing:
/// * The cleaned string (ANSI sequences removed, newlines replaced with spaces)
/// * A vector mapping each byte position in the cleaned string to its byte position in the original string
fn build_index_mapping(raw: &str) -> (String, Vec<usize>) {
    // First, identify all ANSI escape sequence positions in the raw string
    let ansi_ranges: Vec<(usize, usize)> = ANSI_ESCAPE_REGEX.find_iter(raw).map(|m| (m.start(), m.end())).collect();

    let mut clean = String::with_capacity(raw.len());
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
            let clean_byte_pos = clean.len();

            // Replace newlines and carriage returns with spaces for regex matching
            if ch == '\n' || ch == '\r' {
                clean.push(' ');
            } else {
                clean.push(ch);
            }

            // Map each byte position in the clean string to the corresponding byte position in raw
            // For multi-byte UTF-8 characters, all bytes of the clean char map to the start of the raw char
            let clean_char_len = clean.len() - clean_byte_pos;
            for _ in 0..clean_char_len {
                mapping.push(raw_idx);
            }
        }

        raw_idx += ch_len;
    }

    mapping.push(raw_idx); // Final entry for end-of-string matching
    (clean, mapping)
}
