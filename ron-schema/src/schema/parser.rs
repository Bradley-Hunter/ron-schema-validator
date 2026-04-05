/*************************
 * Author: Bradley Hunter
 */

use crate::span::{Position, Span, Spanned};
use crate::error::{SchemaParseError, SchemaErrorKind};
use super::{SchemaType, FieldDef, StructDef, EnumDef, HashSet, Schema, HashMap};

#[derive(Debug)]
struct Parser<'a> {
    source: &'a str,
    bytes: &'a [u8],
    offset: usize,
    line: usize,
    column: usize,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, bytes: source.as_bytes(), offset: 0, line: 1, column: 1 }
    }

    fn position(&self) -> Position {
        Position { offset: self.offset, line: self.line, column: self.column }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn advance(&mut self) {
        if let Some(byte) = self.peek() {
            if byte == b'\n'{
                self.column = 1;
                self.line += 1;
            } else {
                self.column += 1;
            }
            self.offset += 1;
        } 
    }

    fn skip_whitespace(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r') => self.advance(),
                Some(b'/') if self.bytes.get(self.offset + 1) == Some(&b'/') => {
                    while self.peek().is_some_and(|b| b != b'\n') {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn expect_char(&mut self, expected: u8) -> Result<(), SchemaParseError> {
        let start = self.position();
        match self.peek() {
            Some(b) if b == expected => {
                self.advance();
                Ok(())
            },
            Some(b) => {
                self.advance();
                let end = self.position();
                Err(SchemaParseError { 
                    span: Span { 
                        start, 
                        end 
                    }, 
                    kind: SchemaErrorKind::UnexpectedToken { 
                        expected: format!("'{}'", expected as char), 
                        found: format!("'{}'", b as char) 
                    } 
                })
            },
            None => {
                Err(SchemaParseError { 
                    span: Span { 
                        start, 
                        end: start 
                    }, 
                    kind: SchemaErrorKind::UnexpectedToken { 
                        expected: format!("'{}'", expected as char), 
                        found: "end of input".to_string() 
                    } 
                })
            }
        }
    }

    fn parse_identifier(&mut self) -> Result<Spanned<String>, SchemaParseError> {
        let start = self.position();

        // Check for valid identifier start
        match self.peek() {
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => {},
            Some(b) => {
                self.advance();
                let end = self.position();
                return Err(SchemaParseError {
                    span: Span { start, end },
                    kind: SchemaErrorKind::UnexpectedToken {
                        expected: "identifier".to_string(),
                        found: format!("'{}'", b as char),
                    },
                });
            },
            None => {
                return Err(SchemaParseError {
                    span: Span { start, end: start },
                    kind: SchemaErrorKind::UnexpectedToken {
                        expected: "identifier".to_string(),
                        found: "end of input".to_string(),
                    },
                });
            },
        }

        // Consume all identifier continuation characters
        while self.peek().is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_') {
            self.advance();
        }

        // Slice out the identifier text
        let end = self.position();
        Ok(Spanned {
            value: self.source[start.offset..end.offset].to_string(),
            span: Span { start, end },
        })
    }

    fn parse_type(&mut self) -> Result<Spanned<SchemaType>, SchemaParseError> {
        self.skip_whitespace();
        let start = self.position();

        match self.peek() {
            Some(b'[') => {
                // List: consume '[', parse inner type, expect ']'
                self.advance();
                self.skip_whitespace();
                let inner = self.parse_type()?;
                self.skip_whitespace();
                self.expect_char(b']')?;
                let end = self.position();
                Ok(Spanned {
                    value: SchemaType::List(Box::new(inner.value)),
                    span: Span { start, end },
                })
            }
            Some(b'(') => {
                let struct_def = self.parse_struct()?;
                let end = self.position();
                Ok(Spanned {
                    value: SchemaType::Struct(struct_def),
                    span: Span { start, end },
                })
            }
            Some(b) if b.is_ascii_alphabetic() => {
                // Identifier: could be primitive, Option, or EnumRef
                let id = self.parse_identifier()?;
                match id.value.as_str() {
                    "String" => Ok(Spanned { value: SchemaType::String, span: id.span }),
                    "Integer" => Ok(Spanned { value: SchemaType::Integer, span: id.span }),
                    "Float" => Ok(Spanned { value: SchemaType::Float, span: id.span }),
                    "Bool" => Ok(Spanned { value: SchemaType::Bool, span: id.span }),
                    "Option" => {
                        // expect '(', parse inner type, expect ')'
                        self.skip_whitespace();
                        self.expect_char(b'(')?;
                        self.skip_whitespace();
                        let inner = self.parse_type()?;
                        self.skip_whitespace();
                        self.expect_char(b')')?;
                        let end = self.position();
                        Ok(Spanned {
                            value: SchemaType::Option(Box::new(inner.value)),
                            span: Span { start, end },
                        })
                    }
                    _ => Ok(Spanned { value: SchemaType::EnumRef(id.value), span: id.span }),
                }
            }
            Some(b) => {
                // Error: unexpected character
                self.advance();
                let end = self.position();
                Err(SchemaParseError {
                    span: Span { start, end },
                    kind: SchemaErrorKind::UnexpectedToken {
                        expected: "type".to_string(),
                        found: format!("'{}'", b as char),
                    },
                })
            }
            None => {
                Err(SchemaParseError {
                    span: Span { start, end: start },
                    kind: SchemaErrorKind::UnexpectedToken {
                        expected: "type".to_string(),
                        found: "end of input".to_string(),
                    },
                })
            }
        }
    }

    fn parse_field(&mut self) -> Result<FieldDef, SchemaParseError> {
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(b':')?;
        self.skip_whitespace();
        let type_ = self.parse_type()?;
        Ok(FieldDef{
            name,
            type_
        })
    }

    fn parse_struct(&mut self) -> Result<StructDef, SchemaParseError> {
        self.skip_whitespace();
        self.expect_char(b'(')?;
        let mut fields: Vec<FieldDef> = Vec::new();
        loop {
            self.skip_whitespace();
            if let Some(byte) = self.peek() {
                if byte == b')' {
                    break ;
                } 
                let field = self.parse_field()?;
                fields.push(field);
                self.skip_whitespace();
                if self.peek() == Some(b',') {
                    self.advance();
                }
            } else {
                return Err(SchemaParseError {
                    span: Span { start: self.position(), end: self.position() },
                    kind: SchemaErrorKind::UnexpectedToken { expected: ")".to_string(), found: "end of file".to_string() }
                });
            }
        }
        self.expect_char(b')')?;
        Ok(StructDef { fields })
    }

    fn parse_enum_def(&mut self) -> Result<EnumDef, SchemaParseError> {
        self.skip_whitespace();
        let keyword = self.parse_identifier()?;
        if keyword.value != "enum" {
            return Err(SchemaParseError {
                span: keyword.span,
                kind: SchemaErrorKind::UnexpectedToken {
                    expected: "\"enum\"".to_string(),
                    found: keyword.value,
                },
            });
        }
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(b'{')?;
        let mut variants = HashSet::new();
        loop {
            self.skip_whitespace();
            if let Some(byte) = self.peek() {
                if byte == b'}' {
                    break ;
                } 
                let variant = self.parse_identifier()?;
                variants.insert(variant.value);
                self.skip_whitespace();
                if self.peek() == Some(b',') {
                    self.advance();
                }
            } else {
                return Err(SchemaParseError {
                    span: Span { start: self.position(), end: self.position() },
                    kind: SchemaErrorKind::UnexpectedToken { expected: "}".to_string(), found: "end of file".to_string() }
                });
            }
        }

        self.expect_char(b'}')?;
        Ok(EnumDef { name: name.value, variants })
    }
}

/// Parses a `.ronschema` source string into a [`Schema`].
///
/// # Errors
///
/// Returns a [`SchemaParseError`] if the source contains syntax errors,
/// duplicate definitions, or unresolved enum references.
pub fn parse_schema(source: &str) -> Result<Schema, SchemaParseError> {
    let mut parser = Parser::new(source);
    parser.skip_whitespace();

    let root = if parser.peek() == Some(b'(') {
        parser.parse_struct()?
    } else {
        StructDef { fields: Vec::new() }
    };

    let mut enums: HashMap<String, EnumDef> = HashMap::new();
    loop {
        parser.skip_whitespace();
        if parser.peek().is_none() {
            break;
        }
        let enum_def = parser.parse_enum_def()?;
        if let Some(old) = enums.insert(enum_def.name.clone(), enum_def) {
            return Err(SchemaParseError {
                span: Span { start: parser.position(), end: parser.position() },
                kind: SchemaErrorKind::DuplicateEnum { name: old.name },
            });
        }
    }

    verify_enum_refs(&root, &enums)?;

    Ok(Schema { root, enums })
}

fn verify_enum_refs(
    struct_def: &StructDef,
    enums: &HashMap<String, EnumDef>,
) -> Result<(), SchemaParseError> {
    for field in &struct_def.fields {
        verify_type_enum_refs(&field.type_, enums)?;
    }
    Ok(())
}

fn verify_type_enum_refs(
    spanned_type: &Spanned<SchemaType>,
    enums: &HashMap<String, EnumDef>,
) -> Result<(), SchemaParseError> {
    check_schema_type(&spanned_type.value, spanned_type.span, enums)
}

fn check_schema_type(
    schema_type: &SchemaType,
    span: Span,
    enums: &HashMap<String, EnumDef>,
) -> Result<(), SchemaParseError> {
    match schema_type {
        SchemaType::EnumRef(name) => {
            if !enums.contains_key(name) {
                return Err(SchemaParseError {
                    span,
                    kind: SchemaErrorKind::UnresolvedEnum { name: name.clone() },
                });
            }
        }
        SchemaType::Option(inner) | SchemaType::List(inner) => {
            check_schema_type(inner, span, enums)?;
        }
        SchemaType::Struct(struct_def) => {
            verify_enum_refs(struct_def, enums)?;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================
    // Helper: constructs a Parser for direct method testing
    // ========================================================

    fn parser(source: &str) -> Parser<'_> {
        Parser::new(source)
    }

    // ========================================================
    // peek() tests
    // ========================================================

    // Returns the current byte without advancing.
    #[test]
    fn peek_returns_current_byte() {
        let p = parser("abc");
        assert_eq!(p.peek(), Some(b'a'));
    }

    // Returns None when at end of input.
    #[test]
    fn peek_returns_none_at_end() {
        let p = parser("");
        assert_eq!(p.peek(), None);
    }

    // ========================================================
    // advance() tests
    // ========================================================

    // Moves to the next byte and increments column.
    #[test]
    fn advance_increments_offset_and_column() {
        let mut p = parser("ab");
        p.advance();
        assert_eq!(p.offset, 1);
        assert_eq!(p.column, 2);
        assert_eq!(p.peek(), Some(b'b'));
    }

    // Newline resets column to 1 and increments line.
    #[test]
    fn advance_past_newline_increments_line() {
        let mut p = parser("a\nb");
        p.advance(); // past 'a'
        p.advance(); // past '\n'
        assert_eq!(p.line, 2);
        assert_eq!(p.column, 1);
    }

    // Advancing at end of input is a no-op.
    #[test]
    fn advance_at_end_is_noop() {
        let mut p = parser("");
        p.advance();
        assert_eq!(p.offset, 0);
    }

    // ========================================================
    // position() tests
    // ========================================================

    // Initial position is offset 0, line 1, column 1.
    #[test]
    fn position_initial_state() {
        let p = parser("abc");
        let pos = p.position();
        assert_eq!(pos.offset, 0);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.column, 1);
    }

    // Position tracks correctly after advancing.
    #[test]
    fn position_after_advance() {
        let mut p = parser("ab\nc");
        p.advance(); // 'a'
        p.advance(); // 'b'
        p.advance(); // '\n'
        let pos = p.position();
        assert_eq!(pos.offset, 3);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.column, 1);
    }

    // ========================================================
    // skip_whitespace() tests
    // ========================================================

    // Skips spaces, tabs, and newlines.
    #[test]
    fn skip_whitespace_skips_spaces_tabs_newlines() {
        let mut p = parser("  \t\nabc");
        p.skip_whitespace();
        assert_eq!(p.peek(), Some(b'a'));
    }

    // Skips line comments.
    #[test]
    fn skip_whitespace_skips_line_comment() {
        let mut p = parser("// comment\nabc");
        p.skip_whitespace();
        assert_eq!(p.peek(), Some(b'a'));
    }

    // Skips whitespace after a comment.
    #[test]
    fn skip_whitespace_skips_comment_then_whitespace() {
        let mut p = parser("// comment\n  abc");
        p.skip_whitespace();
        assert_eq!(p.peek(), Some(b'a'));
    }

    // Does nothing when already on a non-whitespace character.
    #[test]
    fn skip_whitespace_noop_on_nonwhitespace() {
        let mut p = parser("abc");
        p.skip_whitespace();
        assert_eq!(p.offset, 0);
    }

    // ========================================================
    // expect_char() tests
    // ========================================================

    // Consumes the expected character and returns Ok.
    #[test]
    fn expect_char_consumes_matching_byte() {
        let mut p = parser("(abc");
        assert!(p.expect_char(b'(').is_ok());
        assert_eq!(p.peek(), Some(b'a'));
    }

    // Returns error when character doesn't match.
    #[test]
    fn expect_char_error_on_mismatch() {
        let mut p = parser("abc");
        let err = p.expect_char(b'(').unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::UnexpectedToken { .. }));
    }

    // Returns error at end of input.
    #[test]
    fn expect_char_error_at_end_of_input() {
        let mut p = parser("");
        let err = p.expect_char(b'(').unwrap_err();
        match err.kind {
            SchemaErrorKind::UnexpectedToken { found, .. } => {
                assert_eq!(found, "end of input");
            }
            other => panic!("expected UnexpectedToken, got {:?}", other),
        }
    }

    // ========================================================
    // parse_identifier() tests
    // ========================================================

    // Reads a simple alphabetic identifier.
    #[test]
    fn parse_identifier_reads_alpha() {
        let mut p = parser("name:");
        let id = p.parse_identifier().unwrap();
        assert_eq!(id.value, "name");
    }

    // Reads an identifier with underscores.
    #[test]
    fn parse_identifier_reads_snake_case() {
        let mut p = parser("field_name:");
        let id = p.parse_identifier().unwrap();
        assert_eq!(id.value, "field_name");
    }

    // Reads an identifier with digits.
    #[test]
    fn parse_identifier_reads_alphanumeric() {
        let mut p = parser("cost2:");
        let id = p.parse_identifier().unwrap();
        assert_eq!(id.value, "cost2");
    }

    // Reads a PascalCase identifier (for types/enums).
    #[test]
    fn parse_identifier_reads_pascal_case() {
        let mut p = parser("CardType ");
        let id = p.parse_identifier().unwrap();
        assert_eq!(id.value, "CardType");
    }

    // Stops at non-identifier characters.
    #[test]
    fn parse_identifier_stops_at_delimiter() {
        let mut p = parser("name: String");
        let id = p.parse_identifier().unwrap();
        assert_eq!(id.value, "name");
        assert_eq!(p.peek(), Some(b':'));
    }

    // Records correct span for the identifier.
    #[test]
    fn parse_identifier_span_is_correct() {
        let mut p = parser("name:");
        let id = p.parse_identifier().unwrap();
        assert_eq!(id.span.start.offset, 0);
        assert_eq!(id.span.end.offset, 4);
    }

    // Error when starting with a digit.
    #[test]
    fn parse_identifier_error_on_digit_start() {
        let mut p = parser("42abc");
        assert!(p.parse_identifier().is_err());
    }

    // Error at end of input.
    #[test]
    fn parse_identifier_error_at_end_of_input() {
        let mut p = parser("");
        assert!(p.parse_identifier().is_err());
    }

    // ========================================================
    // parse_type() tests
    // ========================================================

    // Parses "String" as SchemaType::String.
    #[test]
    fn parse_type_string() {
        let mut p = parser("String");
        let t = p.parse_type().unwrap();
        assert_eq!(t.value, SchemaType::String);
    }

    // Parses "Integer" as SchemaType::Integer.
    #[test]
    fn parse_type_integer() {
        let mut p = parser("Integer");
        let t = p.parse_type().unwrap();
        assert_eq!(t.value, SchemaType::Integer);
    }

    // Parses "Float" as SchemaType::Float.
    #[test]
    fn parse_type_float() {
        let mut p = parser("Float");
        let t = p.parse_type().unwrap();
        assert_eq!(t.value, SchemaType::Float);
    }

    // Parses "Bool" as SchemaType::Bool.
    #[test]
    fn parse_type_bool() {
        let mut p = parser("Bool");
        let t = p.parse_type().unwrap();
        assert_eq!(t.value, SchemaType::Bool);
    }

    // Parses "[String]" as a List wrapping String.
    #[test]
    fn parse_type_list() {
        let mut p = parser("[String]");
        let t = p.parse_type().unwrap();
        assert_eq!(t.value, SchemaType::List(Box::new(SchemaType::String)));
    }

    // Parses "Option(Integer)" as an Option wrapping Integer.
    #[test]
    fn parse_type_option() {
        let mut p = parser("Option(Integer)");
        let t = p.parse_type().unwrap();
        assert_eq!(t.value, SchemaType::Option(Box::new(SchemaType::Integer)));
    }

    // Parses an unknown PascalCase name as an EnumRef.
    #[test]
    fn parse_type_enum_ref() {
        let mut p = parser("Faction");
        let t = p.parse_type().unwrap();
        assert_eq!(t.value, SchemaType::EnumRef("Faction".to_string()));
    }

    // Parses nested composites: [Option(String)].
    #[test]
    fn parse_type_nested_list_of_option() {
        let mut p = parser("[Option(String)]");
        let t = p.parse_type().unwrap();
        assert_eq!(
            t.value,
            SchemaType::List(Box::new(SchemaType::Option(Box::new(SchemaType::String))))
        );
    }

    // Parses an inline struct type.
    #[test]
    fn parse_type_inline_struct() {
        let mut p = parser("(\n  x: Integer,\n)");
        let t = p.parse_type().unwrap();
        if let SchemaType::Struct(s) = &t.value {
            assert_eq!(s.fields.len(), 1);
            assert_eq!(s.fields[0].name.value, "x");
        } else {
            panic!("expected SchemaType::Struct");
        }
    }

    // Error on unexpected token in type position.
    #[test]
    fn parse_type_error_on_unexpected_token() {
        let mut p = parser("42");
        let err = p.parse_type().unwrap_err();
        match err.kind {
            SchemaErrorKind::UnexpectedToken { expected, .. } => {
                assert_eq!(expected, "type");
            }
            other => panic!("expected UnexpectedToken, got {:?}", other),
        }
    }

    // ========================================================
    // parse_field() tests
    // ========================================================

    // Parses "name: String" into a FieldDef.
    #[test]
    fn parse_field_name_and_type() {
        let mut p = parser("name: String,");
        let f = p.parse_field().unwrap();
        assert_eq!(f.name.value, "name");
        assert_eq!(f.type_.value, SchemaType::String);
    }

    // Error when colon is missing.
    #[test]
    fn parse_field_error_missing_colon() {
        let mut p = parser("name String");
        let err = p.parse_field().unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::UnexpectedToken { .. }));
    }

    // ========================================================
    // parse_struct() tests
    // ========================================================

    // Parses an empty struct.
    #[test]
    fn parse_struct_empty() {
        let mut p = parser("()");
        let s = p.parse_struct().unwrap();
        assert!(s.fields.is_empty());
    }

    // Parses a struct with one field.
    #[test]
    fn parse_struct_single_field() {
        let mut p = parser("(\n  name: String,\n)");
        let s = p.parse_struct().unwrap();
        assert_eq!(s.fields.len(), 1);
        assert_eq!(s.fields[0].name.value, "name");
    }

    // Parses a struct with multiple fields.
    #[test]
    fn parse_struct_multiple_fields() {
        let mut p = parser("(\n  a: String,\n  b: Integer,\n)");
        let s = p.parse_struct().unwrap();
        assert_eq!(s.fields.len(), 2);
    }

    // Struct without trailing comma is valid.
    #[test]
    fn parse_struct_no_trailing_comma() {
        let mut p = parser("(\n  name: String\n)");
        let s = p.parse_struct().unwrap();
        assert_eq!(s.fields.len(), 1);
    }

    // Error on unclosed struct.
    #[test]
    fn parse_struct_error_on_unclosed() {
        let mut p = parser("(\n  name: String,\n");
        assert!(p.parse_struct().is_err());
    }

    // ========================================================
    // parse_enum_def() tests
    // ========================================================

    // Parses a simple enum definition.
    #[test]
    fn parse_enum_def_simple() {
        let mut p = parser("enum Dir { North, South }");
        let e = p.parse_enum_def().unwrap();
        assert_eq!(e.name, "Dir");
        assert_eq!(e.variants.len(), 2);
        assert!(e.variants.contains("North"));
        assert!(e.variants.contains("South"));
    }

    // Trailing comma in variant list is allowed.
    #[test]
    fn parse_enum_def_trailing_comma() {
        let mut p = parser("enum Dir { North, South, }");
        let e = p.parse_enum_def().unwrap();
        assert_eq!(e.variants.len(), 2);
    }

    // Single variant enum is valid.
    #[test]
    fn parse_enum_def_single_variant() {
        let mut p = parser("enum Single { Only }");
        let e = p.parse_enum_def().unwrap();
        assert_eq!(e.variants.len(), 1);
    }

    // Error when keyword is not "enum".
    #[test]
    fn parse_enum_def_error_wrong_keyword() {
        let mut p = parser("struct Dir { North }");
        let err = p.parse_enum_def().unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::UnexpectedToken { .. }));
    }

    // Error on unclosed enum.
    #[test]
    fn parse_enum_def_error_on_unclosed() {
        let mut p = parser("enum Dir { North, South");
        assert!(p.parse_enum_def().is_err());
    }

    // ========================================================
    // parse_schema() integration tests
    // ========================================================

    // Empty input produces an empty schema.
    #[test]
    fn schema_empty_input() {
        let schema = parse_schema("").unwrap();
        assert!(schema.root.fields.is_empty());
    }

    // Empty input produces no enums.
    #[test]
    fn schema_empty_input_no_enums() {
        let schema = parse_schema("").unwrap();
        assert!(schema.enums.is_empty());
    }

    // Root struct with enum ref resolves when enum is defined.
    #[test]
    fn schema_enum_ref_resolves() {
        let source = "(\n  faction: Faction,\n)\nenum Faction { Sentinels, Reavers }";
        let schema = parse_schema(source).unwrap();
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::EnumRef("Faction".to_string()));
    }

    // Multiple enum definitions are all stored.
    #[test]
    fn schema_multiple_enums_stored() {
        let source = "enum A { X }\nenum B { Y }";
        let schema = parse_schema(source).unwrap();
        assert_eq!(schema.enums.len(), 2);
    }

    // Comments before root struct are ignored.
    #[test]
    fn schema_comments_before_root() {
        let source = "// comment\n(\n  name: String,\n)";
        let schema = parse_schema(source).unwrap();
        assert_eq!(schema.root.fields.len(), 1);
    }

    // Inline comment after field is ignored.
    #[test]
    fn schema_inline_comment_after_field() {
        let source = "(\n  name: String, // a name\n)";
        let schema = parse_schema(source).unwrap();
        assert_eq!(schema.root.fields[0].name.value, "name");
    }

    // Unresolved enum ref is an error.
    #[test]
    fn schema_unresolved_enum_ref() {
        let err = parse_schema("(\n  f: Faction,\n)").unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::UnresolvedEnum { name: "Faction".to_string() });
    }

    // Unresolved enum ref inside Option is an error.
    #[test]
    fn schema_unresolved_enum_ref_in_option() {
        let err = parse_schema("(\n  t: Option(Timing),\n)").unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::UnresolvedEnum { name: "Timing".to_string() });
    }

    // Unresolved enum ref inside List is an error.
    #[test]
    fn schema_unresolved_enum_ref_in_list() {
        let err = parse_schema("(\n  t: [CardType],\n)").unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::UnresolvedEnum { name: "CardType".to_string() });
    }

    // Duplicate enum name is an error.
    #[test]
    fn schema_duplicate_enum_name() {
        let err = parse_schema("enum A { X }\nenum A { Y }").unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::DuplicateEnum { name: "A".to_string() });
    }
}