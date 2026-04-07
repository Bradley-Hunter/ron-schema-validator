/*************************
 * Author: Bradley Hunter
 */

use crate::span::Span;

/// Specific kinds of errors that can occur when parsing a `.ronschema` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaErrorKind {
    /// A type name was not recognized (e.g., `"Intgeer"` instead of `"Integer"`).
    UnknownType {
        /// The unrecognized type name.
        name: String,
        /// A suggested correction, if one is close enough.
        suggestion: Option<String>,
    },
    /// A type references an enum that is never defined in the schema.
    UnresolvedEnum {
        /// The referenced enum name.
        name: String,
    },
    /// The same enum name is defined more than once.
    DuplicateEnum {
        /// The duplicated enum name.
        name: String,
    },
    /// The same variant appears more than once within an enum definition.
    DuplicateVariant {
        /// The enum containing the duplicate.
        enum_name: String,
        /// The duplicated variant name.
        variant: String,
    },
    /// The same field name appears more than once within a struct definition.
    DuplicateField {
        /// The duplicated field name.
        field_name: String,
    },
    /// The same type alias name is defined more than once.
    DuplicateAlias {
        /// The duplicated alias name.
        name: String,
    },
    /// A type alias references itself, directly or indirectly.
    RecursiveAlias {
        /// The alias name involved in the cycle.
        name: String,
    },
    /// A `PascalCase` name could not be resolved to an enum or type alias.
    UnresolvedType {
        /// The unresolved type name.
        name: String,
    },
    /// A map key type is not valid (must be `String`, `Integer`, or an enum type).
    InvalidMapKeyType {
        /// A description of the invalid key type.
        found: String,
    },
    /// A syntax error — the parser encountered a token it did not expect.
    UnexpectedToken {
        /// What the parser expected at this position.
        expected: String,
        /// What was actually found.
        found: String,
    },
}

/// Specific kinds of errors that can occur when parsing a `.ron` data file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RonErrorKind {
    /// A syntax error — the parser encountered a token it did not expect.
    UnexpectedToken {
        /// What the parser expected at this position.
        expected: String,
        /// What was actually found.
        found: String,
    },
    /// A string literal is missing its closing quote.
    UnterminatedString,
    /// The same field name appears more than once within a struct.
    DuplicateField {
        /// The duplicated field name.
        field_name: String,
    },
    /// A numeric literal could not be parsed as a valid number.
    InvalidNumber {
        /// The text that failed to parse.
        text: String,
    },
}

/// Specific kinds of validation errors produced when RON data does not match a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// A required field defined in the schema is absent from the data.
    MissingField {
        /// The name of the missing field.
        field_name: String,
    },
    /// The data contains a field not defined in the schema.
    UnknownField {
        /// The name of the unrecognized field.
        field_name: String,
    },
    /// A value exists but has the wrong type.
    TypeMismatch {
        /// The type expected by the schema.
        expected: String,
        /// A description of what was actually found.
        found: String,
    },
    /// A bare identifier does not match any variant of the expected enum.
    InvalidEnumVariant {
        /// The enum the identifier was validated against.
        enum_name: String,
        /// The identifier that was found.
        variant: String,
        /// All valid variants for this enum.
        valid: Vec<String>,
    },
    /// The value inside `Some(...)` has the wrong type.
    InvalidOptionValue {
        /// The type expected by the schema.
        expected: String,
        /// A description of what was actually found.
        found: String,
    },
    /// A list element has the wrong type.
    InvalidListElement {
        /// The 0-based index of the offending element.
        index: usize,
        /// The type expected by the schema.
        expected: String,
        /// A description of what was actually found.
        found: String,
    },
    /// Expected a struct `(...)` but found a non-struct value.
    ExpectedStruct {
        /// A description of what was actually found.
        found: String,
    },
    /// Expected a list `[...]` but found a non-list value.
    ExpectedList {
        /// A description of what was actually found.
        found: String,
    },
    /// Expected `Some(...)` or `None` but found something else.
    ExpectedOption {
        /// A description of what was actually found.
        found: String,
    },
    /// Expected a map `{ ... }` but found a non-map value.
    ExpectedMap {
        /// A description of what was actually found.
        found: String,
    },
    /// A map key has the wrong type.
    InvalidMapKey {
        /// A string representation of the key.
        key: String,
        /// The type expected by the schema.
        expected: String,
        /// A description of what was actually found.
        found: String,
    },
    /// A map value has the wrong type.
    InvalidMapValue {
        /// A string representation of the key this value belongs to.
        key: String,
        /// The type expected by the schema.
        expected: String,
        /// A description of what was actually found.
        found: String,
    },
}

/// An error produced when parsing a `.ronschema` file fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaParseError {
    /// Source location where the error occurred.
    pub span: Span,
    /// What went wrong.
    pub kind: SchemaErrorKind,
}

/// An error produced when parsing a `.ron` data file fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RonParseError {
    /// Source location where the error occurred.
    pub span: Span,
    /// What went wrong.
    pub kind: RonErrorKind,
}

/// An error produced when RON data does not conform to a schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Dot/bracket field path to the problematic value (e.g., `"cost.generic"`, `"card_types[0]"`).
    pub path: String,
    /// Source location of the problematic value in the data file.
    pub span: Span,
    /// What went wrong.
    pub kind: ErrorKind,
}