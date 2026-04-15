/*************************
 * Author: Bradley Hunter
 */

use std::collections::{HashMap, HashSet};

use crate::error::{ErrorKind, ValidationError};

/// Validates a parsed RON value against a schema.
///
/// Returns all validation errors found — does not stop at the first error.
/// An empty vec means the data is valid.
#[must_use] 
pub fn validate(schema: &Schema, value: &Spanned<RonValue>) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    validate_struct(&schema.root, value, "", &mut errors, &schema.enums, &schema.aliases);
    errors
}
use crate::ron::RonValue;
use crate::schema::{EnumDef, Schema, SchemaType, StructDef};
use crate::span::Spanned;

/// Produces a human-readable description of a RON value for error messages.
fn describe(value: &RonValue) -> String {
    match value {
        RonValue::String(s) => {
            if s.len() > 20 {
                format!("String(\"{}...\")", &s[..20])
            } else {
                format!("String(\"{s}\")")
            }
        }
        RonValue::Integer(n) => format!("Integer({n})"),
        RonValue::Float(f) => format!("Float({f})"),
        RonValue::Bool(b) => format!("Bool({b})"),
        RonValue::Option(_) => "Option".to_string(),
        RonValue::Identifier(s) => format!("Identifier({s})"),
        RonValue::EnumVariant(name, _) => format!("{name}(...)"),
        RonValue::List(_) => "List".to_string(),
        RonValue::Map(_) => "Map".to_string(),
        RonValue::Tuple(_) => "Tuple".to_string(),
        RonValue::Struct(_) => "Struct".to_string(),
    }
}

/// Builds a dot-separated field path for error messages.
///
/// An empty parent means we're at the root, so just return the field name.
/// Otherwise, join with a dot: `"cost"` + `"generic"` → `"cost.generic"`.
fn build_path(parent: &str, field: &str) -> String {
    if parent.is_empty() {
        field.to_string()
    } else {
        format!("{parent}.{field}")
    }
}

/// Validates a single RON value against an expected schema type.
///
/// Matches on the expected type and checks that the actual value conforms.
/// For composite types (Option, List, Struct), recurses into the inner values.
/// Errors are collected into the `errors` vec — validation does not stop at the first error.
#[allow(clippy::too_many_lines)]
fn validate_type(
    expected: &SchemaType,
    actual: &Spanned<RonValue>,
    path: &str,
    errors: &mut Vec<ValidationError>,
    enums: &HashMap<String, EnumDef>,
    aliases: &HashMap<String, Spanned<SchemaType>>,
) {
    match expected {
        // Primitives: check that the value variant matches the schema type.
        SchemaType::String => {
            if !matches!(actual.value, RonValue::String(_)) {
                errors.push(ValidationError {
                    path: path.to_string(),
                    span: actual.span,
                    kind: ErrorKind::TypeMismatch {
                        expected: "String".to_string(),
                        found: describe(&actual.value),
                    },
                });
            }
        }
        SchemaType::Integer => {
            if !matches!(actual.value, RonValue::Integer(_)) {
                errors.push(ValidationError {
                    path: path.to_string(),
                    span: actual.span,
                    kind: ErrorKind::TypeMismatch {
                        expected: "Integer".to_string(),
                        found: describe(&actual.value),
                    },
                });
            }
        }
        SchemaType::Float => {
            if !matches!(actual.value, RonValue::Float(_)) {
                errors.push(ValidationError {
                    path: path.to_string(),
                    span: actual.span,
                    kind: ErrorKind::TypeMismatch {
                        expected: "Float".to_string(),
                        found: describe(&actual.value),
                    },
                });
            }
        }
        SchemaType::Bool => {
            if !matches!(actual.value, RonValue::Bool(_)) {
                errors.push(ValidationError {
                    path: path.to_string(),
                    span: actual.span,
                    kind: ErrorKind::TypeMismatch {
                        expected: "Bool".to_string(),
                        found: describe(&actual.value),
                    },
                });
            }
        }

        // Option: None is always valid. Some(inner) recurses into the inner value.
        // Anything else (bare integer, string, etc.) is an error — must be Some(...) or None.
        SchemaType::Option(inner_type) => match &actual.value {
            RonValue::Option(None) => {}
            RonValue::Option(Some(inner_value)) => {
                validate_type(inner_type, inner_value, path, errors, enums, aliases);
            }
            _ => {
                errors.push(ValidationError {
                    path: path.to_string(),
                    span: actual.span,
                    kind: ErrorKind::ExpectedOption {
                        found: describe(&actual.value),
                    },
                });
            }
        },

        // List: check value is a list, then validate each element against the element type.
        // Path gets bracket notation: "card_types[0]", "card_types[1]", etc.
        SchemaType::List(element_type) => {
            if let RonValue::List(elements) = &actual.value {
                for (index, element) in elements.iter().enumerate() {
                    let element_path = format!("{path}[{index}]");
                    validate_type(element_type, element, &element_path, errors, enums, aliases);
                }
            } else {
                errors.push(ValidationError {
                    path: path.to_string(),
                    span: actual.span,
                    kind: ErrorKind::ExpectedList {
                        found: describe(&actual.value),
                    },
                });
            }
        }

        // EnumRef: value must be a known variant. Unit variants are bare identifiers,
        // data variants are EnumVariant(name, data). The schema defines which variants
        // exist and whether they carry data.
        SchemaType::EnumRef(enum_name) => {
            let enum_def = &enums[enum_name];
            let variant_names: Vec<String> = enum_def.variants.keys().cloned().collect();

            match &actual.value {
                // Bare identifier — must be a known unit variant
                RonValue::Identifier(variant) => {
                    match enum_def.variants.get(variant) {
                        None => {
                            errors.push(ValidationError {
                                path: path.to_string(),
                                span: actual.span,
                                kind: ErrorKind::InvalidEnumVariant {
                                    enum_name: enum_name.clone(),
                                    variant: variant.clone(),
                                    valid: variant_names,
                                },
                            });
                        }
                        Some(Some(_expected_data_type)) => {
                            // Variant exists but expects data — bare identifier is wrong
                            errors.push(ValidationError {
                                path: path.to_string(),
                                span: actual.span,
                                kind: ErrorKind::InvalidVariantData {
                                    enum_name: enum_name.clone(),
                                    variant: variant.clone(),
                                    expected: "data".to_string(),
                                    found: "unit variant".to_string(),
                                },
                            });
                        }
                        Some(None) => {} // Unit variant, matches
                    }
                }
                // Enum variant with data — must be a known data variant, and data must match
                RonValue::EnumVariant(variant, data) => {
                    match enum_def.variants.get(variant) {
                        None => {
                            errors.push(ValidationError {
                                path: path.to_string(),
                                span: actual.span,
                                kind: ErrorKind::InvalidEnumVariant {
                                    enum_name: enum_name.clone(),
                                    variant: variant.clone(),
                                    valid: variant_names,
                                },
                            });
                        }
                        Some(None) => {
                            // Variant exists but is a unit variant — data is unexpected
                            errors.push(ValidationError {
                                path: path.to_string(),
                                span: actual.span,
                                kind: ErrorKind::InvalidVariantData {
                                    enum_name: enum_name.clone(),
                                    variant: variant.clone(),
                                    expected: "unit variant".to_string(),
                                    found: describe(&data.value),
                                },
                            });
                        }
                        Some(Some(expected_data_type)) => {
                            // Validate the associated data
                            validate_type(expected_data_type, data, path, errors, enums, aliases);
                        }
                    }
                }
                // Wrong value type entirely
                _ => {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        span: actual.span,
                        kind: ErrorKind::TypeMismatch {
                            expected: enum_name.clone(),
                            found: describe(&actual.value),
                        },
                    });
                }
            }
        }

        // Map: check value is a map, then validate each key and value.
        SchemaType::Map(key_type, value_type) => {
            if let RonValue::Map(entries) = &actual.value {
                for (key, value) in entries {
                    let key_desc = describe(&key.value);
                    validate_type(key_type, key, path, errors, enums, aliases);
                    let entry_path = format!("{path}[{key_desc}]");
                    validate_type(value_type, value, &entry_path, errors, enums, aliases);
                }
            } else {
                errors.push(ValidationError {
                    path: path.to_string(),
                    span: actual.span,
                    kind: ErrorKind::ExpectedMap {
                        found: describe(&actual.value),
                    },
                });
            }
        }

        // Tuple: check value is a tuple, check element count, validate each element.
        SchemaType::Tuple(element_types) => {
            if let RonValue::Tuple(elements) = &actual.value {
                if elements.len() == element_types.len() {
                    for (index, (expected_type, element)) in element_types.iter().zip(elements).enumerate() {
                        let element_path = format!("{path}.{index}");
                        validate_type(expected_type, element, &element_path, errors, enums, aliases);
                    }
                } else {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        span: actual.span,
                        kind: ErrorKind::TupleLengthMismatch {
                            expected: element_types.len(),
                            found: elements.len(),
                        },
                    });
                }
            } else {
                errors.push(ValidationError {
                    path: path.to_string(),
                    span: actual.span,
                    kind: ErrorKind::ExpectedTuple {
                        found: describe(&actual.value),
                    },
                });
            }
        }

        // AliasRef: look up the alias and validate against the resolved type.
        // Error messages use the alias name (e.g., "expected Cost") not the expanded type.
        SchemaType::AliasRef(alias_name) => {
            if let Some(resolved) = aliases.get(alias_name) {
                validate_type(&resolved.value, actual, path, errors, enums, aliases);
            }
            // If alias doesn't exist, the parser already caught it — unreachable in practice.
        }

        // Nested struct: recurse into validate_struct.
        SchemaType::Struct(struct_def) => {
            validate_struct(struct_def, actual, path, errors, enums, aliases);
        }
    }
}

/// Validates a RON struct against a schema struct definition.
///
/// Three checks:
/// 1. Missing fields — in schema but not in data (points to closing paren)
/// 2. Unknown fields — in data but not in schema (points to field name)
/// 3. Matching fields — present in both, recurse into `validate_type`
fn validate_struct(
    struct_def: &StructDef,
    actual: &Spanned<RonValue>,
    path: &str,
    errors: &mut Vec<ValidationError>,
    enums: &HashMap<String, EnumDef>,
    aliases: &HashMap<String, Spanned<SchemaType>>,
) {
    // Value must be a struct — if not, report and bail (can't check fields of a non-struct)
    let RonValue::Struct(data_struct) = &actual.value else {
        errors.push(ValidationError {
            path: path.to_string(),
            span: actual.span,
            kind: ErrorKind::ExpectedStruct {
                found: describe(&actual.value),
            },
        });
        return;
    };

    // Build a lookup map from data fields for O(1) access by name
    let data_map: HashMap<&str, &Spanned<RonValue>> = data_struct
        .fields
        .iter()
        .map(|(name, value)| (name.value.as_str(), value))
        .collect();

    // Build a set of schema field names for unknown-field detection
    let schema_names: HashSet<&str> = struct_def
        .fields
        .iter()
        .map(|f| f.name.value.as_str())
        .collect();

    // 1. Missing fields: in schema but not in data (skip fields with defaults)
    for field_def in &struct_def.fields {
        if !data_map.contains_key(field_def.name.value.as_str()) && field_def.default.is_none() {
            errors.push(ValidationError {
                path: build_path(path, &field_def.name.value),
                span: data_struct.close_span,
                kind: ErrorKind::MissingField {
                    field_name: field_def.name.value.clone(),
                },
            });
        }
    }

    // 2. Unknown fields: in data but not in schema
    for (name, _value) in &data_struct.fields {
        if !schema_names.contains(name.value.as_str()) {
            errors.push(ValidationError {
                path: build_path(path, &name.value),
                span: name.span,
                kind: ErrorKind::UnknownField {
                    field_name: name.value.clone(),
                },
            });
        }
    }

    // 3. Matching fields: validate each against its expected type
    for field_def in &struct_def.fields {
        if let Some(data_value) = data_map.get(field_def.name.value.as_str()) {
            let field_path = build_path(path, &field_def.name.value);
            validate_type(&field_def.type_.value, data_value, &field_path, errors, enums, aliases);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parser::parse_schema;
    use crate::ron::parser::parse_ron;

    /// Parses both a schema and data string, runs validation, returns errors.
    fn validate_str(schema_src: &str, data_src: &str) -> Vec<ValidationError> {
        let schema = parse_schema(schema_src).expect("test schema should parse");
        let data = parse_ron(data_src).expect("test data should parse");
        validate(&schema, &data)
    }

    // ========================================================
    // describe() tests
    // ========================================================

    // Describes a string value.
    #[test]
    fn describe_string() {
        assert_eq!(describe(&RonValue::String("hi".to_string())), "String(\"hi\")");
    }

    // Truncates long strings at 20 characters.
    #[test]
    fn describe_string_truncated() {
        let long = "a".repeat(30);
        let desc = describe(&RonValue::String(long));
        assert!(desc.contains("..."));
    }

    // Describes an integer.
    #[test]
    fn describe_integer() {
        assert_eq!(describe(&RonValue::Integer(42)), "Integer(42)");
    }

    // Describes a float.
    #[test]
    fn describe_float() {
        assert_eq!(describe(&RonValue::Float(3.14)), "Float(3.14)");
    }

    // Describes a bool.
    #[test]
    fn describe_bool() {
        assert_eq!(describe(&RonValue::Bool(true)), "Bool(true)");
    }

    // Describes an identifier.
    #[test]
    fn describe_identifier() {
        assert_eq!(describe(&RonValue::Identifier("Creature".to_string())), "Identifier(Creature)");
    }

    // ========================================================
    // build_path() tests
    // ========================================================

    // Root-level field has no dot prefix.
    #[test]
    fn build_path_root() {
        assert_eq!(build_path("", "name"), "name");
    }

    // Nested field gets dot notation.
    #[test]
    fn build_path_nested() {
        assert_eq!(build_path("cost", "generic"), "cost.generic");
    }

    // Deeply nested path.
    #[test]
    fn build_path_deep() {
        assert_eq!(build_path("a.b", "c"), "a.b.c");
    }

    // ========================================================
    // Valid data — no errors
    // ========================================================

    // Valid data with a single string field.
    #[test]
    fn valid_single_string_field() {
        let errs = validate_str("(\n  name: String,\n)", "(name: \"hello\")");
        assert!(errs.is_empty());
    }

    // Valid data with all primitive types.
    #[test]
    fn valid_all_primitives() {
        let schema = "(\n  s: String,\n  i: Integer,\n  f: Float,\n  b: Bool,\n)";
        let data = "(s: \"hi\", i: 42, f: 3.14, b: true)";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Valid data with None option.
    #[test]
    fn valid_option_none() {
        let errs = validate_str("(\n  power: Option(Integer),\n)", "(power: None)");
        assert!(errs.is_empty());
    }

    // Valid data with Some option.
    #[test]
    fn valid_option_some() {
        let errs = validate_str("(\n  power: Option(Integer),\n)", "(power: Some(5))");
        assert!(errs.is_empty());
    }

    // Valid data with empty list.
    #[test]
    fn valid_list_empty() {
        let errs = validate_str("(\n  tags: [String],\n)", "(tags: [])");
        assert!(errs.is_empty());
    }

    // Valid data with populated list.
    #[test]
    fn valid_list_populated() {
        let errs = validate_str("(\n  tags: [String],\n)", "(tags: [\"a\", \"b\"])");
        assert!(errs.is_empty());
    }

    // Valid data with enum variant.
    #[test]
    fn valid_enum_variant() {
        let schema = "(\n  kind: Kind,\n)\nenum Kind { A, B, C }";
        let data = "(kind: B)";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Valid data with list of enum variants.
    #[test]
    fn valid_enum_list() {
        let schema = "(\n  types: [CardType],\n)\nenum CardType { Creature, Trap }";
        let data = "(types: [Creature, Trap])";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Valid data with nested struct.
    #[test]
    fn valid_nested_struct() {
        let schema = "(\n  cost: (\n    generic: Integer,\n    sigil: Integer,\n  ),\n)";
        let data = "(cost: (generic: 2, sigil: 1))";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // ========================================================
    // TypeMismatch errors
    // ========================================================

    // String field rejects integer value.
    #[test]
    fn type_mismatch_string_got_integer() {
        let errs = validate_str("(\n  name: String,\n)", "(name: 42)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "String"));
    }

    // Integer field rejects string value.
    #[test]
    fn type_mismatch_integer_got_string() {
        let errs = validate_str("(\n  age: Integer,\n)", "(age: \"five\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "Integer"));
    }

    // Float field rejects integer value.
    #[test]
    fn type_mismatch_float_got_integer() {
        let errs = validate_str("(\n  rate: Float,\n)", "(rate: 5)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "Float"));
    }

    // Bool field rejects string value.
    #[test]
    fn type_mismatch_bool_got_string() {
        let errs = validate_str("(\n  flag: Bool,\n)", "(flag: \"yes\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "Bool"));
    }

    // Error path is correct for type mismatch.
    #[test]
    fn type_mismatch_has_correct_path() {
        let errs = validate_str("(\n  name: String,\n)", "(name: 42)");
        assert_eq!(errs[0].path, "name");
    }

    // ========================================================
    // MissingField errors
    // ========================================================

    // Missing field is detected.
    #[test]
    fn missing_field_detected() {
        let errs = validate_str("(\n  name: String,\n  age: Integer,\n)", "(name: \"hi\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::MissingField { field_name } if field_name == "age"));
    }

    // Missing field path is correct.
    #[test]
    fn missing_field_has_correct_path() {
        let errs = validate_str("(\n  name: String,\n  age: Integer,\n)", "(name: \"hi\")");
        assert_eq!(errs[0].path, "age");
    }

    // Missing field span points to close paren.
    #[test]
    fn missing_field_span_points_to_close_paren() {
        let data = "(name: \"hi\")";
        let errs = validate_str("(\n  name: String,\n  age: Integer,\n)", data);
        // close paren is the last character
        assert_eq!(errs[0].span.start.offset, data.len() - 1);
    }

    // Multiple missing fields are all reported.
    #[test]
    fn missing_fields_all_reported() {
        let errs = validate_str("(\n  a: String,\n  b: Integer,\n  c: Bool,\n)", "()");
        assert_eq!(errs.len(), 3);
    }

    // ========================================================
    // Default values — fields with defaults are optional
    // ========================================================

    // Field with default does not produce MissingField when absent.
    #[test]
    fn default_field_not_required() {
        let errs = validate_str("(\n  name: String,\n  label: String = \"none\",\n)", "(name: \"hi\")");
        assert!(errs.is_empty());
    }

    // Field with default still validates type when present.
    #[test]
    fn default_field_still_validates_type() {
        let errs = validate_str("(\n  name: String,\n  label: String = \"none\",\n)", "(name: \"hi\", label: 42)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { .. }));
    }

    // Field with default accepts correct type when present.
    #[test]
    fn default_field_accepts_correct_type() {
        let errs = validate_str("(\n  name: String,\n  label: String = \"none\",\n)", "(name: \"hi\", label: \"custom\")");
        assert!(errs.is_empty());
    }

    // Field without default still produces MissingField.
    #[test]
    fn non_default_field_still_required() {
        let errs = validate_str("(\n  name: String,\n  label: String = \"none\",\n)", "(label: \"hi\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::MissingField { field_name } if field_name == "name"));
    }

    // Multiple fields with defaults can all be absent.
    #[test]
    fn multiple_default_fields_all_absent() {
        let errs = validate_str(
            "(\n  name: String,\n  a: Integer = 0,\n  b: Bool = false,\n  c: String = \"x\",\n)",
            "(name: \"hi\")",
        );
        assert!(errs.is_empty());
    }

    // Default on Option field allows absence.
    #[test]
    fn default_option_field_not_required() {
        let errs = validate_str("(\n  name: String,\n  tag: Option(String) = None,\n)", "(name: \"hi\")");
        assert!(errs.is_empty());
    }

    // Default on list field allows absence.
    #[test]
    fn default_list_field_not_required() {
        let errs = validate_str("(\n  name: String,\n  tags: [String] = [],\n)", "(name: \"hi\")");
        assert!(errs.is_empty());
    }

    // ========================================================
    // UnknownField errors
    // ========================================================

    // Unknown field is detected.
    #[test]
    fn unknown_field_detected() {
        let errs = validate_str("(\n  name: String,\n)", "(name: \"hi\", colour: \"red\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::UnknownField { field_name } if field_name == "colour"));
    }

    // Unknown field path is correct.
    #[test]
    fn unknown_field_has_correct_path() {
        let errs = validate_str("(\n  name: String,\n)", "(name: \"hi\", extra: 5)");
        assert_eq!(errs[0].path, "extra");
    }

    // ========================================================
    // InvalidEnumVariant errors
    // ========================================================

    // Invalid enum variant is detected.
    #[test]
    fn invalid_enum_variant() {
        let schema = "(\n  kind: Kind,\n)\nenum Kind { A, B }";
        let errs = validate_str(schema, "(kind: C)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::InvalidEnumVariant { variant, .. } if variant == "C"));
    }

    // Enum field rejects string value (should be bare identifier).
    #[test]
    fn enum_rejects_string() {
        let schema = "(\n  kind: Kind,\n)\nenum Kind { A, B }";
        let errs = validate_str(schema, "(kind: \"A\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { .. }));
    }

    // ========================================================
    // ExpectedOption errors
    // ========================================================

    // Option field rejects bare integer (not wrapped in Some).
    #[test]
    fn expected_option_got_bare_value() {
        let errs = validate_str("(\n  power: Option(Integer),\n)", "(power: 5)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::ExpectedOption { .. }));
    }

    // Some wrapping wrong type is an error.
    #[test]
    fn option_some_wrong_inner_type() {
        let errs = validate_str("(\n  power: Option(Integer),\n)", "(power: Some(\"five\"))");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "Integer"));
    }

    // ========================================================
    // ExpectedList errors
    // ========================================================

    // List field rejects non-list value.
    #[test]
    fn expected_list_got_string() {
        let errs = validate_str("(\n  tags: [String],\n)", "(tags: \"hi\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::ExpectedList { .. }));
    }

    // List element with wrong type is an error.
    #[test]
    fn list_element_wrong_type() {
        let errs = validate_str("(\n  tags: [String],\n)", "(tags: [\"ok\", 42])");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "String"));
    }

    // List element error has bracket path.
    #[test]
    fn list_element_error_has_bracket_path() {
        let errs = validate_str("(\n  tags: [String],\n)", "(tags: [\"ok\", 42])");
        assert_eq!(errs[0].path, "tags[1]");
    }

    // ========================================================
    // ExpectedStruct errors
    // ========================================================

    // Struct field rejects non-struct value.
    #[test]
    fn expected_struct_got_integer() {
        let schema = "(\n  cost: (\n    generic: Integer,\n  ),\n)";
        let errs = validate_str(schema, "(cost: 5)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::ExpectedStruct { .. }));
    }

    // ========================================================
    // Nested validation
    // ========================================================

    // Type mismatch in nested struct has correct path.
    #[test]
    fn nested_struct_type_mismatch_path() {
        let schema = "(\n  cost: (\n    generic: Integer,\n  ),\n)";
        let errs = validate_str(schema, "(cost: (generic: \"two\"))");
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "cost.generic");
    }

    // Missing field in nested struct has correct path.
    #[test]
    fn nested_struct_missing_field_path() {
        let schema = "(\n  cost: (\n    generic: Integer,\n    sigil: Integer,\n  ),\n)";
        let errs = validate_str(schema, "(cost: (generic: 1))");
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "cost.sigil");
    }

    // ========================================================
    // Multiple errors collected
    // ========================================================

    // Multiple errors in one struct are all reported.
    #[test]
    fn multiple_errors_collected() {
        let schema = "(\n  name: String,\n  age: Integer,\n  active: Bool,\n)";
        let data = "(name: 42, age: \"five\", active: \"yes\")";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 3);
    }

    // Mixed error types are all collected.
    #[test]
    fn mixed_error_types_collected() {
        let schema = "(\n  name: String,\n  age: Integer,\n)";
        let data = "(name: \"hi\", age: \"five\", extra: true)";
        let errs = validate_str(schema, data);
        // age is TypeMismatch, extra is UnknownField
        assert_eq!(errs.len(), 2);
    }

    // ========================================================
    // Integration: card-like schema
    // ========================================================

    // Valid card data produces no errors.
    #[test]
    fn valid_card_data() {
        let schema = r#"(
            name: String,
            card_types: [CardType],
            legendary: Bool,
            power: Option(Integer),
            toughness: Option(Integer),
            keywords: [String],
        )
        enum CardType { Creature, Trap, Artifact }"#;
        let data = r#"(
            name: "Ashborn Hound",
            card_types: [Creature],
            legendary: false,
            power: Some(1),
            toughness: Some(1),
            keywords: [],
        )"#;
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Card data with multiple errors reports all of them.
    #[test]
    fn card_data_multiple_errors() {
        let schema = r#"(
            name: String,
            card_types: [CardType],
            legendary: Bool,
            power: Option(Integer),
        )
        enum CardType { Creature, Trap }"#;
        let data = r#"(
            name: 42,
            card_types: [Pirates],
            legendary: false,
            power: Some("five"),
        )"#;
        let errs = validate_str(schema, data);
        // name: TypeMismatch, card_types[0]: InvalidEnumVariant, power: TypeMismatch
        assert_eq!(errs.len(), 3);
    }

    // ========================================================
    // Type alias validation
    // ========================================================

    // Alias to a struct type validates correctly.
    #[test]
    fn alias_struct_valid() {
        let schema = "(\n  cost: Cost,\n)\ntype Cost = (generic: Integer,)";
        let data = "(cost: (generic: 5))";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Alias to a struct type catches type mismatch inside.
    #[test]
    fn alias_struct_type_mismatch() {
        let schema = "(\n  cost: Cost,\n)\ntype Cost = (generic: Integer,)";
        let data = "(cost: (generic: \"five\"))";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "Integer"));
    }

    // Alias to a primitive type validates correctly.
    #[test]
    fn alias_primitive_valid() {
        let schema = "(\n  name: Name,\n)\ntype Name = String";
        let data = "(name: \"hello\")";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Alias to a primitive type catches mismatch.
    #[test]
    fn alias_primitive_mismatch() {
        let schema = "(\n  name: Name,\n)\ntype Name = String";
        let data = "(name: 42)";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
    }

    // Alias used inside a list validates each element.
    #[test]
    fn alias_in_list_valid() {
        let schema = "(\n  costs: [Cost],\n)\ntype Cost = (generic: Integer,)";
        let data = "(costs: [(generic: 1), (generic: 2)])";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Alias used inside a list catches element errors.
    #[test]
    fn alias_in_list_element_error() {
        let schema = "(\n  costs: [Cost],\n)\ntype Cost = (generic: Integer,)";
        let data = "(costs: [(generic: 1), (generic: \"two\")])";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].path, "costs[1].generic");
    }

    // ========================================================
    // Map validation
    // ========================================================

    // Valid map with string keys and integer values.
    #[test]
    fn map_valid() {
        let schema = "(\n  attrs: {String: Integer},\n)";
        let data = "(attrs: {\"str\": 5, \"dex\": 3})";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Empty map is always valid.
    #[test]
    fn map_empty_valid() {
        let schema = "(\n  attrs: {String: Integer},\n)";
        let data = "(attrs: {})";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Non-map value where map expected.
    #[test]
    fn map_expected_got_string() {
        let schema = "(\n  attrs: {String: Integer},\n)";
        let data = "(attrs: \"not a map\")";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::ExpectedMap { .. }));
    }

    // Map value with wrong type.
    #[test]
    fn map_wrong_value_type() {
        let schema = "(\n  attrs: {String: Integer},\n)";
        let data = "(attrs: {\"str\": \"five\"})";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "Integer"));
    }

    // Map key with wrong type.
    #[test]
    fn map_wrong_key_type() {
        let schema = "(\n  attrs: {String: Integer},\n)";
        let data = "(attrs: {42: 5})";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "String"));
    }

    // ========================================================
    // Tuple validation
    // ========================================================

    // Valid tuple.
    #[test]
    fn tuple_valid() {
        let schema = "(\n  pos: (Float, Float),\n)";
        let data = "(pos: (1.0, 2.5))";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Non-tuple value where tuple expected.
    #[test]
    fn tuple_expected_got_string() {
        let schema = "(\n  pos: (Float, Float),\n)";
        let data = "(pos: \"not a tuple\")";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::ExpectedTuple { .. }));
    }

    // Tuple with wrong element count.
    #[test]
    fn tuple_wrong_length() {
        let schema = "(\n  pos: (Float, Float),\n)";
        let data = "(pos: (1.0, 2.5, 3.0))";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TupleLengthMismatch { expected: 2, found: 3 }));
    }

    // Tuple with wrong element type.
    #[test]
    fn tuple_wrong_element_type() {
        let schema = "(\n  pos: (Float, Float),\n)";
        let data = "(pos: (1.0, \"bad\"))";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "Float"));
    }

    // Tuple element error has correct path.
    #[test]
    fn tuple_element_error_path() {
        let schema = "(\n  pos: (Float, Float),\n)";
        let data = "(pos: (1.0, \"bad\"))";
        let errs = validate_str(schema, data);
        assert_eq!(errs[0].path, "pos.1");
    }

    // ========================================================
    // Enum variant with data — validation
    // ========================================================

    // Valid data variant.
    #[test]
    fn enum_data_variant_valid() {
        let schema = "(\n  effect: Effect,\n)\nenum Effect { Damage(Integer), Draw }";
        let data = "(effect: Damage(5))";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Valid unit variant alongside data variants.
    #[test]
    fn enum_unit_variant_valid() {
        let schema = "(\n  effect: Effect,\n)\nenum Effect { Damage(Integer), Draw }";
        let data = "(effect: Draw)";
        let errs = validate_str(schema, data);
        assert!(errs.is_empty());
    }

    // Data variant with wrong inner type.
    #[test]
    fn enum_data_variant_wrong_type() {
        let schema = "(\n  effect: Effect,\n)\nenum Effect { Damage(Integer), Draw }";
        let data = "(effect: Damage(\"five\"))";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { expected, .. } if expected == "Integer"));
    }

    // Unknown variant name with data.
    #[test]
    fn enum_data_variant_unknown() {
        let schema = "(\n  effect: Effect,\n)\nenum Effect { Damage(Integer), Draw }";
        let data = "(effect: Explode(10))";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::InvalidEnumVariant { .. }));
    }

    // Bare identifier for a variant that expects data.
    #[test]
    fn enum_missing_data() {
        let schema = "(\n  effect: Effect,\n)\nenum Effect { Damage(Integer), Draw }";
        let data = "(effect: Damage)";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::InvalidVariantData { .. }));
    }

    // Data provided for a unit variant.
    #[test]
    fn enum_unexpected_data() {
        let schema = "(\n  effect: Effect,\n)\nenum Effect { Damage(Integer), Draw }";
        let data = "(effect: Draw(5))";
        let errs = validate_str(schema, data);
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::InvalidVariantData { .. }));
    }
}
