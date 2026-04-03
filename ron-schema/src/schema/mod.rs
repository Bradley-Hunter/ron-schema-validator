/*************************
 * Author: Bradley Hunter
 */

use std::collections::{HashMap, HashSet};

use crate::Spanned;

/// A named enum with a closed set of unit variants.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    /// The enum name (e.g., `"CardType"`).
    pub name: String,
    /// The set of valid variant names.
    pub variants: HashSet<String>,
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
    /// An inline nested struct — matches `(field: value, ...)`.
    Struct(StructDef),
}

/// A single field definition within a struct.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    /// The field name with source location.
    pub name: Spanned<String>,
    /// The expected type for this field's value, with source location.
    pub type_: Spanned<SchemaType>,
}

/// A struct definition containing an ordered list of field definitions.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    /// Ordered list of fields. Uses `Vec` to preserve declaration order for error messages.
    pub fields: Vec<FieldDef>,
}

/// The top-level schema produced by parsing a `.ronschema` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    /// The root struct definition.
    pub root: StructDef,
    /// Named enum definitions, keyed by name for O(1) lookup during validation.
    pub enums: HashMap<String, EnumDef>,
}