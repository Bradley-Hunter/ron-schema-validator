/*************************
 * Author: Bradley Hunter
 */

use crate::span::*;
use crate::error::{RonErrorKind, RonParseError};
use super::*;

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
        match self.peek() {
            Some(byte) => {
                if byte == b'\n'{
                    self.column = 1;
                    self.line += 1;
                } else {
                    self.column += 1;
                }
                self.offset += 1;
            },
            None => {}
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

    fn expect_char(&mut self, expected: u8) -> Result<(), RonParseError> {
        let start = self.position();
        match self.peek() {
            Some(b) if b == expected => {
                self.advance();
                Ok(())
            },
            Some(b) => {
                self.advance();
                let end = self.position();
                Err(RonParseError { 
                    span: Span { 
                        start, 
                        end 
                    }, 
                    kind: RonErrorKind::UnexpectedToken { 
                        expected: format!("'{}'", expected as char), 
                        found: format!("'{}'", b as char) 
                    } 
                })
            },
            None => {
                Err(RonParseError { 
                    span: Span { 
                        start, 
                        end: start 
                    }, 
                    kind: RonErrorKind::UnexpectedToken { 
                        expected: format!("'{}'", expected as char), 
                        found: "end of input".to_string() 
                    } 
                })
            }
        }
    }

    fn parse_identifier(&mut self) -> Result<Spanned<String>, RonParseError> {
        let start = self.position();

        // Check for valid identifier start
        match self.peek() {
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => {},
            Some(b) => {
                self.advance();
                let end = self.position();
                return Err(RonParseError {
                    span: Span { start, end },
                    kind: RonErrorKind::UnexpectedToken {
                        expected: "identifier".to_string(),
                        found: format!("'{}'", b as char),
                    },
                });
            },
            None => {
                return Err(RonParseError {
                    span: Span { start, end: start },
                    kind: RonErrorKind::UnexpectedToken {
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

    fn parse_value(&mut self) -> Result<Spanned<RonValue>, RonParseError> {
        self.skip_whitespace();
        let start = self.position();

        match self.peek() {
            Some(b'"') => {
                self.advance(); // skip opening quote
                let mut content = String::new();
                loop {
                    match self.peek() {
                        Some(b'"') => {
                            self.advance(); // skip closing quote
                            break;
                        }
                        // b'\\' is a single backslash byte — Rust escapes it in source code.
                        // We detect RON escape sequences (like \n, \t, \") by first matching
                        // the backslash, then checking the next character to decide what to emit.
                        Some(b'\\') => {
                            self.advance(); // skip the backslash
                            match self.peek() {
                                Some(b'n') => { content.push('\n'); self.advance(); }
                                Some(b't') => { content.push('\t'); self.advance(); }
                                Some(b'\\') => { content.push('\\'); self.advance(); }
                                Some(b'"') => { content.push('"'); self.advance(); }
                                Some(b) => { content.push(b as char); self.advance(); }
                                None => {
                                    return Err(RonParseError {
                                        span: Span { start, end: self.position() },
                                        kind: RonErrorKind::UnterminatedString,
                                    });
                                }
                            }
                        }
                        Some(b) => {
                            content.push(b as char);
                            self.advance();
                        }
                        None => {
                            return Err(RonParseError {
                                span: Span { start, end: self.position() },
                                kind: RonErrorKind::UnterminatedString,
                            });
                        }
                    }
                }
                let end = self.position();
                Ok(Spanned {
                    value: RonValue::String(content),
                    span: Span { start, end },
                })
            },
            Some(b) if b.is_ascii_digit() || b == b'-' => {
                if b == b'-' {
                    self.advance();
                }

                let mut has_dot = false;
        
                loop {
                    match self.peek() {
                        Some(b) if b.is_ascii_digit() => {self.advance();},
                        Some(b'.') if !has_dot => {
                            has_dot = true;
                            self.advance();
                        },
                        Some(_) => {break;}
                        None => {break;}
                    }
                }

                let end = self.position();
                let number_str = &self.source[start.offset..end.offset];
                if has_dot {
                    let num_float = number_str.parse::<f64>();
                    if let Ok(num) = num_float {
                        return Ok(Spanned {
                            value: RonValue::Float(num),
                            span: Span { start, end },
                        });
                    } else {
                        return Err(RonParseError { 
                            span: Span { start, end }, 
                            kind: RonErrorKind::InvalidNumber { text: number_str.to_string() } 
                        });
                    }
                } else {
                    let num_int = number_str.parse::<i64>();
                    if let Ok(num) = num_int {
                        return Ok(Spanned {
                            value: RonValue::Integer(num),
                            span: Span { start, end },
                        });
                    } else {
                        return Err(RonParseError { 
                            span: Span { start, end }, 
                            kind: RonErrorKind::InvalidNumber { text: number_str.to_string() } 
                        });
                    }
                }
            },
            Some(b) if b.is_ascii_alphabetic() => {
                let identifier = self.parse_identifier()?;
                let word = identifier.value.as_str();
                let identifier_span = identifier.span;
                match word {
                    "true" => {
                        return Ok(Spanned { value: RonValue::Bool(true), span: identifier_span });
                    },
                    "false" => {
                        return Ok(Spanned { value: RonValue::Bool(false), span: identifier_span });
                    }
                    "None" => {
                        return Ok(Spanned { value: RonValue::Option(None), span: identifier_span });
                    }
                    "Some" => {
                        self.skip_whitespace();
                        self.expect_char(b'(')?;
                        let inner = self.parse_value()?;
                        self.expect_char(b')')?;
                        return Ok(Spanned { 
                            value: RonValue::Option(Some(Box::new(inner))), 
                            span: Span { start, end: self.position() } 
                        });
                    }
                    _ => {
                        return Ok(Spanned { 
                            value: RonValue::Identifier(word.to_string()), 
                            span: identifier_span 
                        });
                    }
                }
            },
            Some(b'[') => {
                self.advance();
                let mut elements = Vec::new();
                loop {
                    self.skip_whitespace();
                    if let Some(b']') = self.peek() {
                        break;
                    }
                    let value = self.parse_value()?;
                    elements.push(value);
                    self.skip_whitespace();
                    if let Some(b',') = self.peek() {
                        self.advance();
                    }
                }
                self.expect_char(b']')?;
                return Ok(Spanned { 
                    value: RonValue::List(elements), 
                    span: Span { start, end: self.position() } 
                });
            },
            Some(b'(') => {
                self.advance();
                let mut fields: Vec<(Spanned<String>, Spanned<RonValue>)> = Vec::new();
                loop {
                    self.skip_whitespace();
                    if let Some(b')') = self.peek() {
                        break;
                    }
                    let field = self.parse_identifier()?;
                    self.skip_whitespace();
                    self.expect_char(b':')?;
                    self.skip_whitespace();
                    let value = self.parse_value()?;
                    fields.push((field, value));
                    self.skip_whitespace();
                    match self.peek() {
                        Some(b',') => self.advance(),
                        Some(_) => {}
                        None => {
                            return Err(RonParseError { 
                                span: Span { start, end: self.position() } , 
                                kind: RonErrorKind::UnexpectedToken { 
                                    expected: "character".to_string(), 
                                    found: "end of file".to_string() } 
                                });
                        }
                    }
                }
                let close_span_start = self.position();
                self.expect_char(b')')?;
                let close_span = Span{ start: close_span_start, end: self.position() };
                return Ok(Spanned { 
                    value: RonValue::Struct(RonStruct { fields, close_span }), 
                    span: Span { start, end: self.position() } 
                })
            }
            Some(b) => {
                self.advance();
                let end = self.position();
                return Err(RonParseError { 
                    span: Span { start, end }, 
                    kind: RonErrorKind::UnexpectedToken { 
                        expected: "value".to_string(), 
                        found: format!("{}", b as char) 
                    } 
                });
            },
            None => {
                return Err(RonParseError { 
                    span: Span { start, end: start }, 
                    kind: RonErrorKind::UnexpectedToken { 
                        expected: "value".to_string(), 
                        found: "end of file".to_string() 
                    } 
                });
            }
        }
    }
}

/// Parses a RON data source string into a spanned value tree.
///
/// Returns a [`RonParseError`] if the source contains syntax errors.
pub fn parse_ron(source: &str) -> Result<Spanned<RonValue>, RonParseError> {
    let mut parser = Parser::new(source);
    parser.parse_value()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser(source: &str) -> Parser<'_> {
        Parser::new(source)
    }

    // ========================================================
    // parse_value() — string parsing
    // ========================================================

    // Parses a simple quoted string.
    #[test]
    fn string_simple() {
        let mut p = parser("\"hello\"");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::String("hello".to_string()));
    }

    // Parses an empty string.
    #[test]
    fn string_empty() {
        let mut p = parser("\"\"");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::String("".to_string()));
    }

    // Parses a string with spaces.
    #[test]
    fn string_with_spaces() {
        let mut p = parser("\"Ashborn Hound\"");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::String("Ashborn Hound".to_string()));
    }

    // Escape sequence: \" becomes a literal quote.
    #[test]
    fn string_escaped_quote() {
        let mut p = parser("\"say \\\"hi\\\"\"");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::String("say \"hi\"".to_string()));
    }

    // Escape sequence: \\ becomes a single backslash.
    #[test]
    fn string_escaped_backslash() {
        let mut p = parser("\"a\\\\b\"");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::String("a\\b".to_string()));
    }

    // Escape sequence: \n becomes a newline.
    #[test]
    fn string_escaped_newline() {
        let mut p = parser("\"line1\\nline2\"");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::String("line1\nline2".to_string()));
    }

    // Escape sequence: \t becomes a tab.
    #[test]
    fn string_escaped_tab() {
        let mut p = parser("\"col1\\tcol2\"");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::String("col1\tcol2".to_string()));
    }

    // Unterminated string is an error.
    #[test]
    fn string_unterminated() {
        let mut p = parser("\"hello");
        let err = p.parse_value().unwrap_err();
        assert_eq!(err.kind, RonErrorKind::UnterminatedString);
    }

    // ========================================================
    // parse_value() — integer parsing
    // ========================================================

    // Parses a positive integer.
    #[test]
    fn integer_positive() {
        let mut p = parser("42");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Integer(42));
    }

    // Parses zero.
    #[test]
    fn integer_zero() {
        let mut p = parser("0");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Integer(0));
    }

    // Parses a negative integer.
    #[test]
    fn integer_negative() {
        let mut p = parser("-7");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Integer(-7));
    }

    // ========================================================
    // parse_value() — float parsing
    // ========================================================

    // Parses a simple float.
    #[test]
    fn float_simple() {
        let mut p = parser("3.14");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Float(3.14));
    }

    // Parses a negative float.
    #[test]
    fn float_negative() {
        let mut p = parser("-0.5");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Float(-0.5));
    }

    // Parses 1.0 as a float, not an integer.
    #[test]
    fn float_one_point_zero() {
        let mut p = parser("1.0");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Float(1.0));
    }

    // ========================================================
    // parse_value() — boolean parsing
    // ========================================================

    // Parses "true" as Bool(true).
    #[test]
    fn bool_true() {
        let mut p = parser("true");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Bool(true));
    }

    // Parses "false" as Bool(false).
    #[test]
    fn bool_false() {
        let mut p = parser("false");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Bool(false));
    }

    // ========================================================
    // parse_value() — option parsing
    // ========================================================

    // Parses "None" as Option(None).
    #[test]
    fn option_none() {
        let mut p = parser("None");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Option(None));
    }

    // Parses "Some(5)" as Option(Some(Integer(5))).
    #[test]
    fn option_some_integer() {
        let mut p = parser("Some(5)");
        let v = p.parse_value().unwrap();
        if let RonValue::Option(Some(inner)) = &v.value {
            assert_eq!(inner.value, RonValue::Integer(5));
        } else {
            panic!("expected Option(Some(...))");
        }
    }

    // Parses "Some(\"hi\")" as Option(Some(String)).
    #[test]
    fn option_some_string() {
        let mut p = parser("Some(\"hi\")");
        let v = p.parse_value().unwrap();
        if let RonValue::Option(Some(inner)) = &v.value {
            assert_eq!(inner.value, RonValue::String("hi".to_string()));
        } else {
            panic!("expected Option(Some(...))");
        }
    }

    // ========================================================
    // parse_value() — identifier parsing
    // ========================================================

    // Bare identifier is parsed as Identifier.
    #[test]
    fn identifier_bare() {
        let mut p = parser("Creature");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Identifier("Creature".to_string()));
    }

    // Another bare identifier.
    #[test]
    fn identifier_another() {
        let mut p = parser("Sentinels");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Identifier("Sentinels".to_string()));
    }

    // ========================================================
    // parse_value() — list parsing
    // ========================================================

    // Parses an empty list.
    #[test]
    fn list_empty() {
        let mut p = parser("[]");
        let v = p.parse_value().unwrap();
        if let RonValue::List(elems) = &v.value {
            assert!(elems.is_empty());
        } else {
            panic!("expected List");
        }
    }

    // Parses a list with one element.
    #[test]
    fn list_single_element() {
        let mut p = parser("[Creature]");
        let v = p.parse_value().unwrap();
        if let RonValue::List(elems) = &v.value {
            assert_eq!(elems.len(), 1);
            assert_eq!(elems[0].value, RonValue::Identifier("Creature".to_string()));
        } else {
            panic!("expected List");
        }
    }

    // Parses a list with multiple elements.
    #[test]
    fn list_multiple_elements() {
        let mut p = parser("[Creature, Trap, Artifact]");
        let v = p.parse_value().unwrap();
        if let RonValue::List(elems) = &v.value {
            assert_eq!(elems.len(), 3);
        } else {
            panic!("expected List");
        }
    }

    // Trailing comma in list is allowed.
    #[test]
    fn list_trailing_comma() {
        let mut p = parser("[Creature, Trap,]");
        let v = p.parse_value().unwrap();
        if let RonValue::List(elems) = &v.value {
            assert_eq!(elems.len(), 2);
        } else {
            panic!("expected List");
        }
    }

    // List of strings.
    #[test]
    fn list_of_strings() {
        let mut p = parser("[\"Vigilance\", \"Haste\"]");
        let v = p.parse_value().unwrap();
        if let RonValue::List(elems) = &v.value {
            assert_eq!(elems.len(), 2);
            assert_eq!(elems[0].value, RonValue::String("Vigilance".to_string()));
            assert_eq!(elems[1].value, RonValue::String("Haste".to_string()));
        } else {
            panic!("expected List");
        }
    }

    // ========================================================
    // parse_value() — struct parsing
    // ========================================================

    // Parses an empty struct.
    #[test]
    fn struct_empty() {
        let mut p = parser("()");
        let v = p.parse_value().unwrap();
        if let RonValue::Struct(s) = &v.value {
            assert!(s.fields.is_empty());
        } else {
            panic!("expected Struct");
        }
    }

    // Parses a struct with one field.
    #[test]
    fn struct_single_field() {
        let mut p = parser("(name: \"Ashborn Hound\")");
        let v = p.parse_value().unwrap();
        if let RonValue::Struct(s) = &v.value {
            assert_eq!(s.fields.len(), 1);
            assert_eq!(s.fields[0].0.value, "name");
            assert_eq!(s.fields[0].1.value, RonValue::String("Ashborn Hound".to_string()));
        } else {
            panic!("expected Struct");
        }
    }

    // Parses a struct with multiple fields.
    #[test]
    fn struct_multiple_fields() {
        let mut p = parser("(name: \"foo\", age: 5)");
        let v = p.parse_value().unwrap();
        if let RonValue::Struct(s) = &v.value {
            assert_eq!(s.fields.len(), 2);
        } else {
            panic!("expected Struct");
        }
    }

    // Trailing comma in struct is allowed.
    #[test]
    fn struct_trailing_comma() {
        let mut p = parser("(name: \"foo\",)");
        let v = p.parse_value().unwrap();
        if let RonValue::Struct(s) = &v.value {
            assert_eq!(s.fields.len(), 1);
        } else {
            panic!("expected Struct");
        }
    }

    // Struct captures close_span for the closing paren.
    #[test]
    fn struct_close_span_captured() {
        let mut p = parser("(x: 1)");
        let v = p.parse_value().unwrap();
        if let RonValue::Struct(s) = &v.value {
            assert_eq!(s.close_span.start.offset, 5);
            assert_eq!(s.close_span.end.offset, 6);
        } else {
            panic!("expected Struct");
        }
    }

    // Nested struct.
    #[test]
    fn struct_nested() {
        let mut p = parser("(cost: (generic: 2, sigil: 1))");
        let v = p.parse_value().unwrap();
        if let RonValue::Struct(s) = &v.value {
            assert_eq!(s.fields.len(), 1);
            assert_eq!(s.fields[0].0.value, "cost");
            if let RonValue::Struct(inner) = &s.fields[0].1.value {
                assert_eq!(inner.fields.len(), 2);
            } else {
                panic!("expected nested Struct");
            }
        } else {
            panic!("expected Struct");
        }
    }

    // ========================================================
    // parse_value() — whitespace and comments
    // ========================================================

    // Leading whitespace is skipped.
    #[test]
    fn whitespace_leading() {
        let mut p = parser("  42");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Integer(42));
    }

    // Comments are skipped.
    #[test]
    fn comment_before_value() {
        let mut p = parser("// comment\n42");
        let v = p.parse_value().unwrap();
        assert_eq!(v.value, RonValue::Integer(42));
    }

    // ========================================================
    // parse_value() — span accuracy
    // ========================================================

    // Span start is after whitespace, not before.
    #[test]
    fn span_starts_after_whitespace() {
        let mut p = parser("  42");
        let v = p.parse_value().unwrap();
        assert_eq!(v.span.start.offset, 2);
    }

    // Span covers the full value.
    #[test]
    fn span_covers_string() {
        let mut p = parser("\"hello\"");
        let v = p.parse_value().unwrap();
        assert_eq!(v.span.start.offset, 0);
        assert_eq!(v.span.end.offset, 7);
    }

    // ========================================================
    // parse_value() — error cases
    // ========================================================

    // Empty input is an error.
    #[test]
    fn error_empty_input() {
        let mut p = parser("");
        let err = p.parse_value().unwrap_err();
        match err.kind {
            RonErrorKind::UnexpectedToken { found, .. } => {
                assert_eq!(found, "end of file");
            }
            other => panic!("expected UnexpectedToken, got {:?}", other),
        }
    }

    // Unexpected character is an error.
    #[test]
    fn error_unexpected_char() {
        let mut p = parser("@");
        assert!(p.parse_value().is_err());
    }

    // ========================================================
    // parse_ron() integration tests
    // ========================================================

    // Parses a complete card-like struct.
    #[test]
    fn ron_full_struct() {
        let source = r#"(
            name: "Ashborn Hound",
            card_types: [Creature],
            legendary: false,
            power: Some(1),
            toughness: None,
            keywords: [],
            flavor_text: "placeholder",
        )"#;
        let v = parse_ron(source).unwrap();
        if let RonValue::Struct(s) = &v.value {
            assert_eq!(s.fields.len(), 7);
            assert_eq!(s.fields[0].0.value, "name");
            assert_eq!(s.fields[0].1.value, RonValue::String("Ashborn Hound".to_string()));
        } else {
            panic!("expected Struct");
        }
    }
}