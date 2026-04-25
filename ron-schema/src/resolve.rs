/*************************
 * Author: Bradley Hunter
 */

use std::collections::HashSet;

use crate::error::{SchemaParseError, SchemaErrorKind};
use crate::schema::Schema;
use crate::schema::parser::{reclassify_refs_in_struct_by_name, verify_refs, verify_defaults};
use crate::schema::parser::parse_schema;
use crate::span::{Span, Spanned};

/// Abstracts file resolution for schema imports.
///
/// The library crate cannot perform file I/O — this trait lets callers
/// (such as the CLI) provide their own loading strategy.
pub trait SchemaResolver {
    /// Loads the source text of a schema at the given import path.
    ///
    /// # Errors
    ///
    /// Returns a human-readable error message if the import path cannot be resolved.
    fn resolve(&self, import_path: &str) -> Result<String, String>;
}

/// Resolves all imports in a parsed schema, merging imported enums and type aliases
/// into the schema's namespace.
///
/// - Rejects name collisions between imported and locally defined types.
/// - Detects circular import chains up to 10 levels deep.
/// - Does not modify the root struct — only enums and aliases are imported.
///
/// # Errors
///
/// Returns a `SchemaParseError` if imports cannot be resolved, contain circular
/// dependencies, cause name collisions, or exceed the maximum nesting depth.
pub fn resolve_imports(
    schema: &mut Schema,
    resolver: &dyn SchemaResolver,
) -> Result<(), SchemaParseError> {
    let mut visited = HashSet::new();
    resolve_imports_recursive(schema, resolver, &mut visited, 0)?;

    // After merging, reclassify any EnumRefs that are now known aliases,
    // then verify all refs resolve and defaults type-check.
    let alias_names: HashSet<String> = schema.aliases.keys().cloned().collect();
    reclassify_refs_in_struct_by_name(&mut schema.root, &alias_names);
    verify_refs(&schema.root, &schema.enums, &schema.aliases)?;
    verify_defaults(&schema.root, &schema.enums, &schema.aliases)?;

    Ok(())
}

/// Maximum import nesting depth to prevent runaway resolution.
const MAX_IMPORT_DEPTH: usize = 10;

fn resolve_imports_recursive(
    schema: &mut Schema,
    resolver: &dyn SchemaResolver,
    visited: &mut HashSet<String>,
    depth: usize,
) -> Result<(), SchemaParseError> {
    if depth > MAX_IMPORT_DEPTH {
        // Use the first import's span for the error — we're too deep
        let span = schema.imports.first()
            .map_or(Span {
                start: crate::span::Position { offset: 0, line: 1, column: 1 },
                end: crate::span::Position { offset: 0, line: 1, column: 1 },
            }, |i| i.span);
        return Err(SchemaParseError {
            span,
            kind: SchemaErrorKind::UnexpectedToken {
                expected: "import depth within limit".to_string(),
                found: format!("import nesting exceeds {MAX_IMPORT_DEPTH} levels"),
            },
        });
    }

    let imports: Vec<Spanned<String>> = schema.imports.clone();

    for import in &imports {
        let path = &import.value;

        // Circular import detection
        if !visited.insert(path.clone()) {
            return Err(SchemaParseError {
                span: import.span,
                kind: SchemaErrorKind::CircularImport {
                    path: path.clone(),
                },
            });
        }

        // Resolve and parse the imported schema
        let source = resolver.resolve(path).map_err(|msg| {
            SchemaParseError {
                span: import.span,
                kind: SchemaErrorKind::UnresolvedImport {
                    path: path.clone(),
                    reason: msg,
                },
            }
        })?;

        let mut imported = parse_schema(&source).map_err(|mut e| {
            // Wrap the error to indicate which import caused it
            e.kind = SchemaErrorKind::ImportParseError {
                path: path.clone(),
                inner: Box::new(e.kind),
            };
            e
        })?;

        // Recursively resolve the imported schema's own imports
        resolve_imports_recursive(&mut imported, resolver, visited, depth + 1)?;

        // Merge imported enums — reject name collisions
        for (name, enum_def) in &imported.enums {
            if schema.enums.contains_key(name) || schema.aliases.contains_key(name) {
                return Err(SchemaParseError {
                    span: import.span,
                    kind: SchemaErrorKind::ImportNameCollision {
                        name: name.clone(),
                        import_path: path.clone(),
                    },
                });
            }
            schema.enums.insert(name.clone(), enum_def.clone());
        }

        // Merge imported aliases — reject name collisions
        for (name, alias) in &imported.aliases {
            if schema.aliases.contains_key(name) || schema.enums.contains_key(name) {
                return Err(SchemaParseError {
                    span: import.span,
                    kind: SchemaErrorKind::ImportNameCollision {
                        name: name.clone(),
                        import_path: path.clone(),
                    },
                });
            }
            schema.aliases.insert(name.clone(), alias.clone());
        }

        // Remove from visited so the same file can be imported from different branches
        visited.remove(path);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A mock resolver backed by an in-memory map of path -> source.
    struct MockResolver {
        files: HashMap<String, String>,
    }

    impl MockResolver {
        fn new() -> Self {
            Self { files: HashMap::new() }
        }

        fn add(mut self, path: &str, source: &str) -> Self {
            self.files.insert(path.to_string(), source.to_string());
            self
        }
    }

    impl SchemaResolver for MockResolver {
        fn resolve(&self, import_path: &str) -> Result<String, String> {
            self.files.get(import_path)
                .cloned()
                .ok_or_else(|| format!("file not found: {import_path}"))
        }
    }

    fn parse_and_resolve(source: &str, resolver: &dyn SchemaResolver) -> Result<Schema, SchemaParseError> {
        let mut schema = parse_schema(source)?;
        resolve_imports(&mut schema, resolver)?;
        Ok(schema)
    }

    // ========================================================
    // Import parsing
    // ========================================================

    // Schema with no imports has empty imports vec.
    #[test]
    fn no_imports() {
        let schema = parse_schema("(\n  name: String,\n)").unwrap();
        assert!(schema.imports.is_empty());
    }

    // Single import is parsed.
    #[test]
    fn single_import_parsed() {
        let schema = parse_schema("import \"types.ronschema\"\n(\n  name: String,\n)").unwrap();
        assert_eq!(schema.imports.len(), 1);
        assert_eq!(schema.imports[0].value, "types.ronschema");
    }

    // Multiple imports are parsed.
    #[test]
    fn multiple_imports_parsed() {
        let schema = parse_schema("import \"a.ronschema\"\nimport \"b.ronschema\"\n(\n  name: String,\n)").unwrap();
        assert_eq!(schema.imports.len(), 2);
        assert_eq!(schema.imports[0].value, "a.ronschema");
        assert_eq!(schema.imports[1].value, "b.ronschema");
    }

    // Import has correct span.
    #[test]
    fn import_has_span() {
        let schema = parse_schema("import \"types.ronschema\"\n(\n  name: String,\n)").unwrap();
        assert_eq!(schema.imports[0].span.start.line, 1);
    }

    // ========================================================
    // Import resolution — success
    // ========================================================

    // Imported enum is available for validation.
    #[test]
    fn imported_enum_available() {
        let resolver = MockResolver::new()
            .add("types.ronschema", "enum Status { Active, Inactive }");
        let schema = parse_and_resolve(
            "import \"types.ronschema\"\n(\n  status: Status,\n)",
            &resolver,
        ).unwrap();
        assert!(schema.enums.contains_key("Status"));
    }

    // Imported alias is available.
    #[test]
    fn imported_alias_available() {
        let resolver = MockResolver::new()
            .add("types.ronschema", "type Name = String");
        let schema = parse_and_resolve(
            "import \"types.ronschema\"\n(\n  name: Name,\n)",
            &resolver,
        ).unwrap();
        assert!(schema.aliases.contains_key("Name"));
    }

    // Local enums and imported enums coexist.
    #[test]
    fn local_and_imported_enums_coexist() {
        let resolver = MockResolver::new()
            .add("types.ronschema", "enum Color { Red, Blue }");
        let schema = parse_and_resolve(
            "import \"types.ronschema\"\n(\n  status: Status,\n  color: Color,\n)\nenum Status { Active, Inactive }",
            &resolver,
        ).unwrap();
        assert!(schema.enums.contains_key("Status"));
        assert!(schema.enums.contains_key("Color"));
    }

    // Imported enum has correct variants.
    #[test]
    fn imported_enum_has_variants() {
        let resolver = MockResolver::new()
            .add("types.ronschema", "enum Status { Active, Inactive }");
        let schema = parse_and_resolve(
            "import \"types.ronschema\"\n(\n  status: Status,\n)",
            &resolver,
        ).unwrap();
        let status = schema.enums.get("Status").unwrap();
        assert!(status.variants.contains_key("Active"));
        assert!(status.variants.contains_key("Inactive"));
    }

    // Transitive imports resolve.
    #[test]
    fn transitive_import_resolves() {
        let resolver = MockResolver::new()
            .add("a.ronschema", "import \"b.ronschema\"\nenum Color { Red }")
            .add("b.ronschema", "enum Size { Small, Large }");
        let schema = parse_and_resolve(
            "import \"a.ronschema\"\n(\n  color: Color,\n  size: Size,\n)",
            &resolver,
        ).unwrap();
        assert!(schema.enums.contains_key("Color"));
        assert!(schema.enums.contains_key("Size"));
    }

    // Schema with no root struct and only imports works.
    #[test]
    fn import_only_schema() {
        let resolver = MockResolver::new()
            .add("types.ronschema", "enum Status { Active }");
        let schema = parse_and_resolve(
            "import \"types.ronschema\"",
            &resolver,
        ).unwrap();
        assert!(schema.enums.contains_key("Status"));
    }

    // ========================================================
    // Import resolution — errors
    // ========================================================

    // Unresolved import produces error.
    #[test]
    fn unresolved_import_error() {
        let resolver = MockResolver::new();
        let err = parse_and_resolve(
            "import \"missing.ronschema\"\n(\n  name: String,\n)",
            &resolver,
        ).unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::UnresolvedImport { .. }));
    }

    // Unresolved import error contains the path.
    #[test]
    fn unresolved_import_error_has_path() {
        let resolver = MockResolver::new();
        let err = parse_and_resolve(
            "import \"missing.ronschema\"\n(\n  name: String,\n)",
            &resolver,
        ).unwrap_err();
        if let SchemaErrorKind::UnresolvedImport { path, .. } = &err.kind {
            assert_eq!(path, "missing.ronschema");
        } else {
            panic!("expected UnresolvedImport");
        }
    }

    // Circular import detected.
    #[test]
    fn circular_import_detected() {
        let resolver = MockResolver::new()
            .add("a.ronschema", "import \"b.ronschema\"\nenum A { X }")
            .add("b.ronschema", "import \"a.ronschema\"\nenum B { Y }");
        let err = parse_and_resolve(
            "import \"a.ronschema\"\n(\n  a: A,\n)",
            &resolver,
        ).unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::CircularImport { .. }));
    }

    // Name collision between import and local enum.
    #[test]
    fn name_collision_import_and_local_enum() {
        let resolver = MockResolver::new()
            .add("types.ronschema", "enum Status { Active }");
        let err = parse_and_resolve(
            "import \"types.ronschema\"\n(\n  status: Status,\n)\nenum Status { Inactive }",
            &resolver,
        ).unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::ImportNameCollision { .. }));
    }

    // Name collision error contains the colliding name.
    #[test]
    fn name_collision_error_has_name() {
        let resolver = MockResolver::new()
            .add("types.ronschema", "enum Status { Active }");
        let err = parse_and_resolve(
            "import \"types.ronschema\"\n(\n  status: Status,\n)\nenum Status { Inactive }",
            &resolver,
        ).unwrap_err();
        if let SchemaErrorKind::ImportNameCollision { name, .. } = &err.kind {
            assert_eq!(name, "Status");
        } else {
            panic!("expected ImportNameCollision");
        }
    }

    // Name collision between two imported schemas.
    #[test]
    fn name_collision_between_imports() {
        let resolver = MockResolver::new()
            .add("a.ronschema", "enum Status { Active }")
            .add("b.ronschema", "enum Status { Inactive }");
        let err = parse_and_resolve(
            "import \"a.ronschema\"\nimport \"b.ronschema\"\n(\n  status: Status,\n)",
            &resolver,
        ).unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::ImportNameCollision { .. }));
    }

    // Parse error in imported file is reported.
    #[test]
    fn import_parse_error_reported() {
        let resolver = MockResolver::new()
            .add("bad.ronschema", "(\n  name: Strang,\n)");
        let err = parse_and_resolve(
            "import \"bad.ronschema\"\n(\n  name: String,\n)",
            &resolver,
        ).unwrap_err();
        assert!(matches!(err.kind, SchemaErrorKind::ImportParseError { .. }));
    }

    // Import parse error contains the path.
    #[test]
    fn import_parse_error_has_path() {
        let resolver = MockResolver::new()
            .add("bad.ronschema", "(\n  name: Strang,\n)");
        let err = parse_and_resolve(
            "import \"bad.ronschema\"\n(\n  name: String,\n)",
            &resolver,
        ).unwrap_err();
        if let SchemaErrorKind::ImportParseError { path, .. } = &err.kind {
            assert_eq!(path, "bad.ronschema");
        } else {
            panic!("expected ImportParseError");
        }
    }
}
