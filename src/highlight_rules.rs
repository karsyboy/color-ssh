//! Shared compiled highlight-rule representation.
//!
//! Config loading compiles regex rules once and stores them in a compact form
//! that renderer overlays can reuse. Interactive session rendering no longer
//! mutates stdout; these rules are presentation inputs only.

use regex::Regex;

#[derive(Debug, Clone)]
pub(crate) struct CompiledHighlightRule {
    pub(crate) regex: Regex,
    pub(crate) ansi_style: String,
}

impl CompiledHighlightRule {
    pub(crate) fn new(regex: Regex, ansi_style: String) -> Self {
        Self { regex, ansi_style }
    }
}
