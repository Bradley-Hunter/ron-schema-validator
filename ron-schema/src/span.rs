/*************************
 * Author: Bradley Hunter
 */


/// A single point in source text, tracking both byte offset and human-readable coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    /// Byte offset from the start of the source string (0-based).
    pub offset: usize,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (1-based, byte-based).
    pub column: usize,
}

/// A range in source text, defined by a start and end [`Position`].
///
/// Uses Rust's standard range convention: start is inclusive, end is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Start of the spanned region (inclusive).
    pub start: Position,
    /// End of the spanned region (exclusive).
    pub end: Position,
}

/// A value paired with the [`Span`] indicating where it appeared in the source text.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    /// The wrapped value.
    pub value: T,
    /// Source location of this value.
    pub span: Span,
}