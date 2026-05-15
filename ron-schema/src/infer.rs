/*************************
 * Author: Bradley Hunter
 */

use std::collections::HashMap;

use crate::ron::RonValue;
use crate::schema::{EnumDef, FieldDef, Schema, SchemaType, StructDef};
use crate::span::{Position, Span, Spanned};

/// A dummy span used for inferred schema elements that have no source location.
const DUMMY_SPAN: Span = Span {
    start: Position { offset: 0, line: 0, column: 0 },
    end: Position { offset: 0, line: 0, column: 0 },
};

fn dummy_spanned<T>(value: T) -> Spanned<T> {
    Spanned { value, span: DUMMY_SPAN }
}

/// Infers a [`Schema`] from a parsed RON value.
///
/// The input should typically be a `RonValue::Struct` (the root of a `.ron` file).
/// The inferred schema is a starting point — users are expected to review and refine it.
///
/// Inference rules:
/// - String → `String`, Integer → `Integer`, Float → `Float`, Bool → `Bool`
/// - `Some(value)` → `Option(inferred_type)`, `None` → `Option(String)`
/// - Lists → `[element_type]` from the first element; empty lists → `[String]`
/// - Bare identifiers → auto-generated enum named after the field
/// - Nested structs → recursive inference
/// - Maps → `{key_type: value_type}` from the first entry; empty maps → `{String: String}`
/// - Tuples → `(type1, type2, ...)` from each element
#[must_use]
pub fn infer_schema(value: &Spanned<RonValue>) -> Schema {
    let mut enums: HashMap<String, EnumDef> = HashMap::new();
    let root = match &value.value {
        RonValue::Struct(s) => infer_struct(s, "", &mut enums),
        _ => StructDef { fields: Vec::new(), annotations: Vec::new() },
    };
    Schema {
        root,
        enums,
        aliases: HashMap::new(),
        imports: Vec::new(),
    }
}

fn infer_struct(
    data: &crate::ron::RonStruct,
    path: &str,
    enums: &mut HashMap<String, EnumDef>,
) -> StructDef {
    let mut fields = Vec::new();
    for (name, value) in &data.fields {
        let type_ = infer_type(&name.value, &value.value, path, enums);
        fields.push(FieldDef {
            name: dummy_spanned(name.value.clone()),
            type_: dummy_spanned(type_),
            default: None,
            annotations: Vec::new(),
        });
    }
    StructDef { fields, annotations: Vec::new() }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + chars.as_str()
                }
                None => String::new(),
            }
        })
        .collect()
}

fn infer_type(
    field_name: &str,
    value: &RonValue,
    path: &str,
    enums: &mut HashMap<String, EnumDef>,
) -> SchemaType {
    match value {
        RonValue::String(_) => SchemaType::String,
        RonValue::Integer(_) => SchemaType::Integer,
        RonValue::Float(_) => SchemaType::Float,
        RonValue::Bool(_) => SchemaType::Bool,

        RonValue::Option(inner) => match inner {
            Some(inner_val) => {
                let inner_type = infer_type(field_name, &inner_val.value, path, enums);
                SchemaType::Option(Box::new(inner_type))
            }
            None => SchemaType::Option(Box::new(SchemaType::String)),
        },

        RonValue::List(elements) => {
            if elements.is_empty() {
                SchemaType::List(Box::new(SchemaType::String))
            } else {
                let mut variants: Vec<String> = Vec::new();
                let mut all_identifiers = true;
                for elem in elements {
                    if let RonValue::Identifier(name) = &elem.value {
                        variants.push(name.clone());
                    } else {
                        all_identifiers = false;
                        break;
                    }
                }

                if all_identifiers && !variants.is_empty() {
                    let enum_name = to_pascal_case(field_name);
                    let variant_map: HashMap<String, Option<SchemaType>> = variants
                        .into_iter()
                        .map(|v| (v, None))
                        .collect();
                    enums.insert(enum_name.clone(), EnumDef {
                        name: enum_name.clone(),
                        variants: variant_map,
                    });
                    SchemaType::List(Box::new(SchemaType::EnumRef(enum_name)))
                } else {
                    let inner_type = infer_type(field_name, &elements[0].value, path, enums);
                    SchemaType::List(Box::new(inner_type))
                }
            }
        }

        RonValue::Identifier(variant_name) => {
            let enum_name = to_pascal_case(field_name);
            let entry = enums.entry(enum_name.clone()).or_insert_with(|| EnumDef {
                name: enum_name.clone(),
                variants: HashMap::new(),
            });
            entry.variants.insert(variant_name.clone(), None);
            SchemaType::EnumRef(enum_name)
        }

        RonValue::EnumVariant(variant_name, data) => {
            let enum_name = to_pascal_case(field_name);
            let data_type = infer_type(field_name, &data.value, path, enums);
            let entry = enums.entry(enum_name.clone()).or_insert_with(|| EnumDef {
                name: enum_name.clone(),
                variants: HashMap::new(),
            });
            entry.variants.insert(variant_name.clone(), Some(data_type));
            SchemaType::EnumRef(enum_name)
        }

        RonValue::Map(entries) => {
            if entries.is_empty() {
                SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::String))
            } else {
                let (first_key, first_val) = &entries[0];
                let key_type = infer_type(field_name, &first_key.value, path, enums);
                let val_type = infer_type(field_name, &first_val.value, path, enums);
                SchemaType::Map(Box::new(key_type), Box::new(val_type))
            }
        }

        RonValue::Tuple(elements) => {
            let types: Vec<SchemaType> = elements.iter()
                .map(|e| infer_type(field_name, &e.value, path, enums))
                .collect();
            SchemaType::Tuple(types)
        }

        RonValue::Struct(s) => {
            let child_path = if path.is_empty() {
                field_name.to_string()
            } else {
                format!("{path}.{field_name}")
            };
            SchemaType::Struct(infer_struct(s, &child_path, enums))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ron::parser::parse_ron;

    fn infer(data_src: &str) -> Schema {
        let value = parse_ron(data_src).expect("test data should parse");
        infer_schema(&value)
    }

    // ========================================================
    // Primitive inference
    // ========================================================

    // String field inferred.
    #[test]
    fn infer_string_field() {
        let schema = infer("(name: \"hello\")");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::String);
    }

    // Integer field inferred.
    #[test]
    fn infer_integer_field() {
        let schema = infer("(count: 42)");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::Integer);
    }

    // Float field inferred.
    #[test]
    fn infer_float_field() {
        let schema = infer("(ratio: 3.14)");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::Float);
    }

    // Bool field inferred.
    #[test]
    fn infer_bool_field() {
        let schema = infer("(active: true)");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::Bool);
    }

    // ========================================================
    // Option inference
    // ========================================================

    // Some(value) inferred as Option(type).
    #[test]
    fn infer_option_some() {
        let schema = infer("(desc: Some(\"hello\"))");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::Option(Box::new(SchemaType::String)));
    }

    // None inferred as Option(String).
    #[test]
    fn infer_option_none() {
        let schema = infer("(desc: None)");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::Option(Box::new(SchemaType::String)));
    }

    // ========================================================
    // List inference
    // ========================================================

    // Non-empty list infers element type from first element.
    #[test]
    fn infer_list_of_strings() {
        let schema = infer("(tags: [\"a\", \"b\"])");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::List(Box::new(SchemaType::String)));
    }

    // Empty list defaults to [String].
    #[test]
    fn infer_empty_list() {
        let schema = infer("(tags: [])");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::List(Box::new(SchemaType::String)));
    }

    // List of identifiers creates an enum.
    #[test]
    fn infer_list_of_identifiers_creates_enum() {
        let schema = infer("(card_types: [Creature, Trap, Spell])");
        assert!(schema.enums.contains_key("CardTypes"));
        let enum_def = &schema.enums["CardTypes"];
        assert!(enum_def.variants.contains_key("Creature"));
        assert!(enum_def.variants.contains_key("Trap"));
        assert!(enum_def.variants.contains_key("Spell"));
    }

    // List of identifiers produces List(EnumRef).
    #[test]
    fn infer_list_of_identifiers_type() {
        let schema = infer("(tags: [Fast, Slow])");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::List(Box::new(SchemaType::EnumRef("Tags".to_string()))));
    }

    // ========================================================
    // Identifier / enum inference
    // ========================================================

    // Bare identifier creates an enum.
    #[test]
    fn infer_identifier_creates_enum() {
        let schema = infer("(status: Active)");
        assert!(schema.enums.contains_key("Status"));
        assert!(schema.enums["Status"].variants.contains_key("Active"));
    }

    // Identifier field type is EnumRef.
    #[test]
    fn infer_identifier_type() {
        let schema = infer("(status: Active)");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::EnumRef("Status".to_string()));
    }

    // Enum variant with data creates an enum with data type.
    #[test]
    fn infer_enum_variant_with_data() {
        let schema = infer("(effect: Damage(5))");
        assert!(schema.enums.contains_key("Effect"));
        assert_eq!(schema.enums["Effect"].variants.get("Damage"), Some(&Some(SchemaType::Integer)));
    }

    // ========================================================
    // Map inference
    // ========================================================

    // Non-empty map infers key/value types.
    #[test]
    fn infer_map_types() {
        let schema = infer("(attrs: {\"str\": 5})");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::Integer)));
    }

    // Empty map defaults to {String: String}.
    #[test]
    fn infer_empty_map() {
        let schema = infer("(attrs: {})");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::String)));
    }

    // ========================================================
    // Tuple inference
    // ========================================================

    // Tuple infers element types.
    #[test]
    fn infer_tuple_types() {
        let schema = infer("(pos: (1.0, 2.5))");
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::Tuple(vec![SchemaType::Float, SchemaType::Float]));
    }

    // ========================================================
    // Nested struct inference
    // ========================================================

    // Nested struct inferred recursively.
    #[test]
    fn infer_nested_struct() {
        let schema = infer("(window: (width: 1920, height: 1080))");
        if let SchemaType::Struct(inner) = &schema.root.fields[0].type_.value {
            assert_eq!(inner.fields[0].name.value, "width");
            assert_eq!(inner.fields[0].type_.value, SchemaType::Integer);
            assert_eq!(inner.fields[1].name.value, "height");
        } else {
            panic!("expected nested struct");
        }
    }

    // ========================================================
    // Field name to enum name conversion
    // ========================================================

    // snake_case converted to PascalCase.
    #[test]
    fn pascal_case_from_snake_case() {
        assert_eq!(to_pascal_case("card_type"), "CardType");
    }

    // Already PascalCase stays the same.
    #[test]
    fn pascal_case_unchanged() {
        assert_eq!(to_pascal_case("Status"), "Status");
    }

    // Single word capitalized.
    #[test]
    fn pascal_case_single_word() {
        assert_eq!(to_pascal_case("status"), "Status");
    }

    // ========================================================
    // Multiple fields
    // ========================================================

    // Multiple fields inferred correctly.
    #[test]
    fn infer_multiple_fields() {
        let schema = infer("(name: \"test\", count: 5, active: true)");
        assert_eq!(schema.root.fields.len(), 3);
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::String);
        assert_eq!(schema.root.fields[1].type_.value, SchemaType::Integer);
        assert_eq!(schema.root.fields[2].type_.value, SchemaType::Bool);
    }

    // ========================================================
    // Non-struct input
    // ========================================================

    // Non-struct root produces empty schema.
    #[test]
    fn infer_non_struct_root() {
        let value = parse_ron("\"hello\"").unwrap();
        let schema = infer_schema(&value);
        assert!(schema.root.fields.is_empty());
    }

    // ========================================================
    // Full roundtrip: infer → format → parse
    // ========================================================

    // Inferred schema formats and parses back.
    #[test]
    fn infer_format_parse_roundtrip() {
        let data = "(name: \"test\", count: 5, tags: [\"a\"], active: true, desc: Some(\"hi\"), status: Active)";
        let schema = infer(data);
        let formatted = crate::format::format_schema(&schema);
        crate::schema::parser::parse_schema(&formatted).expect("inferred schema should parse");
    }

    // Complex data roundtrips.
    #[test]
    fn infer_complex_roundtrip() {
        let data = "(name: \"app\", window: (width: 1920, height: 1080), attrs: {\"str\": 5}, pos: (1.0, 2.5))";
        let schema = infer(data);
        let formatted = crate::format::format_schema(&schema);
        crate::schema::parser::parse_schema(&formatted).expect("inferred schema should parse");
    }
}
