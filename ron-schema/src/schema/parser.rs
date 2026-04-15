/*************************
 * Author: Bradley Hunter
 */

use crate::span::{Position, Span, Spanned};
use crate::error::{SchemaParseError, SchemaErrorKind};
use crate::ron::RonValue;
use crate::ron::parser::Parser as RonParser;
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

    #[allow(clippy::too_many_lines)]
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
            Some(b'{') => {
                // Map: consume '{', parse key type, expect ':', parse value type, expect '}'
                self.advance();
                self.skip_whitespace();
                let key_type = self.parse_type()?;
                // Validate key type is String, Integer, or EnumRef
                match &key_type.value {
                    SchemaType::String | SchemaType::Integer | SchemaType::EnumRef(_) => {}
                    _ => {
                        return Err(SchemaParseError {
                            span: key_type.span,
                            kind: SchemaErrorKind::InvalidMapKeyType {
                                found: format!("{:?}", key_type.value),
                            },
                        });
                    }
                }
                self.skip_whitespace();
                self.expect_char(b':')?;
                self.skip_whitespace();
                let value_type = self.parse_type()?;
                self.skip_whitespace();
                self.expect_char(b'}')?;
                let end = self.position();
                Ok(Spanned {
                    value: SchemaType::Map(Box::new(key_type.value), Box::new(value_type.value)),
                    span: Span { start, end },
                })
            }
            Some(b'(') => {
                // Disambiguate struct vs tuple:
                // Save position, consume '(', skip whitespace.
                // If ')' → empty struct. If identifier followed by ':' → struct.
                // Otherwise → tuple (comma-separated types).
                let saved = (self.offset, self.line, self.column);
                self.advance(); // consume '('
                self.skip_whitespace();

                let is_struct = if self.peek() == Some(b')') {
                    true // empty parens → treat as empty struct
                } else {
                    // Try to determine if this is name: Type (struct) or Type, Type (tuple)
                    let probe_pos = (self.offset, self.line, self.column);
                    let is_field = if let Ok(_id) = self.parse_identifier() {
                        self.skip_whitespace();
                        
                        self.peek() == Some(b':')
                    } else {
                        false
                    };
                    // Rewind to after '('
                    self.offset = probe_pos.0;
                    self.line = probe_pos.1;
                    self.column = probe_pos.2;
                    is_field
                };

                // Rewind to before '(' and parse as struct or tuple
                self.offset = saved.0;
                self.line = saved.1;
                self.column = saved.2;

                if is_struct {
                    let struct_def = self.parse_struct()?;
                    let end = self.position();
                    Ok(Spanned {
                        value: SchemaType::Struct(struct_def),
                        span: Span { start, end },
                    })
                } else {
                    let types = self.parse_tuple_type()?;
                    let end = self.position();
                    Ok(Spanned {
                        value: SchemaType::Tuple(types),
                        span: Span { start, end },
                    })
                }
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
        self.skip_whitespace();

        // Parse optional default value: `= <value>`
        let default = if self.peek() == Some(b'=') {
            self.advance(); // skip '='
            self.skip_whitespace();
            let mut ron_parser = RonParser::new_at(
                self.source,
                self.offset,
                self.position(),
            );
            let value = ron_parser.parse_single_value().map_err(|e| {
                SchemaParseError {
                    span: e.span,
                    kind: SchemaErrorKind::UnexpectedToken {
                        expected: "default value".to_string(),
                        found: format!("{:?}", e.kind),
                    },
                }
            })?;
            // Advance the schema parser past the value the RON parser consumed
            let bytes_consumed = ron_parser.current_offset() - self.offset;
            for _ in 0..bytes_consumed {
                self.advance();
            }
            Some(value)
        } else {
            None
        };

        Ok(FieldDef{
            name,
            type_,
            default,
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

    /// Parses `(Type, Type, ...)` as a tuple type.
    fn parse_tuple_type(&mut self) -> Result<Vec<SchemaType>, SchemaParseError> {
        self.skip_whitespace();
        self.expect_char(b'(')?;
        let mut types = Vec::new();
        loop {
            self.skip_whitespace();
            if self.peek() == Some(b')') {
                break;
            }
            let t = self.parse_type()?;
            types.push(t.value);
            self.skip_whitespace();
            if self.peek() == Some(b',') {
                self.advance();
            }
        }
        self.expect_char(b')')?;
        Ok(types)
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
        let mut variants = HashMap::new();
        loop {
            self.skip_whitespace();
            if let Some(byte) = self.peek() {
                if byte == b'}' {
                    break;
                }
                let variant = self.parse_identifier()?;
                // Check for associated data: Variant(Type)
                self.skip_whitespace();
                let data_type = if self.peek() == Some(b'(') {
                    self.advance(); // consume '('
                    self.skip_whitespace();
                    let t = self.parse_type()?;
                    self.skip_whitespace();
                    self.expect_char(b')')?;
                    Some(t.value)
                } else {
                    None
                };
                variants.insert(variant.value, data_type);
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

    /// Parses `type Name = <type>` — assumes the "type" keyword has already been confirmed.
    fn parse_alias_def(&mut self) -> Result<(String, Spanned<SchemaType>), SchemaParseError> {
        self.skip_whitespace();
        self.parse_identifier()?; // consume "type" keyword
        self.skip_whitespace();
        let name = self.parse_identifier()?;
        self.skip_whitespace();
        self.expect_char(b'=')?;
        self.skip_whitespace();
        let type_ = self.parse_type()?;
        Ok((name.value, type_))
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

    let mut root = if parser.peek() == Some(b'(') {
        parser.parse_struct()?
    } else {
        StructDef { fields: Vec::new() }
    };

    let mut enums: HashMap<String, EnumDef> = HashMap::new();
    let mut aliases: HashMap<String, Spanned<SchemaType>> = HashMap::new();

    loop {
        parser.skip_whitespace();
        if parser.peek().is_none() {
            break;
        }

        // Peek ahead to determine if this is "enum" or "type"
        let start = parser.position();
        let keyword = parser.parse_identifier()?;

        match keyword.value.as_str() {
            "enum" => {
                // Rewind — parse_enum_def expects to consume "enum" itself
                parser.offset = start.offset;
                parser.line = start.line;
                parser.column = start.column;

                let enum_def = parser.parse_enum_def()?;
                if let Some(old) = enums.insert(enum_def.name.clone(), enum_def) {
                    return Err(SchemaParseError {
                        span: Span { start: parser.position(), end: parser.position() },
                        kind: SchemaErrorKind::DuplicateEnum { name: old.name },
                    });
                }
            }
            "type" => {
                // Rewind — parse_alias_def expects to consume "type" itself
                parser.offset = start.offset;
                parser.line = start.line;
                parser.column = start.column;

                let (name, type_) = parser.parse_alias_def()?;
                if aliases.contains_key(&name) {
                    return Err(SchemaParseError {
                        span: type_.span,
                        kind: SchemaErrorKind::DuplicateAlias { name },
                    });
                }
                aliases.insert(name, type_);
            }
            other => {
                return Err(SchemaParseError {
                    span: keyword.span,
                    kind: SchemaErrorKind::UnexpectedToken {
                        expected: "\"enum\" or \"type\"".to_string(),
                        found: other.to_string(),
                    },
                });
            }
        }
    }

    // Reclassify EnumRefs that are actually aliases — in the root struct and in alias definitions.
    // Collect alias names into a set to avoid borrow conflicts when mutating alias values.
    let alias_names: HashSet<String> = aliases.keys().cloned().collect();
    reclassify_refs_in_struct_by_name(&mut root, &alias_names);
    for spanned_type in aliases.values_mut() {
        reclassify_refs_in_type_by_name(&mut spanned_type.value, &alias_names);
    }

    // Verify all refs resolve to a known enum or alias
    verify_refs(&root, &enums, &aliases)?;

    // Check for recursive aliases
    verify_no_recursive_aliases(&aliases)?;

    // Verify default values match their declared types
    verify_defaults(&root, &enums, &aliases)?;

    Ok(Schema { root, enums, aliases })
}

/// Reclassifies `EnumRef` names that are actually type aliases into `AliasRef`.
/// Mutates the struct in place.
fn reclassify_refs_in_struct_by_name(
    struct_def: &mut StructDef,
    alias_names: &HashSet<String>,
) {
    for field in &mut struct_def.fields {
        reclassify_refs_in_type_by_name(&mut field.type_.value, alias_names);
    }
}

fn reclassify_refs_in_type_by_name(
    schema_type: &mut SchemaType,
    alias_names: &HashSet<String>,
) {
    match schema_type {
        SchemaType::EnumRef(name) if alias_names.contains(name.as_str()) => {
            *schema_type = SchemaType::AliasRef(name.clone());
        }
        SchemaType::Option(inner) | SchemaType::List(inner) => {
            reclassify_refs_in_type_by_name(inner, alias_names);
        }
        SchemaType::Map(key, value) => {
            reclassify_refs_in_type_by_name(key, alias_names);
            reclassify_refs_in_type_by_name(value, alias_names);
        }
        SchemaType::Tuple(types) => {
            for t in types {
                reclassify_refs_in_type_by_name(t, alias_names);
            }
        }
        SchemaType::Struct(struct_def) => {
            reclassify_refs_in_struct_by_name(struct_def, alias_names);
        }
        _ => {}
    }
}

/// Verifies all `EnumRef` names resolve to a defined enum.
/// (`AliasRefs` have already been reclassified, so any remaining `EnumRef` must be an actual enum.)
fn verify_refs(
    struct_def: &StructDef,
    enums: &HashMap<String, EnumDef>,
    aliases: &HashMap<String, Spanned<SchemaType>>,
) -> Result<(), SchemaParseError> {
    for field in &struct_def.fields {
        check_type_refs(&field.type_.value, field.type_.span, enums, aliases)?;
    }
    Ok(())
}

fn check_type_refs(
    schema_type: &SchemaType,
    span: Span,
    enums: &HashMap<String, EnumDef>,
    aliases: &HashMap<String, Spanned<SchemaType>>,
) -> Result<(), SchemaParseError> {
    match schema_type {
        SchemaType::EnumRef(name) => {
            if !enums.contains_key(name) {
                return Err(SchemaParseError {
                    span,
                    kind: SchemaErrorKind::UnresolvedType { name: name.clone() },
                });
            }
        }
        SchemaType::AliasRef(name) => {
            if !aliases.contains_key(name) {
                return Err(SchemaParseError {
                    span,
                    kind: SchemaErrorKind::UnresolvedType { name: name.clone() },
                });
            }
        }
        SchemaType::Option(inner) | SchemaType::List(inner) => {
            check_type_refs(inner, span, enums, aliases)?;
        }
        SchemaType::Map(key, value) => {
            check_type_refs(key, span, enums, aliases)?;
            check_type_refs(value, span, enums, aliases)?;
        }
        SchemaType::Tuple(types) => {
            for t in types {
                check_type_refs(t, span, enums, aliases)?;
            }
        }
        SchemaType::Struct(struct_def) => {
            verify_refs(struct_def, enums, aliases)?;
        }
        _ => {}
    }
    Ok(())
}

/// Detects recursive type aliases — an alias that references itself directly or indirectly.
fn verify_no_recursive_aliases(
    aliases: &HashMap<String, Spanned<SchemaType>>,
) -> Result<(), SchemaParseError> {
    for (name, spanned_type) in aliases {
        let mut visited = HashSet::new();
        visited.insert(name.as_str());
        if let Some(cycle_name) = find_alias_cycle(&spanned_type.value, aliases, &mut visited) {
            return Err(SchemaParseError {
                span: spanned_type.span,
                kind: SchemaErrorKind::RecursiveAlias { name: cycle_name },
            });
        }
    }
    Ok(())
}

fn find_alias_cycle<'a>(
    schema_type: &'a SchemaType,
    aliases: &'a HashMap<String, Spanned<SchemaType>>,
    visited: &mut HashSet<&'a str>,
) -> Option<String> {
    match schema_type {
        SchemaType::AliasRef(name) => {
            if visited.contains(name.as_str()) {
                return Some(name.clone());
            }
            visited.insert(name.as_str());
            if let Some(target) = aliases.get(name) {
                return find_alias_cycle(&target.value, aliases, visited);
            }
            None
        }
        SchemaType::Option(inner) | SchemaType::List(inner) => {
            find_alias_cycle(inner, aliases, visited)
        }
        SchemaType::Map(key, value) => {
            if let Some(cycle) = find_alias_cycle(key, aliases, visited) {
                return Some(cycle);
            }
            find_alias_cycle(value, aliases, visited)
        }
        SchemaType::Tuple(types) => {
            for t in types {
                if let Some(cycle) = find_alias_cycle(t, aliases, visited) {
                    return Some(cycle);
                }
            }
            None
        }
        SchemaType::Struct(struct_def) => {
            for field in &struct_def.fields {
                if let Some(cycle) = find_alias_cycle(&field.type_.value, aliases, visited) {
                    return Some(cycle);
                }
            }
            None
        }
        _ => None,
    }
}

/// Resolves a `SchemaType` through alias indirection to its underlying type.
fn resolve_type<'a>(
    schema_type: &'a SchemaType,
    aliases: &'a HashMap<String, Spanned<SchemaType>>,
) -> &'a SchemaType {
    match schema_type {
        SchemaType::AliasRef(name) => {
            if let Some(target) = aliases.get(name) {
                resolve_type(&target.value, aliases)
            } else {
                schema_type
            }
        }
        _ => schema_type,
    }
}

/// Checks whether a default value is compatible with a schema type.
/// Returns `true` if the value matches the type.
#[allow(clippy::match_same_arms)]
fn default_matches_type(
    value: &RonValue,
    schema_type: &SchemaType,
    enums: &HashMap<String, EnumDef>,
    aliases: &HashMap<String, Spanned<SchemaType>>,
) -> bool {
    let resolved = resolve_type(schema_type, aliases);
    match (resolved, value) {
        (SchemaType::String, RonValue::String(_)) => true,
        (SchemaType::Integer, RonValue::Integer(_)) => true,
        (SchemaType::Float, RonValue::Float(_)) => true,
        (SchemaType::Bool, RonValue::Bool(_)) => true,
        (SchemaType::Option(_), RonValue::Option(None)) => true,
        (SchemaType::Option(inner), RonValue::Option(Some(inner_val))) => {
            default_matches_type(&inner_val.value, inner, enums, aliases)
        }
        (SchemaType::List(elem_type), RonValue::List(elements)) => {
            elements.iter().all(|e| default_matches_type(&e.value, elem_type, enums, aliases))
        }
        (SchemaType::EnumRef(enum_name), RonValue::Identifier(variant)) => {
            enums.get(enum_name).is_some_and(|e| e.variants.contains_key(variant))
        }
        (SchemaType::EnumRef(enum_name), RonValue::EnumVariant(variant, data)) => {
            enums.get(enum_name).is_some_and(|e| {
                matches!(e.variants.get(variant), Some(Some(data_type)) if default_matches_type(&data.value, data_type, enums, aliases))
            })
        }
        (SchemaType::Tuple(types), RonValue::Tuple(values)) => {
            types.len() == values.len()
                && types.iter().zip(values.iter()).all(|(t, v)| default_matches_type(&v.value, t, enums, aliases))
        }
        (SchemaType::Map(_, _), RonValue::Map(_)) => true, // map default type checking is impractical at parse time
        _ => false,
    }
}

/// Describes a schema type for error messages.
fn describe_type(schema_type: &SchemaType) -> String {
    match schema_type {
        SchemaType::String => "String".to_string(),
        SchemaType::Integer => "Integer".to_string(),
        SchemaType::Float => "Float".to_string(),
        SchemaType::Bool => "Bool".to_string(),
        SchemaType::Option(inner) => format!("Option({})", describe_type(inner)),
        SchemaType::List(inner) => format!("[{}]", describe_type(inner)),
        SchemaType::EnumRef(name) | SchemaType::AliasRef(name) => name.clone(),
        SchemaType::Map(k, v) => format!("{{{}: {}}}", describe_type(k), describe_type(v)),
        SchemaType::Tuple(types) => {
            let inner: Vec<String> = types.iter().map(describe_type).collect();
            format!("({})", inner.join(", "))
        }
        SchemaType::Struct(_) => "Struct".to_string(),
    }
}

/// Describes a RON value for error messages.
fn describe_value(value: &RonValue) -> String {
    match value {
        RonValue::String(s) => format!("String(\"{s}\")"),
        RonValue::Integer(n) => format!("Integer({n})"),
        RonValue::Float(f) => format!("Float({f})"),
        RonValue::Bool(b) => format!("Bool({b})"),
        RonValue::Option(None) => "None".to_string(),
        RonValue::Option(Some(_)) => "Some(...)".to_string(),
        RonValue::Identifier(s) => format!("Identifier({s})"),
        RonValue::EnumVariant(name, _) => format!("{name}(...)"),
        RonValue::List(_) => "List".to_string(),
        RonValue::Map(_) => "Map".to_string(),
        RonValue::Tuple(_) => "Tuple".to_string(),
        RonValue::Struct(_) => "Struct".to_string(),
    }
}

/// Verifies that all default values in a struct match their declared types.
fn verify_defaults(
    struct_def: &StructDef,
    enums: &HashMap<String, EnumDef>,
    aliases: &HashMap<String, Spanned<SchemaType>>,
) -> Result<(), SchemaParseError> {
    for field in &struct_def.fields {
        if let Some(default) = &field.default {
            if !default_matches_type(&default.value, &field.type_.value, enums, aliases) {
                return Err(SchemaParseError {
                    span: default.span,
                    kind: SchemaErrorKind::InvalidDefault {
                        field_name: field.name.value.clone(),
                        expected: describe_type(&field.type_.value),
                        found: describe_value(&default.value),
                    },
                });
            }
        }
    }
    // Check nested structs
    for field in &struct_def.fields {
        if let SchemaType::Struct(inner) = &field.type_.value {
            verify_defaults(inner, enums, aliases)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ron::RonValue;

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

    // Field without default has None.
    #[test]
    fn parse_field_no_default() {
        let mut p = parser("name: String,");
        let f = p.parse_field().unwrap();
        assert!(f.default.is_none());
    }

    // Field with string default.
    #[test]
    fn parse_field_default_string() {
        let mut p = parser("name: String = \"unnamed\",");
        let f = p.parse_field().unwrap();
        assert!(f.default.is_some());
        assert_eq!(f.default.unwrap().value, RonValue::String("unnamed".to_string()));
    }

    // Field with integer default.
    #[test]
    fn parse_field_default_integer() {
        let mut p = parser("count: Integer = 0,");
        let f = p.parse_field().unwrap();
        assert_eq!(f.default.unwrap().value, RonValue::Integer(0));
    }

    // Field with float default.
    #[test]
    fn parse_field_default_float() {
        let mut p = parser("weight: Float = 1.0,");
        let f = p.parse_field().unwrap();
        assert_eq!(f.default.unwrap().value, RonValue::Float(1.0));
    }

    // Field with bool default.
    #[test]
    fn parse_field_default_bool() {
        let mut p = parser("active: Bool = false,");
        let f = p.parse_field().unwrap();
        assert_eq!(f.default.unwrap().value, RonValue::Bool(false));
    }

    // Field with None default.
    #[test]
    fn parse_field_default_none() {
        let mut p = parser("label: Option(String) = None,");
        let f = p.parse_field().unwrap();
        assert_eq!(f.default.unwrap().value, RonValue::Option(None));
    }

    // Field with Some default.
    #[test]
    fn parse_field_default_some() {
        let mut p = parser("label: Option(String) = Some(\"default\"),");
        let f = p.parse_field().unwrap();
        if let RonValue::Option(Some(inner)) = &f.default.unwrap().value {
            assert_eq!(inner.value, RonValue::String("default".to_string()));
        } else {
            panic!("expected Option(Some(...))");
        }
    }

    // Field with empty list default.
    #[test]
    fn parse_field_default_empty_list() {
        let mut p = parser("tags: [String] = [],");
        let f = p.parse_field().unwrap();
        assert_eq!(f.default.unwrap().value, RonValue::List(vec![]));
    }

    // Field with identifier default.
    #[test]
    fn parse_field_default_identifier() {
        let mut p = parser("status: Status = Active,");
        let f = p.parse_field().unwrap();
        assert_eq!(f.default.unwrap().value, RonValue::Identifier("Active".to_string()));
    }

    // Default value has correct span.
    #[test]
    fn parse_field_default_has_span() {
        let mut p = parser("name: String = \"hi\",");
        let f = p.parse_field().unwrap();
        let default = f.default.unwrap();
        assert!(default.span.start.column > 1);
    }

    // ========================================================
    // Default value type checking (parse_schema level)
    // ========================================================

    // String default matches String type.
    #[test]
    fn default_type_check_string_accepts_string() {
        let result = parse_schema("(\n  name: String = \"hi\",\n)");
        assert!(result.is_ok());
    }

    // Integer default rejected for String type.
    #[test]
    fn default_type_check_string_rejects_integer() {
        let err = parse_schema("(\n  name: String = 42,\n)").unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidDefault { field_name, .. } if field_name == "name"));
    }

    // Integer default matches Integer type.
    #[test]
    fn default_type_check_integer_accepts_integer() {
        assert!(parse_schema("(\n  count: Integer = 0,\n)").is_ok());
    }

    // String default rejected for Integer type.
    #[test]
    fn default_type_check_integer_rejects_string() {
        let err = parse_schema("(\n  count: Integer = \"zero\",\n)").unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidDefault { .. }));
    }

    // Float default matches Float type.
    #[test]
    fn default_type_check_float_accepts_float() {
        assert!(parse_schema("(\n  weight: Float = 1.0,\n)").is_ok());
    }

    // Integer default rejected for Float type.
    #[test]
    fn default_type_check_float_rejects_integer() {
        let err = parse_schema("(\n  weight: Float = 1,\n)").unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidDefault { .. }));
    }

    // Bool default matches Bool type.
    #[test]
    fn default_type_check_bool_accepts_bool() {
        assert!(parse_schema("(\n  active: Bool = false,\n)").is_ok());
    }

    // String default rejected for Bool type.
    #[test]
    fn default_type_check_bool_rejects_string() {
        let err = parse_schema("(\n  active: Bool = \"false\",\n)").unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidDefault { .. }));
    }

    // None default matches Option type.
    #[test]
    fn default_type_check_option_accepts_none() {
        assert!(parse_schema("(\n  label: Option(String) = None,\n)").is_ok());
    }

    // Some with correct inner type matches Option type.
    #[test]
    fn default_type_check_option_accepts_some_correct() {
        assert!(parse_schema("(\n  label: Option(String) = Some(\"hi\"),\n)").is_ok());
    }

    // Some with wrong inner type rejected for Option type.
    #[test]
    fn default_type_check_option_rejects_some_wrong_type() {
        let err = parse_schema("(\n  label: Option(String) = Some(42),\n)").unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidDefault { .. }));
    }

    // Empty list matches list type.
    #[test]
    fn default_type_check_list_accepts_empty() {
        assert!(parse_schema("(\n  tags: [String] = [],\n)").is_ok());
    }

    // List with correct element type matches.
    #[test]
    fn default_type_check_list_accepts_correct_elements() {
        assert!(parse_schema("(\n  tags: [String] = [\"a\", \"b\"],\n)").is_ok());
    }

    // List with wrong element type rejected.
    #[test]
    fn default_type_check_list_rejects_wrong_elements() {
        let err = parse_schema("(\n  tags: [String] = [1, 2],\n)").unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidDefault { .. }));
    }

    // Valid enum variant accepted as default.
    #[test]
    fn default_type_check_enum_accepts_valid_variant() {
        assert!(parse_schema("(\n  status: Status = Active,\n)\nenum Status { Active, Inactive }").is_ok());
    }

    // Invalid enum variant rejected as default.
    #[test]
    fn default_type_check_enum_rejects_invalid_variant() {
        let err = parse_schema("(\n  status: Status = Unknown,\n)\nenum Status { Active, Inactive }").unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidDefault { .. }));
    }

    // InvalidDefault error includes expected type.
    #[test]
    fn default_type_check_error_includes_expected() {
        let err = parse_schema("(\n  name: String = 42,\n)").unwrap_err();
        if let SchemaErrorKind::InvalidDefault { expected, .. } = &err.kind {
            assert_eq!(expected, "String");
        } else {
            panic!("expected InvalidDefault");
        }
    }

    // InvalidDefault error includes found value description.
    #[test]
    fn default_type_check_error_includes_found() {
        let err = parse_schema("(\n  name: String = 42,\n)").unwrap_err();
        if let SchemaErrorKind::InvalidDefault { found, .. } = &err.kind {
            assert!(found.contains("Integer"));
        } else {
            panic!("expected InvalidDefault");
        }
    }

    // InvalidDefault error span points to the default value.
    #[test]
    fn default_type_check_error_has_span() {
        let err = parse_schema("(\n  name: String = 42,\n)").unwrap_err();
        assert!(err.span.start.line > 0);
    }

    // Type alias resolved for default type checking.
    #[test]
    fn default_type_check_alias_resolved() {
        assert!(parse_schema("(\n  name: Name = \"hi\",\n)\ntype Name = String").is_ok());
    }

    // Type alias with wrong default rejected.
    #[test]
    fn default_type_check_alias_rejects_wrong_type() {
        let err = parse_schema("(\n  name: Name = 42,\n)\ntype Name = String").unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidDefault { .. }));
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
        assert!(e.variants.contains_key("North"));
        assert!(e.variants.contains_key("South"));
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

    // Unresolved type ref is an error.
    #[test]
    fn schema_unresolved_type_ref() {
        let err = parse_schema("(\n  f: Faction,\n)").unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::UnresolvedType { name: "Faction".to_string() });
    }

    // Unresolved type ref inside Option is an error.
    #[test]
    fn schema_unresolved_type_ref_in_option() {
        let err = parse_schema("(\n  t: Option(Timing),\n)").unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::UnresolvedType { name: "Timing".to_string() });
    }

    // Unresolved type ref inside List is an error.
    #[test]
    fn schema_unresolved_type_ref_in_list() {
        let err = parse_schema("(\n  t: [CardType],\n)").unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::UnresolvedType { name: "CardType".to_string() });
    }

    // Duplicate enum name is an error.
    #[test]
    fn schema_duplicate_enum_name() {
        let err = parse_schema("enum A { X }\nenum A { Y }").unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::DuplicateEnum { name: "A".to_string() });
    }

    // ========================================================
    // Type alias tests — parsing
    // ========================================================

    // Basic type alias is stored in schema.aliases.
    #[test]
    fn alias_stored_in_schema() {
        let source = "(\n  cost: Cost,\n)\ntype Cost = (generic: Integer,)";
        let schema = parse_schema(source).unwrap();
        assert!(schema.aliases.contains_key("Cost"));
    }

    // Alias field is reclassified from EnumRef to AliasRef.
    #[test]
    fn alias_ref_reclassified() {
        let source = "(\n  cost: Cost,\n)\ntype Cost = (generic: Integer,)";
        let schema = parse_schema(source).unwrap();
        assert_eq!(schema.root.fields[0].type_.value, SchemaType::AliasRef("Cost".to_string()));
    }

    // Alias to a primitive type.
    #[test]
    fn alias_to_primitive() {
        let source = "(\n  name: Name,\n)\ntype Name = String";
        let schema = parse_schema(source).unwrap();
        assert_eq!(schema.aliases["Name"].value, SchemaType::String);
    }

    // Alias to a list type.
    #[test]
    fn alias_to_list() {
        let source = "(\n  tags: Tags,\n)\ntype Tags = [String]";
        let schema = parse_schema(source).unwrap();
        assert_eq!(schema.aliases["Tags"].value, SchemaType::List(Box::new(SchemaType::String)));
    }

    // Alias to an option type.
    #[test]
    fn alias_to_option() {
        let source = "(\n  power: Power,\n)\ntype Power = Option(Integer)";
        let schema = parse_schema(source).unwrap();
        assert_eq!(schema.aliases["Power"].value, SchemaType::Option(Box::new(SchemaType::Integer)));
    }

    // Alias inside a list field is reclassified.
    #[test]
    fn alias_ref_inside_list_reclassified() {
        let source = "(\n  costs: [Cost],\n)\ntype Cost = (generic: Integer,)";
        let schema = parse_schema(source).unwrap();
        assert_eq!(
            schema.root.fields[0].type_.value,
            SchemaType::List(Box::new(SchemaType::AliasRef("Cost".to_string())))
        );
    }

    // Alias inside an option field is reclassified.
    #[test]
    fn alias_ref_inside_option_reclassified() {
        let source = "(\n  cost: Option(Cost),\n)\ntype Cost = (generic: Integer,)";
        let schema = parse_schema(source).unwrap();
        assert_eq!(
            schema.root.fields[0].type_.value,
            SchemaType::Option(Box::new(SchemaType::AliasRef("Cost".to_string())))
        );
    }

    // Enums and aliases can coexist.
    #[test]
    fn alias_and_enum_coexist() {
        let source = "(\n  cost: Cost,\n  kind: Kind,\n)\ntype Cost = (generic: Integer,)\nenum Kind { A, B }";
        let schema = parse_schema(source).unwrap();
        assert!(schema.aliases.contains_key("Cost"));
        assert!(schema.enums.contains_key("Kind"));
    }

    // ========================================================
    // Type alias tests — error cases
    // ========================================================

    // Duplicate alias name is an error.
    #[test]
    fn alias_duplicate_name() {
        let source = "type A = String\ntype A = Integer";
        let err = parse_schema(source).unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::DuplicateAlias { name: "A".to_string() });
    }

    // Recursive alias is an error.
    #[test]
    fn alias_recursive_direct() {
        let source = "(\n  x: Foo,\n)\ntype Foo = Option(Foo)";
        let err = parse_schema(source).unwrap_err();
        assert_eq!(err.kind, SchemaErrorKind::RecursiveAlias { name: "Foo".to_string() });
    }

    // Indirect recursive alias is an error.
    #[test]
    fn alias_recursive_indirect() {
        let source = "(\n  x: Foo,\n)\ntype Foo = Option(Bar)\ntype Bar = [Foo]";
        let err = parse_schema(source).unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::RecursiveAlias { .. }));
    }

    // ========================================================
    // Map type tests — parsing
    // ========================================================

    // Parses a map type with String keys and Integer values.
    #[test]
    fn parse_type_map_string_to_integer() {
        let mut p = parser("{String: Integer}");
        let t = p.parse_type().unwrap();
        assert_eq!(
            t.value,
            SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::Integer))
        );
    }

    // Parses a map type with Integer keys.
    #[test]
    fn parse_type_map_integer_keys() {
        let mut p = parser("{Integer: String}");
        let t = p.parse_type().unwrap();
        assert_eq!(
            t.value,
            SchemaType::Map(Box::new(SchemaType::Integer), Box::new(SchemaType::String))
        );
    }

    // Map type field in a schema.
    #[test]
    fn schema_map_field() {
        let source = "(\n  attrs: {String: Integer},\n)";
        let schema = parse_schema(source).unwrap();
        assert_eq!(
            schema.root.fields[0].type_.value,
            SchemaType::Map(Box::new(SchemaType::String), Box::new(SchemaType::Integer))
        );
    }

    // Map with enum key type is allowed.
    #[test]
    fn schema_map_enum_key() {
        let source = "(\n  scores: {Stat: Integer},\n)\nenum Stat { Str, Dex, Con }";
        let schema = parse_schema(source).unwrap();
        assert_eq!(
            schema.root.fields[0].type_.value,
            SchemaType::Map(Box::new(SchemaType::EnumRef("Stat".to_string())), Box::new(SchemaType::Integer))
        );
    }

    // Map with Float key type is rejected.
    #[test]
    fn schema_map_float_key_rejected() {
        let source = "(\n  bad: {Float: String},\n)";
        let err = parse_schema(source).unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidMapKeyType { .. }));
    }

    // Map with Bool key type is rejected.
    #[test]
    fn schema_map_bool_key_rejected() {
        let source = "(\n  bad: {Bool: String},\n)";
        let err = parse_schema(source).unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::InvalidMapKeyType { .. }));
    }

    // ========================================================
    // Tuple type tests — parsing
    // ========================================================

    // Parses a tuple type with two elements.
    #[test]
    fn parse_type_tuple() {
        let mut p = parser("(Float, Float)");
        let t = p.parse_type().unwrap();
        assert_eq!(t.value, SchemaType::Tuple(vec![SchemaType::Float, SchemaType::Float]));
    }

    // Parses a tuple type with mixed types.
    #[test]
    fn parse_type_tuple_mixed() {
        let mut p = parser("(String, Integer, Bool)");
        let t = p.parse_type().unwrap();
        assert_eq!(
            t.value,
            SchemaType::Tuple(vec![SchemaType::String, SchemaType::Integer, SchemaType::Bool])
        );
    }

    // Tuple type in a schema field.
    #[test]
    fn schema_tuple_field() {
        let source = "(\n  pos: (Float, Float),\n)";
        let schema = parse_schema(source).unwrap();
        assert_eq!(
            schema.root.fields[0].type_.value,
            SchemaType::Tuple(vec![SchemaType::Float, SchemaType::Float])
        );
    }

    // Inline struct still works after tuple disambiguation.
    #[test]
    fn schema_struct_still_works() {
        let source = "(\n  cost: (generic: Integer,),\n)";
        let schema = parse_schema(source).unwrap();
        if let SchemaType::Struct(s) = &schema.root.fields[0].type_.value {
            assert_eq!(s.fields[0].name.value, "generic");
        } else {
            panic!("expected Struct");
        }
    }

    // Empty parens still parse as empty struct.
    #[test]
    fn schema_empty_parens_is_struct() {
        let source = "(\n  empty: (),\n)";
        let schema = parse_schema(source).unwrap();
        assert!(matches!(schema.root.fields[0].type_.value, SchemaType::Struct(_)));
    }

    // ========================================================
    // Enum variants with data — parsing
    // ========================================================

    // Parses enum with data variants.
    #[test]
    fn parse_enum_data_variant() {
        let source = "enum Effect { Damage(Integer), Heal(Integer), Draw }";
        let schema = parse_schema(source).unwrap();
        let effect = schema.enums.get("Effect").unwrap();
        assert_eq!(effect.variants.get("Damage"), Some(&Some(SchemaType::Integer)));
        assert_eq!(effect.variants.get("Heal"), Some(&Some(SchemaType::Integer)));
        assert_eq!(effect.variants.get("Draw"), Some(&None));
    }

    // Enum with struct data variant.
    #[test]
    fn parse_enum_struct_data_variant() {
        let source = "enum Action { Move((Integer, Integer)), Wait }";
        let schema = parse_schema(source).unwrap();
        let action = schema.enums.get("Action").unwrap();
        assert!(matches!(action.variants.get("Move"), Some(Some(SchemaType::Tuple(_)))));
        assert_eq!(action.variants.get("Wait"), Some(&None));
    }
}