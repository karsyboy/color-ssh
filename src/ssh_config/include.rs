//! Include expansion helpers for SSH config parsing.

use super::path::expand_tilde;
use std::path::{Path, PathBuf};

pub(super) fn resolve_include_pattern(pattern: &str, base_dir: &Path) -> String {
    let expanded = expand_tilde(pattern);
    let expanded_path = PathBuf::from(&expanded);
    if expanded_path.is_absolute() {
        expanded
    } else {
        base_dir.join(expanded_path).to_string_lossy().to_string()
    }
}

pub(super) fn expand_include_pattern(pattern: &str) -> Vec<PathBuf> {
    let path = PathBuf::from(pattern);

    if !pattern.contains('*') && !pattern.contains('?') {
        if path.is_file() {
            return vec![path];
        }
        return Vec::new();
    }

    let parent = path.parent().unwrap_or(Path::new("."));
    let filename_pattern = path.file_name().and_then(|segment| segment.to_str()).unwrap_or("*");

    let mut matched_paths = Vec::new();
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file() {
                continue;
            }

            if let Ok(file_name) = entry.file_name().into_string()
                && matches_pattern(&file_name, filename_pattern)
            {
                matched_paths.push(entry.path());
            }
        }
    }

    matched_paths.sort_by(|left_path, right_path| left_path.file_name().cmp(&right_path.file_name()));
    matched_paths
}

fn matches_pattern(text: &str, pattern: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    let mut pattern_idx = 0;
    let mut text_idx = 0;

    while pattern_idx < pattern_chars.len() && text_idx < text_chars.len() {
        match pattern_chars[pattern_idx] {
            '*' => {
                if pattern_idx == pattern_chars.len() - 1 {
                    return true;
                }

                pattern_idx += 1;
                while text_idx < text_chars.len() {
                    if matches_pattern(
                        &text_chars[text_idx..].iter().collect::<String>(),
                        &pattern_chars[pattern_idx..].iter().collect::<String>(),
                    ) {
                        return true;
                    }
                    text_idx += 1;
                }
                return false;
            }
            '?' => {
                text_idx += 1;
                pattern_idx += 1;
            }
            pattern_char => {
                if text_chars[text_idx] != pattern_char {
                    return false;
                }
                text_idx += 1;
                pattern_idx += 1;
            }
        }
    }

    pattern_idx == pattern_chars.len() && text_idx == text_chars.len()
}

#[cfg(test)]
mod tests {
    use super::matches_pattern;

    #[test]
    fn matches_pattern_supports_star_and_question() {
        assert!(matches_pattern("abc.conf", "*.conf"));
        assert!(matches_pattern("a1.conf", "a?.conf"));
        assert!(!matches_pattern("abc.conf", "a?.conf"));
        assert!(!matches_pattern("abc.txt", "*.conf"));
    }
}
