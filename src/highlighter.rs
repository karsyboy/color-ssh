/*
TODO:
    - Split the highlighter code into struct engine and highlighter?
    - Investigate the use of .lines() and .split() for chunk processing.
        - Note: this may make it easier to just regex match per line. May also need a buffer for this???
*/
mod errors;

pub use errors::HighlightError;

use crate::log_debug;
use regex::Regex;
use std::thread;

/// Processes a chunk of text by applying syntax highlighting based on the provided rules.
///
/// - `chunk`: The input text.
/// - `rules`: A slice of tuple where each tuple is `(Regex, ANSI color String)`.
/// - `reset_color`: ANSI escape sequence to reset the color.
///
/// Returns the processed text with ANSI color sequences applied.
pub fn process_chunk(chunk: String, chunk_id: i32, rules: &[(Regex, String)], reset_color: &str) -> String {
    // If debugging, log the raw chunk
    let thread_id = thread::current().id();

    // Clean up the chunk and build the index mapping
    let (clean_chunk, mapping) = build_index_mapping(&chunk);

    let mut matches: Vec<(usize, usize, String, String)> = Vec::new();
    // Find all matches in the chunk using the provided regex rules
    for (regex, color) in rules {
        for mat in regex.find_iter(&clean_chunk) {
            let clean_start = mat.start();
            let clean_end = mat.end();

            // Ensure clean_start and clean_end are within bounds before using them
            let raw_start = if clean_start < mapping.len() {
                mapping[clean_start]
            } else {
                0 // Fallback to 0 if clean_start is out of bounds
            };

            let raw_end = if clean_end < mapping.len() {
                mapping[clean_end]
            } else {
                chunk.len() // Fallback to the full length of the chunk if clean_end is out of bounds
            };

            // Extract the matched text and store it with the color
            let matched_text = chunk[raw_start..raw_end].to_owned();
            matches.push((raw_start, raw_end, matched_text, color.clone()));
        }
    }

    // Filter the matches to avoid overlap (keep only the most specific matches)
    matches.sort_by_key(|&(start, _, _, _)| start);
    let mut filtered_matches = matches.clone();

    // Sort the matches by their starting position in the raw chunk
    filtered_matches.sort_by_key(|&(start, _, _, _)| start);

    // Apply the color formatting to the chunk based on the matches
    let mut highlighted = String::with_capacity(chunk.len());
    let mut last_index = 0;
    for (start, end, matched_text, color) in filtered_matches.clone() {
        // Append the text between the last match and the current match
        if last_index > start {
            continue; // Skip if the last index is already at or beyond the start
        }
        highlighted.push_str(&chunk[last_index..start]);
        // Append the matched text with color formatting
        highlighted.push_str(&format!("{}{}{}", color, matched_text, reset_color));
        last_index = end;
    }

    // Append the remaining text after the last match
    highlighted.push_str(&chunk[last_index..]);

    // If debugging, log clean chunk and the matches
    log_debug!("[{:?}] Chunk[{:?}] 1:Raw chunk: {:?}", thread_id, chunk_id, chunk);
    log_debug!("[{:?}] Chunk[{:?}] 2:Clean chunk: {:?}", thread_id, chunk_id, clean_chunk);
    log_debug!("[{:?}] Chunk[{:?}] 3:Matches: {:?}", thread_id, chunk_id, matches);
    log_debug!("[{:?}] Chunk[{:?}] 4:Filtered matches: {:?}", thread_id, chunk_id, filtered_matches);
    log_debug!("[{:?}] Chunk[{:?}] 5:Highlighted chunk: {:?}", thread_id, chunk_id, highlighted);

    highlighted
}

/// Build a mapping of the original string to a cleaned version with newlines replaced by spaces.
///
///  - `raw`: The original string with newlines
///
/// Returns a tuple containing the cleaned string and a vector of indices mapping the cleaned string back to the original string.
fn build_index_mapping(raw: &str) -> (String, Vec<usize>) {
    let mut clean = String::with_capacity(raw.len());
    let mut mapping = Vec::with_capacity(raw.len());

    let mut raw_idx = 0;
    for ch in raw.chars() {
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
