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
    validate_struct(&schema.root, value, "", &mut errors, &schema.enums);
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
        RonValue::List(_) => "List".to_string(),
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
                validate_type(inner_type, inner_value, path, errors, enums);
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
                    validate_type(element_type, element, &element_path, errors, enums);
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

        // EnumRef: value must be an Identifier whose name is in the enum's variant set.
        // The enum is guaranteed to exist — the schema parser verified all references.
        SchemaType::EnumRef(enum_name) => {
            let enum_def = &enums[enum_name];
            if let RonValue::Identifier(variant) = &actual.value {
                if !enum_def.variants.contains(variant) {
                    errors.push(ValidationError {
                        path: path.to_string(),
                        span: actual.span,
                        kind: ErrorKind::InvalidEnumVariant {
                            enum_name: enum_name.clone(),
                            variant: variant.clone(),
                            valid: enum_def.variants.iter().cloned().collect(),
                        },
                    });
                }
            } else {
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

        // Nested struct: recurse into validate_struct.
        SchemaType::Struct(struct_def) => {
            validate_struct(struct_def, actual, path, errors, enums);
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

    // 1. Missing fields: in schema but not in data
    for field_def in &struct_def.fields {
        if !data_map.contains_key(field_def.name.value.as_str()) {
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
            validate_type(&field_def.type_.value, data_value, &field_path, errors, enums);
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
}
