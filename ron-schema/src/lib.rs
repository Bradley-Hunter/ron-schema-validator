//! Schema definition and validation for RON (Rusty Object Notation) files.

/// Source location tracking types for mapping parsed values back to their origin in source text.
pub mod span;
/// Schema AST types representing a parsed `.ronschema` file.
pub mod schema;
/// RON data value types representing a parsed `.ron` file.
pub mod ron;
/// Validation logic for checking RON data against a schema.
pub mod validate;
/// Error types for schema parsing, RON parsing, and validation.
pub mod error;
/// Source line extraction for rendering error diagnostics.
pub mod diagnostic;
/// Import resolution for schema composition.
pub mod resolve;

// Re-exports — these are the public API
pub use span::{Position, Span, Spanned};
pub use schema::{Schema, StructDef, FieldDef, SchemaType, EnumDef};
pub use schema::parser::parse_schema;
pub use ron::{RonValue, RonStruct};
pub use ron::parser::parse_ron;
pub use validate::validate;
pub use resolve::{SchemaResolver, resolve_imports};
pub use error::{ValidationError, ErrorKind, ValidationResult, Warning, WarningKind, SchemaParseError, SchemaErrorKind, RonParseError, RonErrorKind};
pub use diagnostic::{extract_source_line, SourceLine};