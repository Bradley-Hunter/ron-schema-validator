/*************************
 * Author: Bradley Hunter
 */

/// Schema parser — converts `.ronschema` source text into a [`Schema`] AST.
pub mod parser;

use std::collections::{HashMap, HashSet};

use crate::Spanned;
use crate::ron::RonValue;

/// A named enum with a closed set of variants, optionally carrying associated data.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    /// The enum name (e.g., `"CardType"`).
    pub name: String,
    /// Variant names mapped to their optional associated data type.
    /// `None` means a unit variant (bare identifier), `Some(type)` means it carries data.
    pub variants: HashMap<String, Option<SchemaType>>,
}

/// A type descriptor representing the expected type of a field value.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaType {
    /// A quoted string.
    String,
    /// A whole number (i64).
    Integer,
    /// A floating-point number (f64).
    Float,
    /// A boolean (`true` or `false`).
    Bool,
    /// An optional value — matches `Some(value)` or `None`.
    Option(Box<SchemaType>),
    /// A homogeneous list — matches `[value, value, ...]`.
    List(Box<SchemaType>),
    /// A reference to a named enum definition.
    EnumRef(String),
    /// A reference to a named type alias.
    AliasRef(String),
    /// A map with typed keys and values — matches `{ key: value, ... }`.
    Map(Box<SchemaType>, Box<SchemaType>),
    /// A positional tuple — matches `(value1, value2, ...)`.
    Tuple(Vec<SchemaType>),
    /// An inline nested struct — matches `(field: value, ...)`.
    Struct(StructDef),
}

/// A comparison operator for `@require` constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
}

/// An annotation on a single field (e.g., `@range`, `@min_length`).
#[derive(Debug, Clone, PartialEq)]
pub enum FieldAnnotation {
    /// `@range(min, max)` — numeric bounds for Integer or Float fields.
    Range(f64, f64),
    /// `@min_length(n)` — minimum length for String or List fields.
    MinLength(usize),
    /// `@max_length(n)` — maximum length for String or List fields.
    MaxLength(usize),
    /// `@pattern("regex")` — regex pattern for String fields (requires `regex` feature).
    Pattern(String),
}

/// An annotation on a struct (e.g., `@require`).
#[derive(Debug, Clone, PartialEq)]
pub struct StructAnnotation {
    /// The left-hand field name.
    pub left: String,
    /// The comparison operator.
    pub op: CompareOp,
    /// The right-hand field name.
    pub right: String,
}

/// A single field definition within a struct.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    /// The field name with source location.
    pub name: Spanned<String>,
    /// The expected type for this field's value, with source location.
    pub type_: Spanned<SchemaType>,
    /// An optional default value. Fields with defaults are not required in data.
    pub default: Option<Spanned<RonValue>>,
    /// Validation annotations attached to this field.
    pub annotations: Vec<Spanned<FieldAnnotation>>,
}

/// A struct definition containing an ordered list of field definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    /// Ordered list of fields. Uses `Vec` to preserve declaration order for error messages.
    pub fields: Vec<FieldDef>,
    /// Struct-level `@require` constraints.
    pub annotations: Vec<Spanned<StructAnnotation>>,
}

/// The top-level schema produced by parsing a `.ronschema` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    /// The root struct definition.
    pub root: StructDef,
    /// Named enum definitions, keyed by name for O(1) lookup during validation.
    pub enums: HashMap<String, EnumDef>,
    /// Type aliases, keyed by name. Stored as-is (not expanded) for better error messages.
    pub aliases: HashMap<String, Spanned<SchemaType>>,
    /// Import paths declared at the top of the schema file, before resolution.
    pub imports: Vec<Spanned<String>>,
}