/*************************
 * Author: Bradley Hunter
 */

use std::fmt::Write;

use crate::schema::{Schema, SchemaType, StructDef};

/// Formats a [`Schema`] as valid `.ronschema` source text.
///
/// The output is a complete schema file that can be parsed back by [`parse_schema`].
/// Enum definitions are written after the root struct.
///
/// [`parse_schema`]: crate::schema::parser::parse_schema
#[must_use]
pub fn format_schema(schema: &Schema) -> String {
    let mut out = String::new();
    format_struct(&schema.root, &mut out, 0);
    out.push('\n');

    let mut enum_names: Vec<&String> = schema.enums.keys().collect();
    enum_names.sort();
    for name in enum_names {
        let enum_def = &schema.enums[name];
        out.push('\n');
        let _ = write!(out, "enum {name} {{ ");
        let mut variants: Vec<&String> = enum_def.variants.keys().collect();
        variants.sort();
        for (i, variant) in variants.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            if let Some(data_type) = &enum_def.variants[*variant] {
                let _ = write!(out, "{variant}({})", format_type(data_type));
            } else {
                out.push_str(variant);
            }
        }
        out.push_str(" }\n");
    }

    out
}

fn format_type(ty: &SchemaType) -> String {
    match ty {
        SchemaType::String => "String".to_string(),
        SchemaType::Integer => "Integer".to_string(),
        SchemaType::Float => "Float".to_string(),
        SchemaType::Bool => "Bool".to_string(),
        SchemaType::Option(inner) => format!("Option({})", format_type(inner)),
        SchemaType::List(inner) => format!("[{}]", format_type(inner)),
        SchemaType::EnumRef(name) | SchemaType::AliasRef(name) => name.clone(),
        SchemaType::Map(key, value) => format!("{{{}: {}}}", format_type(key), format_type(value)),
        SchemaType::Tuple(types) => {
            let inner: Vec<String> = types.iter().map(format_type).collect();
            format!("({})", inner.join(", "))
        }
        SchemaType::Struct(def) => {
            let mut out = String::new();
            format_struct(def, &mut out, 0);
            out
        }
    }
}

fn format_struct(def: &StructDef, out: &mut String, indent: usize) {
    let close_prefix = "  ".repeat(indent);
    let inner_prefix = "  ".repeat(indent + 1);

    out.push_str("(\n");
    for field in &def.fields {
        match &field.type_.value {
            SchemaType::Struct(inner_def) => {
                let _ = write!(out, "{inner_prefix}{}: ", field.name.value);
                format_struct(inner_def, out, indent + 1);
                out.push_str(",\n");
            }
            other => {
                let _ = writeln!(out, "{inner_prefix}{}: {},", field.name.value, format_type(other));
            }
        }
    }
    let _ = write!(out, "{close_prefix})");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parser::parse_schema;

    fn roundtrip(source: &str) -> String {
        let schema = parse_schema(source).expect("test schema should parse");
        format_schema(&schema)
    }

    // ========================================================
    // Primitive fields
    // ========================================================

    // Single string field.
    #[test]
    fn format_single_string_field() {
        let out = roundtrip("(\n  name: String,\n)");
        assert!(out.contains("name: String,"));
    }

    // All primitive types.
    #[test]
    fn format_all_primitives() {
        let out = roundtrip("(\n  s: String,\n  i: Integer,\n  f: Float,\n  b: Bool,\n)");
        assert!(out.contains("s: String,"));
        assert!(out.contains("i: Integer,"));
        assert!(out.contains("f: Float,"));
        assert!(out.contains("b: Bool,"));
    }

    // ========================================================
    // Composite types
    // ========================================================

    // Option type.
    #[test]
    fn format_option_type() {
        let out = roundtrip("(\n  desc: Option(String),\n)");
        assert!(out.contains("desc: Option(String),"));
    }

    // List type.
    #[test]
    fn format_list_type() {
        let out = roundtrip("(\n  tags: [String],\n)");
        assert!(out.contains("tags: [String],"));
    }

    // Map type.
    #[test]
    fn format_map_type() {
        let out = roundtrip("(\n  attrs: {String: Integer},\n)");
        assert!(out.contains("attrs: {String: Integer},"));
    }

    // Tuple type.
    #[test]
    fn format_tuple_type() {
        let out = roundtrip("(\n  pos: (Integer, Integer),\n)");
        assert!(out.contains("pos: (Integer, Integer),"));
    }

    // ========================================================
    // Nested structs
    // ========================================================

    // Nested struct is indented.
    #[test]
    fn format_nested_struct() {
        let out = roundtrip("(\n  inner: (\n    x: Integer,\n  ),\n)");
        assert!(out.contains("inner: (\n    x: Integer,\n  ),"));
    }

    // ========================================================
    // Enums
    // ========================================================

    // Enum definition appears after the root struct.
    #[test]
    fn format_enum_after_struct() {
        let out = roundtrip("(\n  status: Status,\n)\nenum Status { Active, Inactive }");
        let struct_end = out.find(')').unwrap();
        let enum_start = out.find("enum Status").unwrap();
        assert!(enum_start > struct_end);
    }

    // Enum variants are present.
    #[test]
    fn format_enum_has_variants() {
        let out = roundtrip("(\n  status: Status,\n)\nenum Status { Active, Inactive }");
        assert!(out.contains("Active"));
        assert!(out.contains("Inactive"));
    }

    // Enum with data variant.
    #[test]
    fn format_enum_with_data_variant() {
        let out = roundtrip("(\n  effect: Effect,\n)\nenum Effect { Damage(Integer), Draw }");
        assert!(out.contains("Damage(Integer)"));
        assert!(out.contains("Draw"));
    }

    // Multiple enums sorted by name.
    #[test]
    fn format_multiple_enums_sorted() {
        let out = roundtrip("(\n  a: B,\n  b: A,\n)\nenum B { X }\nenum A { Y }");
        let a_pos = out.find("enum A").unwrap();
        let b_pos = out.find("enum B").unwrap();
        assert!(a_pos < b_pos);
    }

    // ========================================================
    // Roundtrip — output parses back successfully
    // ========================================================

    // Roundtripped output parses without error.
    #[test]
    fn format_roundtrip_parses() {
        let source = "(\n  name: String,\n  tags: [Integer],\n  pos: (Float, Float),\n)\nenum Status { Active, Inactive }";
        let formatted = roundtrip(source);
        parse_schema(&formatted).expect("formatted output should parse");
    }

    // Complex schema roundtrips.
    #[test]
    fn format_complex_roundtrip() {
        let source = "(\n  name: String,\n  health: Integer,\n  stats: {String: Integer},\n  effect: Effect,\n  window: (\n    width: Integer,\n    height: Integer,\n  ),\n)\nenum Effect { Damage(Integer), Heal(Integer), Draw }";
        let formatted = roundtrip(source);
        parse_schema(&formatted).expect("formatted output should parse");
    }
}
