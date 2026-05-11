/*************************
 * Author: Bradley Hunter
 */

use std::collections::{HashMap, HashSet};

use crate::error::{ErrorKind, ValidationError, ValidationResult, Warning, WarningKind};

/// Validates a parsed RON value against a schema.
///
/// Returns all validation errors and warnings found — does not stop at the first error.
/// An empty `errors` vec means the data is valid. Warnings are informational only.
#[must_use]
pub fn validate(schema: &Schema, value: &Spanned<RonValue>) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    validate_struct(&schema.root, value, "", &mut errors, &mut warnings, &schema.enums, &schema.aliases);
    ValidationResult { errors, warnings }
}
use crate::ron::RonValue;
use crate::schema::{CompareOp, EnumDef, FieldAnnotation, Schema, SchemaType, StructDef};
use crate::span::{Span, Spanned};

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
    warnings: &mut Vec<Warning>,
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
                validate_type(inner_type, inner_value, path, errors, warnings, enums, aliases);
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
                    validate_type(element_type, element, &element_path, errors, warnings, enums, aliases);
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
                            validate_type(expected_data_type, data, path, errors, warnings, enums, aliases);
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
                    validate_type(key_type, key, path, errors, warnings, enums, aliases);
                    let entry_path = format!("{path}[{key_desc}]");
                    validate_type(value_type, value, &entry_path, errors, warnings, enums, aliases);
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
                        validate_type(expected_type, element, &element_path, errors, warnings, enums, aliases);
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
                validate_type(&resolved.value, actual, path, errors, warnings, enums, aliases);
            }
            // If alias doesn't exist, the parser already caught it — unreachable in practice.
        }

        // Nested struct: recurse into validate_struct.
        SchemaType::Struct(struct_def) => {
            validate_struct(struct_def, actual, path, errors, warnings, enums, aliases);
        }
    }
}

/// Checks a field's annotations against its actual value.
#[allow(clippy::cast_precision_loss)]
fn validate_field_annotations(
    annotations: &[Spanned<FieldAnnotation>],
    field_name: &str,
    actual: &Spanned<RonValue>,
    errors: &mut Vec<ValidationError>,
) {
    for ann in annotations {
        match &ann.value {
            FieldAnnotation::Range(min, max) => {
                let numeric_value = match &actual.value {
                    RonValue::Integer(n) => Some(*n as f64),
                    RonValue::Float(f) => Some(*f),
                    _ => None,
                };
                if let Some(v) = numeric_value {
                    if v < *min || v > *max {
                        errors.push(ValidationError {
                            path: field_name.to_string(),
                            span: actual.span,
                            kind: ErrorKind::ValueOutOfRange {
                                field_name: field_name.to_string(),
                                min: *min,
                                max: *max,
                                found: v,
                            },
                        });
                    }
                }
            }
            FieldAnnotation::MinLength(min) => {
                let len = match &actual.value {
                    RonValue::String(s) => Some(s.len()),
                    RonValue::List(items) => Some(items.len()),
                    _ => None,
                };
                if let Some(l) = len {
                    if l < *min {
                        errors.push(ValidationError {
                            path: field_name.to_string(),
                            span: actual.span,
                            kind: ErrorKind::LengthTooShort {
                                field_name: field_name.to_string(),
                                min: *min,
                                found: l,
                            },
                        });
                    }
                }
            }
            FieldAnnotation::MaxLength(max) => {
                let len = match &actual.value {
                    RonValue::String(s) => Some(s.len()),
                    RonValue::List(items) => Some(items.len()),
                    _ => None,
                };
                if let Some(l) = len {
                    if l > *max {
                        errors.push(ValidationError {
                            path: field_name.to_string(),
                            span: actual.span,
                            kind: ErrorKind::LengthTooLong {
                                field_name: field_name.to_string(),
                                max: *max,
                                found: l,
                            },
                        });
                    }
                }
            }
            FieldAnnotation::Pattern(pattern) => {
                #[cfg(feature = "regex")]
                if let RonValue::String(s) = &actual.value {
                    if let Ok(re) = regex::Regex::new(pattern) {
                        if !re.is_match(s) {
                            errors.push(ValidationError {
                                path: field_name.to_string(),
                                span: actual.span,
                                kind: ErrorKind::PatternMismatch {
                                    field_name: field_name.to_string(),
                                    pattern: pattern.clone(),
                                },
                            });
                        }
                    }
                }
                #[cfg(not(feature = "regex"))]
                let _ = pattern;
            }
        }
    }
}

/// Compares two RON values using a comparison operator.
#[allow(clippy::cast_precision_loss)]
fn compare_values(left: &RonValue, right: &RonValue, op: CompareOp) -> Option<bool> {
    let (l, r) = match (left, right) {
        (RonValue::Integer(a), RonValue::Integer(b)) => (*a as f64, *b as f64),
        (RonValue::Float(a), RonValue::Float(b)) => (*a, *b),
        (RonValue::Integer(a), RonValue::Float(b)) => (*a as f64, *b),
        (RonValue::Float(a), RonValue::Integer(b)) => (*a, *b as f64),
        _ => return None,
    };
    Some(match op {
        CompareOp::Lt => l < r,
        CompareOp::Le => l <= r,
        CompareOp::Gt => l > r,
        CompareOp::Ge => l >= r,
        CompareOp::Eq => (l - r).abs() < f64::EPSILON,
        CompareOp::Ne => (l - r).abs() >= f64::EPSILON,
    })
}

/// Validates a RON struct against a schema struct definition.
///
/// Three checks:
/// 1. Missing fields — in schema but not in data (points to closing paren)
/// 2. Unknown fields — in data but not in schema (points to field name)
/// 3. Matching fields — present in both, recurse into `validate_type`
#[allow(clippy::too_many_lines)]
fn validate_struct(
    struct_def: &StructDef,
    actual: &Spanned<RonValue>,
    path: &str,
    errors: &mut Vec<ValidationError>,
    warnings: &mut Vec<Warning>,
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

    // 3. Matching fields: validate each against its expected type, then check annotations
    for field_def in &struct_def.fields {
        if let Some(data_value) = data_map.get(field_def.name.value.as_str()) {
            let field_path = build_path(path, &field_def.name.value);
            validate_type(&field_def.type_.value, data_value, &field_path, errors, warnings, enums, aliases);
            if !field_def.annotations.is_empty() {
                validate_field_annotations(&field_def.annotations, &field_path, data_value, errors);
            }
        }
    }

    // 4. Field order: warn if data fields appear in a different order than the schema
    let schema_order: Vec<&str> = struct_def.fields.iter()
        .map(|f| f.name.value.as_str())
        .collect();
    let data_fields: Vec<(&str, Span)> = data_struct.fields.iter()
        .map(|(name, _)| (name.value.as_str(), name.span))
        .collect();
    let mut last_schema_index = 0;
    for (data_name, data_span) in &data_fields {
        if let Some(schema_index) = schema_order.iter().position(|&s| s == *data_name) {
            if schema_index < last_schema_index {
                // Find the field that should have come after this one
                let expected_after = schema_order[last_schema_index];
                warnings.push(Warning {
                    path: build_path(path, data_name),
                    span: *data_span,
                    kind: WarningKind::FieldOrderMismatch {
                        field_name: data_name.to_string(),
                        expected_after: expected_after.to_string(),
                    },
                });
            } else {
                last_schema_index = schema_index;
            }
        }
    }

    // 5. @require constraints: cross-field comparisons
    for ann in &struct_def.annotations {
        let left_name = &ann.value.left;
        let right_name = &ann.value.right;

        let default_map: HashMap<&str, &RonValue> = struct_def.fields.iter()
            .filter_map(|f| f.default.as_ref().map(|d| (f.name.value.as_str(), &d.value)))
            .collect();

        let left_val = data_map.get(left_name.as_str())
            .map(|v| &v.value)
            .or_else(|| default_map.get(left_name.as_str()).copied());
        let right_val = data_map.get(right_name.as_str())
            .map(|v| &v.value)
            .or_else(|| default_map.get(right_name.as_str()).copied());

        let (Some(lv), Some(rv)) = (left_val, right_val) else {
            continue;
        };

        let result = compare_values(lv, rv, ann.value.op);
        if result == Some(false) {
            let op_str = match ann.value.op {
                CompareOp::Lt => "<",
                CompareOp::Le => "<=",
                CompareOp::Gt => ">",
                CompareOp::Ge => ">=",
                CompareOp::Eq => "==",
                CompareOp::Ne => "!=",
            };
            errors.push(ValidationError {
                path: path.to_string(),
                span: actual.span,
                kind: ErrorKind::CrossFieldViolation {
                    constraint: format!("{left_name} {op_str} {right_name}"),
                },
            });
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
        validate_full(schema_src, data_src).errors
    }

    /// Parses both a schema and data string, runs validation, returns the full result.
    fn validate_full(schema_src: &str, data_src: &str) -> ValidationResult {
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
    // FieldOrderMismatch warnings
    // ========================================================

    // No warning when fields are in schema order.
    #[test]
    fn field_order_correct_no_warning() {
        let result = validate_full("(\n  a: String,\n  b: Integer,\n)", "(a: \"hi\", b: 1)");
        assert!(result.warnings.is_empty());
    }

    // Warning when fields are swapped.
    #[test]
    fn field_order_swapped_produces_warning() {
        let result = validate_full("(\n  a: String,\n  b: Integer,\n)", "(b: 1, a: \"hi\")");
        assert_eq!(result.warnings.len(), 1);
    }

    // Warning has FieldOrderMismatch kind.
    #[test]
    fn field_order_warning_has_correct_kind() {
        let result = validate_full("(\n  a: String,\n  b: Integer,\n)", "(b: 1, a: \"hi\")");
        assert!(matches!(&result.warnings[0].kind, WarningKind::FieldOrderMismatch { .. }));
    }

    // Warning identifies the out-of-order field.
    #[test]
    fn field_order_warning_identifies_field() {
        let result = validate_full("(\n  a: String,\n  b: Integer,\n)", "(b: 1, a: \"hi\")");
        if let WarningKind::FieldOrderMismatch { field_name, .. } = &result.warnings[0].kind {
            assert_eq!(field_name, "a");
        } else {
            panic!("expected FieldOrderMismatch");
        }
    }

    // Warning identifies the field it should come after.
    #[test]
    fn field_order_warning_identifies_expected_after() {
        let result = validate_full("(\n  a: String,\n  b: Integer,\n)", "(b: 1, a: \"hi\")");
        if let WarningKind::FieldOrderMismatch { expected_after, .. } = &result.warnings[0].kind {
            assert_eq!(expected_after, "b");
        } else {
            panic!("expected FieldOrderMismatch");
        }
    }

    // Warning has correct path.
    #[test]
    fn field_order_warning_has_correct_path() {
        let result = validate_full("(\n  a: String,\n  b: Integer,\n)", "(b: 1, a: \"hi\")");
        assert_eq!(result.warnings[0].path, "a");
    }

    // Warning span points to the field name.
    #[test]
    fn field_order_warning_has_span() {
        let result = validate_full("(\n  a: String,\n  b: Integer,\n)", "(b: 1, a: \"hi\")");
        assert!(result.warnings[0].span.start.line > 0);
    }

    // Correct order with three fields produces no warning.
    #[test]
    fn field_order_three_fields_correct() {
        let result = validate_full(
            "(\n  a: String,\n  b: Integer,\n  c: Bool,\n)",
            "(a: \"hi\", b: 1, c: true)",
        );
        assert!(result.warnings.is_empty());
    }

    // Middle field out of order.
    #[test]
    fn field_order_middle_field_swapped() {
        let result = validate_full(
            "(\n  a: String,\n  b: Integer,\n  c: Bool,\n)",
            "(a: \"hi\", c: true, b: 1)",
        );
        assert_eq!(result.warnings.len(), 1);
        if let WarningKind::FieldOrderMismatch { field_name, .. } = &result.warnings[0].kind {
            assert_eq!(field_name, "b");
        } else {
            panic!("expected FieldOrderMismatch");
        }
    }

    // Field order warning does not produce errors.
    #[test]
    fn field_order_warning_no_errors() {
        let result = validate_full("(\n  a: String,\n  b: Integer,\n)", "(b: 1, a: \"hi\")");
        assert!(result.errors.is_empty());
    }

    // Unknown fields don't affect order checking.
    #[test]
    fn field_order_with_unknown_field() {
        let result = validate_full("(\n  a: String,\n  b: Integer,\n)", "(b: 1, x: true, a: \"hi\")");
        assert_eq!(result.warnings.len(), 1);
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

    // ========================================================
    // @range annotation
    // ========================================================

    // Integer within range passes.
    #[test]
    fn range_integer_within_bounds() {
        let errs = validate_str("(\n  @range(0, 100)\n  health: Integer,\n)", "(health: 50)");
        assert!(errs.is_empty());
    }

    // Integer at lower bound passes.
    #[test]
    fn range_integer_at_min() {
        let errs = validate_str("(\n  @range(0, 100)\n  health: Integer,\n)", "(health: 0)");
        assert!(errs.is_empty());
    }

    // Integer at upper bound passes.
    #[test]
    fn range_integer_at_max() {
        let errs = validate_str("(\n  @range(0, 100)\n  health: Integer,\n)", "(health: 100)");
        assert!(errs.is_empty());
    }

    // Integer below range fails.
    #[test]
    fn range_integer_below_min() {
        let errs = validate_str("(\n  @range(0, 100)\n  health: Integer,\n)", "(health: -1)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::ValueOutOfRange { .. }));
    }

    // Integer above range fails.
    #[test]
    fn range_integer_above_max() {
        let errs = validate_str("(\n  @range(0, 100)\n  health: Integer,\n)", "(health: 101)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::ValueOutOfRange { .. }));
    }

    // Float within range passes.
    #[test]
    fn range_float_within_bounds() {
        let errs = validate_str("(\n  @range(0.0, 1.0)\n  ratio: Float,\n)", "(ratio: 0.5)");
        assert!(errs.is_empty());
    }

    // Float below range fails.
    #[test]
    fn range_float_below_min() {
        let errs = validate_str("(\n  @range(0.0, 1.0)\n  ratio: Float,\n)", "(ratio: -0.1)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::ValueOutOfRange { .. }));
    }

    // Negative range bounds work.
    #[test]
    fn range_negative_bounds() {
        let errs = validate_str("(\n  @range(-50, 50)\n  offset: Integer,\n)", "(offset: -50)");
        assert!(errs.is_empty());
    }

    // Range error contains correct field name.
    #[test]
    fn range_error_has_field_name() {
        let errs = validate_str("(\n  @range(0, 100)\n  health: Integer,\n)", "(health: 200)");
        if let ErrorKind::ValueOutOfRange { field_name, .. } = &errs[0].kind {
            assert_eq!(field_name, "health");
        } else {
            panic!("expected ValueOutOfRange");
        }
    }

    // Range error contains correct bounds and found value.
    #[test]
    fn range_error_has_bounds_and_value() {
        let errs = validate_str("(\n  @range(0, 100)\n  health: Integer,\n)", "(health: 200)");
        if let ErrorKind::ValueOutOfRange { min, max, found, .. } = &errs[0].kind {
            assert_eq!(*min, 0.0);
            assert_eq!(*max, 100.0);
            assert_eq!(*found, 200.0);
        } else {
            panic!("expected ValueOutOfRange");
        }
    }

    // ========================================================
    // @min_length annotation
    // ========================================================

    // String meeting min_length passes.
    #[test]
    fn min_length_string_passes() {
        let errs = validate_str("(\n  @min_length(3)\n  name: String,\n)", "(name: \"abc\")");
        assert!(errs.is_empty());
    }

    // String shorter than min_length fails.
    #[test]
    fn min_length_string_fails() {
        let errs = validate_str("(\n  @min_length(3)\n  name: String,\n)", "(name: \"ab\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::LengthTooShort { .. }));
    }

    // List meeting min_length passes.
    #[test]
    fn min_length_list_passes() {
        let errs = validate_str("(\n  @min_length(2)\n  tags: [String],\n)", "(tags: [\"a\", \"b\"])");
        assert!(errs.is_empty());
    }

    // List shorter than min_length fails.
    #[test]
    fn min_length_list_fails() {
        let errs = validate_str("(\n  @min_length(2)\n  tags: [String],\n)", "(tags: [\"a\"])");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::LengthTooShort { .. }));
    }

    // min_length error contains correct values.
    #[test]
    fn min_length_error_has_values() {
        let errs = validate_str("(\n  @min_length(3)\n  name: String,\n)", "(name: \"ab\")");
        if let ErrorKind::LengthTooShort { min, found, .. } = &errs[0].kind {
            assert_eq!(*min, 3);
            assert_eq!(*found, 2);
        } else {
            panic!("expected LengthTooShort");
        }
    }

    // ========================================================
    // @max_length annotation
    // ========================================================

    // String within max_length passes.
    #[test]
    fn max_length_string_passes() {
        let errs = validate_str("(\n  @max_length(5)\n  name: String,\n)", "(name: \"abc\")");
        assert!(errs.is_empty());
    }

    // String exceeding max_length fails.
    #[test]
    fn max_length_string_fails() {
        let errs = validate_str("(\n  @max_length(3)\n  name: String,\n)", "(name: \"abcd\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::LengthTooLong { .. }));
    }

    // List exceeding max_length fails.
    #[test]
    fn max_length_list_fails() {
        let errs = validate_str("(\n  @max_length(2)\n  tags: [String],\n)", "(tags: [\"a\", \"b\", \"c\"])");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::LengthTooLong { .. }));
    }

    // max_length error contains correct values.
    #[test]
    fn max_length_error_has_values() {
        let errs = validate_str("(\n  @max_length(3)\n  name: String,\n)", "(name: \"abcde\")");
        if let ErrorKind::LengthTooLong { max, found, .. } = &errs[0].kind {
            assert_eq!(*max, 3);
            assert_eq!(*found, 5);
        } else {
            panic!("expected LengthTooLong");
        }
    }

    // ========================================================
    // Multiple annotations on one field
    // ========================================================

    // Both min and max length on same field — passes.
    #[test]
    fn min_and_max_length_passes() {
        let errs = validate_str("(\n  @min_length(1)\n  @max_length(10)\n  name: String,\n)", "(name: \"hello\")");
        assert!(errs.is_empty());
    }

    // Both min and max length on same field — too short.
    #[test]
    fn min_and_max_length_too_short() {
        let errs = validate_str("(\n  @min_length(3)\n  @max_length(10)\n  name: String,\n)", "(name: \"ab\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::LengthTooShort { .. }));
    }

    // Both min and max length on same field — too long.
    #[test]
    fn min_and_max_length_too_long() {
        let errs = validate_str("(\n  @min_length(1)\n  @max_length(3)\n  name: String,\n)", "(name: \"abcde\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::LengthTooLong { .. }));
    }

    // Annotation on field that doesn't match type is silently ignored.
    #[test]
    fn range_on_string_field_ignored() {
        let errs = validate_str("(\n  @range(0, 100)\n  name: String,\n)", "(name: \"hello\")");
        assert!(errs.is_empty());
    }

    // Annotation not checked when field has type mismatch.
    #[test]
    fn annotation_skipped_on_type_mismatch() {
        let errs = validate_str("(\n  @range(0, 100)\n  health: Integer,\n)", "(health: \"hello\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::TypeMismatch { .. }));
    }

    // ========================================================
    // @pattern annotation (requires regex feature)
    // ========================================================

    // String matching pattern passes.
    #[cfg(feature = "regex")]
    #[test]
    fn pattern_matching_string_passes() {
        let errs = validate_str("(\n  @pattern(\"^[a-z]+$\")\n  tag: String,\n)", "(tag: \"hello\")");
        assert!(errs.is_empty());
    }

    // String not matching pattern fails.
    #[cfg(feature = "regex")]
    #[test]
    fn pattern_non_matching_string_fails() {
        let errs = validate_str("(\n  @pattern(\"^[a-z]+$\")\n  tag: String,\n)", "(tag: \"Hello123\")");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::PatternMismatch { .. }));
    }

    // Pattern error contains correct field name and pattern.
    #[cfg(feature = "regex")]
    #[test]
    fn pattern_error_has_field_and_pattern() {
        let errs = validate_str("(\n  @pattern(\"^[0-9]+$\")\n  code: String,\n)", "(code: \"abc\")");
        if let ErrorKind::PatternMismatch { field_name, pattern } = &errs[0].kind {
            assert_eq!(field_name, "code");
            assert_eq!(pattern, "^[0-9]+$");
        } else {
            panic!("expected PatternMismatch");
        }
    }

    // Pattern on non-string field is silently ignored.
    #[cfg(feature = "regex")]
    #[test]
    fn pattern_on_integer_field_ignored() {
        let errs = validate_str("(\n  @pattern(\"^[0-9]+$\")\n  count: Integer,\n)", "(count: 42)");
        assert!(errs.is_empty());
    }

    // ========================================================
    // @require annotation
    // ========================================================

    // Satisfied <= constraint passes.
    #[test]
    fn require_le_satisfied() {
        let schema = "(\n  @require(min <= max)\n  min: Integer,\n  max: Integer,\n)";
        let errs = validate_str(schema, "(min: 5, max: 10)");
        assert!(errs.is_empty());
    }

    // Equal values satisfy <=.
    #[test]
    fn require_le_equal_satisfied() {
        let schema = "(\n  @require(min <= max)\n  min: Integer,\n  max: Integer,\n)";
        let errs = validate_str(schema, "(min: 5, max: 5)");
        assert!(errs.is_empty());
    }

    // Violated <= constraint fails.
    #[test]
    fn require_le_violated() {
        let schema = "(\n  @require(min <= max)\n  min: Integer,\n  max: Integer,\n)";
        let errs = validate_str(schema, "(min: 10, max: 5)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::CrossFieldViolation { .. }));
    }

    // Satisfied < constraint passes.
    #[test]
    fn require_lt_satisfied() {
        let schema = "(\n  @require(start < end)\n  start: Integer,\n  end: Integer,\n)";
        let errs = validate_str(schema, "(start: 1, end: 10)");
        assert!(errs.is_empty());
    }

    // Equal values violate <.
    #[test]
    fn require_lt_equal_violated() {
        let schema = "(\n  @require(start < end)\n  start: Integer,\n  end: Integer,\n)";
        let errs = validate_str(schema, "(start: 5, end: 5)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::CrossFieldViolation { .. }));
    }

    // Satisfied == constraint passes.
    #[test]
    fn require_eq_satisfied() {
        let schema = "(\n  @require(a == b)\n  a: Integer,\n  b: Integer,\n)";
        let errs = validate_str(schema, "(a: 5, b: 5)");
        assert!(errs.is_empty());
    }

    // Violated == constraint fails.
    #[test]
    fn require_eq_violated() {
        let schema = "(\n  @require(a == b)\n  a: Integer,\n  b: Integer,\n)";
        let errs = validate_str(schema, "(a: 5, b: 10)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::CrossFieldViolation { .. }));
    }

    // Satisfied != constraint passes.
    #[test]
    fn require_ne_satisfied() {
        let schema = "(\n  @require(a != b)\n  a: Integer,\n  b: Integer,\n)";
        let errs = validate_str(schema, "(a: 5, b: 10)");
        assert!(errs.is_empty());
    }

    // Violated != constraint fails.
    #[test]
    fn require_ne_violated() {
        let schema = "(\n  @require(a != b)\n  a: Integer,\n  b: Integer,\n)";
        let errs = validate_str(schema, "(a: 5, b: 5)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::CrossFieldViolation { .. }));
    }

    // CrossFieldViolation error contains constraint string.
    #[test]
    fn require_error_has_constraint() {
        let schema = "(\n  @require(min <= max)\n  min: Integer,\n  max: Integer,\n)";
        let errs = validate_str(schema, "(min: 10, max: 5)");
        if let ErrorKind::CrossFieldViolation { constraint } = &errs[0].kind {
            assert_eq!(constraint, "min <= max");
        } else {
            panic!("expected CrossFieldViolation");
        }
    }

    // @require with float fields.
    #[test]
    fn require_float_comparison() {
        let schema = "(\n  @require(low < high)\n  low: Float,\n  high: Float,\n)";
        let errs = validate_str(schema, "(low: 1.5, high: 2.5)");
        assert!(errs.is_empty());
    }

    // @require with mixed integer and float fields.
    #[test]
    fn require_mixed_int_float() {
        let schema = "(\n  @require(count <= limit)\n  count: Integer,\n  limit: Float,\n)";
        let errs = validate_str(schema, "(count: 5, limit: 10.0)");
        assert!(errs.is_empty());
    }

    // @require skipped when field is missing (missing field error takes priority).
    #[test]
    fn require_skipped_when_field_missing() {
        let schema = "(\n  @require(min <= max)\n  min: Integer,\n  max: Integer,\n)";
        let errs = validate_str(schema, "(min: 5)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::MissingField { .. }));
    }

    // @require uses default value for absent field.
    #[test]
    fn require_uses_default_for_absent_field() {
        let schema = "(\n  @require(min <= max)\n  min: Integer,\n  max: Integer = 100,\n)";
        let errs = validate_str(schema, "(min: 5)");
        assert!(errs.is_empty());
    }

    // @require violated with default value.
    #[test]
    fn require_violated_with_default() {
        let schema = "(\n  @require(min <= max)\n  min: Integer,\n  max: Integer = 0,\n)";
        let errs = validate_str(schema, "(min: 5)");
        assert_eq!(errs.len(), 1);
        assert!(matches!(&errs[0].kind, ErrorKind::CrossFieldViolation { .. }));
    }

    // Multiple @require constraints both checked.
    #[test]
    fn require_multiple_constraints() {
        let schema = "(\n  @require(a < b)\n  @require(b < c)\n  a: Integer,\n  b: Integer,\n  c: Integer,\n)";
        let errs = validate_str(schema, "(a: 1, b: 2, c: 3)");
        assert!(errs.is_empty());
    }

    // Multiple @require constraints — both violated.
    #[test]
    fn require_multiple_constraints_both_violated() {
        let schema = "(\n  @require(a < b)\n  @require(b < c)\n  a: Integer,\n  b: Integer,\n  c: Integer,\n)";
        let errs = validate_str(schema, "(a: 3, b: 2, c: 1)");
        assert_eq!(errs.len(), 2);
    }

    // @require on non-numeric fields is skipped.
    #[test]
    fn require_non_numeric_skipped() {
        let schema = "(\n  @require(a <= b)\n  a: String,\n  b: String,\n)";
        let errs = validate_str(schema, "(a: \"hello\", b: \"world\")");
        assert!(errs.is_empty());
    }
}
