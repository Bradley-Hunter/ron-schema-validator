/*************************
 * Author: Bradley Hunter
 */

use crate::span::{Span};



/// Extracted source line with highlight positions for rendering error underlines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLine {
    /// 1-based line number.
    pub line_number: usize,
    /// The full source line with trailing newline trimmed.
    pub line_text: String,
    /// Column where the underline starts (0-based, for spacing).
    pub highlight_start: usize,
    /// Column where the underline ends (0-based).
    pub highlight_end: usize,
}

/// Extracts a source line and highlight positions from the original source text for a given span.
///
/// For spans that cross multiple lines, the highlight extends to the end of the first line.
pub fn extract_source_line(source: &str, span: Span) -> SourceLine {
    let mut line_start = span.start.offset;
    while line_start > 0 && source.as_bytes()[line_start - 1] != b'\n' {
        line_start -= 1;
    }

    let mut line_end = span.start.offset;
    while line_end < source.len() && source.as_bytes()[line_end] != b'\n' {
        line_end += 1;
    }

    let line_text = source[line_start..line_end].to_string();

    let highlight_start = span.start.offset - line_start;
    let highlight_end = if span.end.line == span.start.line {
        span.end.offset - line_start
    } else {
        line_end - line_start
    };

    return SourceLine { line_number: span.start.line, line_text, highlight_start, highlight_end };
    
}