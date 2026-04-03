//! Schema definition and validation for RON (Rusty Object Notation) files.

/// Source location tracking types for mapping parsed values back to their origin in source text.
pub mod span;
/// Schema AST types representing a parsed `.ronschema` file.
pub mod schema;
/// RON data value types representing a parsed `.ron` file.
pub mod ron;
// pub mod validate;
/// Error types for schema parsing, RON parsing, and validation.
pub mod error;
/// Source line extraction for rendering error diagnostics.
pub mod diagnostic;

// Re-exports — these are the public API
pub use span::{Position, Span, Spanned};
pub use schema::{Schema, StructDef, FieldDef, SchemaType, EnumDef};
// pub use schema::parser::parse_schema;
pub use ron::{RonValue, RonStruct};
// pub use ron::parser::parse_ron;
// pub use validate::validate;
pub use error::{ValidationError, ErrorKind, SchemaParseError, SchemaErrorKind, RonParseError, RonErrorKind};
pub use diagnostic::{extract_source_line, SourceLine};