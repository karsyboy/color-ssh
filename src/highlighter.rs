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
use regex::Regex;
use std::thread;

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
    let filtered_matches = matches.clone();

    // Apply the color formatting to the chunk based on the matches
    // Reserve extra capacity for ANSI escape sequences (approximately 20 bytes per match)
    let estimated_capacity = chunk.len() + (filtered_matches.len() * 20);
    let mut highlighted = String::with_capacity(estimated_capacity);
    let mut last_index = 0;

    for (start, end, matched_text, color) in filtered_matches.clone() {
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
    log_debug!("[{:?}] Chunk[{:?}] 4:Filtered matches: {:?}", thread_id, chunk_id, filtered_matches);
    log_debug!("[{:?}] Chunk[{:?}] 5:Highlighted chunk: {:?}", thread_id, chunk_id, highlighted);

    highlighted
}

/// Build a mapping of the original string to a cleaned version with newlines replaced by spaces.
///
/// This is necessary because regex patterns typically don't match across newline boundaries.
/// By replacing newlines with spaces, we can apply regex patterns to multi-line text while
/// maintaining the ability to map matches back to the original string positions.
///
/// # Arguments
/// * `raw` - The original string with newlines
///
/// # Returns
/// A tuple containing:
/// * The cleaned string (newlines replaced with spaces)
/// * A vector mapping each character position in the cleaned string to its byte position in the original string
fn build_index_mapping(raw: &str) -> (String, Vec<usize>) {
    let mut clean = String::with_capacity(raw.len());
    let mut mapping = Vec::with_capacity(raw.len());

    let mut raw_idx = 0;
    for ch in raw.chars() {
        // Replace newlines and carriage returns with spaces for regex matching
        if ch == '\n' || ch == '\r' {
            clean.push(' ');
        } else {
            clean.push(ch);
        }
        mapping.push(raw_idx);
        raw_idx += ch.len_utf8(); // Keep track of the character's byte length
    }
    (clean, mapping)
}
