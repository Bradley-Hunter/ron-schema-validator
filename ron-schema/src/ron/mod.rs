/*************************
 * Author: Bradley Hunter
 */

use crate::{Span, Spanned};
/// RON data parser — converts `.ron` source text into a spanned [`RonValue`] tree.
pub mod parser;

/// A parsed RON data value, preserving bare identifiers for enum validation.
#[derive(Debug, Clone, PartialEq)]
pub enum RonValue {
    /// A quoted string (e.g., `"Ashborn Hound"`).
    String(String),
    /// A whole number (e.g., `42`, `-1`).
    Integer(i64),
    /// A floating-point number (e.g., `3.14`, `1.0`).
    Float(f64),
    /// A boolean (`true` or `false`).
    Bool(bool),
    /// `Some(value)` or `None`. The inner value carries its own span for precise error reporting.
    Option(Option<Box<Spanned<RonValue>>>),
    /// A bare identifier (e.g., `Creature`, `Sentinels`). Preserved for enum variant validation.
    Identifier(String),
    /// A list of values (e.g., `[Creature, Trap]`). Each element carries its own span.
    List(Vec<Spanned<RonValue>>),
    /// A struct with named fields (e.g., `(name: "foo", age: 5)`).
    Struct(RonStruct),
}

/// A parsed RON struct containing ordered field name-value pairs.
#[derive(Debug, Clone, PartialEq)]
pub struct RonStruct {
    /// Field name-value pairs in declaration order. Both names and values carry spans.
    pub fields: Vec<(Spanned<String>, Spanned<RonValue>)>,
    /// Source location of the closing `)`, used as the anchor for missing field errors.
    pub close_span: Span,
}