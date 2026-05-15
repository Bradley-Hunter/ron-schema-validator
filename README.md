# ron-schema-validator

Schema validation for [RON (Rusty Object Notation)](https://github.com/ron-rs/ron) files. Define the expected structure of your `.ron` data in a `.ronschema` file, then validate against it — catching type mismatches, missing fields, invalid enum variants, and more.

RON has no equivalent of JSON Schema. This project fills that gap.

> **Status:** v1.0 — Feature-complete. Schema parser, RON parser, validator, and CLI with schema inference, JSON output, default field values, warnings, schema imports, and custom validation annotations.

## Schema Format

Schemas use a custom `.ronschema` format that mirrors the shape of the data it validates. Types replace values:

```
// config.ronschema
(
  name: String,
  version: Integer,
  debug: Bool,
  description: Option(String),
  tags: [String],
  window: (
    width: Integer,
    height: Integer,
  ),
)
```

A matching `.ron` data file:

```ron
// config.ron
(
  name: "my-app",
  version: 3,
  debug: true,
  description: Some("A sample application"),
  tags: ["cli", "tools"],
  window: (
    width: 1920,
    height: 1080,
  ),
)
```

### Supported Types

| Type | Matches | Example |
|------|---------|---------|
| `String` | Quoted strings | `"hello"` |
| `Integer` | Signed integers (i64) | `42`, `-1` |
| `Float` | Floating-point values (f64) | `3.14` |
| `Bool` | Boolean literals | `true`, `false` |
| `Option(T)` | `Some(value)` or `None` | `Some(5)`, `None` |
| `[T]` | Homogeneous list | `[1, 2, 3]` |
| `{K: V}` | Map with typed keys and values | `{"str": 5, "dex": 3}` |
| `(T1, T2, ...)` | Positional tuple | `(1.0, 2.5)` |
| Inline struct | Nested `(...)` with named fields | See example above |
| Enum reference | Bare identifier from a defined enum | `Creature`, `Damage(5)` |
| Type alias | Named type via `type Name = T` | See below |

### Enums

Define enums after the root struct. Variants can be unit (bare identifiers) or carry associated data:

```
(
  status: Status,
  effect: Effect,
)

enum Status { Active, Inactive, Pending }
enum Effect { Damage(Integer), Heal(Integer), Draw }
```

### Type Aliases

Define reusable types with `type Name = T`:

```
(
  cost: Cost,
  backup_cost: Cost,
)

type Cost = (generic: Integer, sigil: Integer,)
```

### Default Values

Fields with defaults are optional — they don't produce errors when absent from data:

```
(
  name: String,
  label: String = "unnamed",
  count: Integer = 0,
  tags: [String] = [],
  status: Status = Active,
)

enum Status { Active, Inactive }
```

Default values are type-checked against their field's declared type at schema parse time.

### Imports

Share enums and type aliases across schemas using `import` statements at the top of a file:

```
// shared-types.ronschema
enum Rarity { Common, Uncommon, Rare }
type Label = String
```

```
// item.ronschema
import "shared-types.ronschema"

(
  name: String,
  rarity: Rarity,
  label: Label,
)
```

Import paths are resolved relative to the importing schema's directory. Circular imports and name collisions between imported and local types are reported as parse errors. Import nesting is limited to 10 levels.

### Annotations

Add value-level constraints with annotations placed before the field:

```
(
  @range(0, 100)
  health: Integer,

  @min_length(1)
  @max_length(50)
  name: String,

  @pattern("^[a-z_]+$")
  tag: String,
)
```

| Annotation | Applies to | Checks |
|------------|-----------|--------|
| `@range(min, max)` | Integer, Float | Value is within bounds (inclusive) |
| `@min_length(n)` | String, List | Length is at least `n` |
| `@max_length(n)` | String, List | Length is at most `n` |
| `@pattern("regex")` | String | Value matches the regex pattern |

`@pattern` requires the `regex` cargo feature:
```toml
ron-schema = { version = "0.10", features = ["regex"] }
```

#### Cross-field constraints

Use `@require` inside a struct to enforce relationships between fields:

```
(
  @require(min <= max)
  min: Integer,
  max: Integer,
)
```

Supported operators: `<`, `<=`, `>`, `>=`, `==`, `!=`. Comparisons work on Integer and Float fields. Fields with defaults use their default value when absent.

### Maps

Map types use `{KeyType: ValueType}`. Keys must be `String`, `Integer`, or an enum type:

```
(
  attributes: {String: Integer},
)
```

## CLI Usage

```
ron-schema validate --schema config.ronschema target.ron
```

Pass a directory as the target to validate all `.ron` files within it:

```
ron-schema validate --schema card.ronschema cards/
```

Use `--format json` for machine-readable output:

```
ron-schema validate --schema config.ronschema data/ --format json
```

```json
{
  "success": true,
  "results": [
    {
      "file": "data/config.ron",
      "errors": [],
      "warnings": []
    }
  ]
}
```

Use `--deny-warnings` to treat warnings as errors (exit code 1):

```
ron-schema validate --schema config.ronschema data/ --deny-warnings
```

### Schema Inference

Generate a schema from an example `.ron` file:

```
ron-schema init example.ron
```

Write the inferred schema to a file:

```
ron-schema init example.ron --output example.ronschema
```

The inferred schema is a starting point — review and refine it to match your requirements.

## Library Usage

The library crate (`ron-schema`) operates on `&str` — no file I/O, no formatting opinions.

```rust
use ron_schema::{parse_schema, parse_ron, validate, extract_source_line};

let schema = parse_schema(schema_source)?;
let value = parse_ron(ron_source)?;
let result = validate(&schema, &value);

for error in &result.errors {
    let source_line = extract_source_line(ron_source, error.span);
    // Render however you like
}
```

Validation collects all errors rather than failing on the first — useful when batch-validating many files.

### Schema Inference

```rust
use ron_schema::{parse_ron, infer_schema, format_schema};

let value = parse_ron(ron_source)?;
let schema = infer_schema(&value);
let schema_text = format_schema(&schema);
```

## Project Structure

```
ron-schema-validator/
├── ron-schema/          ← library crate (zero default dependencies, optional `regex` feature)
│   └── src/
│       ├── lib.rs       ← public API re-exports
│       ├── span.rs      ← Position, Span, Spanned<T>
│       ├── error.rs     ← error types (schema, RON, validation)
│       ├── diagnostic.rs
│       ├── schema/      ← schema AST + parser
│       ├── ron/         ← RON value types + parser
│       ├── format.rs    ← schema-to-text formatter
│       ├── infer.rs     ← schema inference from RON data
│       └── resolve.rs   ← import resolution
└── ron-schema-cli/      ← binary crate (clap)
    └── src/
        └── main.rs
```

## Building

```
cargo build
cargo test
```

Requires Rust 2021 edition.

## Roadmap

### MVP (v0.1)

- [x] Workspace and crate scaffolding
- [x] Source location types (`Span`, `Position`, `Spanned<T>`)
- [x] Error types (schema parsing, RON parsing, validation)
- [x] Schema AST types
- [x] RON value types
- [x] Source line extraction for diagnostics
- [X] Schema parser (`.ronschema` → AST)
- [X] RON data parser (`.ron` → `Spanned<RonValue>`)
- [X] Validation engine
- [X] CLI wiring (file I/O, error rendering, batch mode)
- [X] Test coverage for all error kinds

### v0.2 — Type Aliases

- [x] `type Name = T` definitions
- [x] Recursive alias detection
- [x] Alias resolution during validation

### v0.3 — Map Types

- [x] `{K: V}` schema syntax
- [x] `{ key: value, ... }` RON parsing
- [x] Map key/value validation
- [x] Key type restriction (String, Integer, enum)

### v0.4 — Tuple Types

- [x] `(T1, T2, ...)` schema syntax
- [x] Tuple parsing in RON data with struct/tuple disambiguation
- [x] Tuple length and element type validation

### v0.5 — Enum Variants with Data

- [x] `Variant(Type)` schema syntax
- [x] `Variant(value)` RON parsing
- [x] Unit vs data variant validation

### v0.6 — JSON Output

- [x] `--format json` for machine-readable output
- [x] Structured error objects with code, severity, path, message, span
- [x] Schema parse errors surfaced in JSON with `success: false`

### v0.7 — Default Values

- [x] `field: Type = <value>` syntax for optional fields
- [x] Fields with defaults not required in data
- [x] Default values type-checked at schema parse time

### v0.8 — Warnings

- [x] Warning infrastructure parallel to the error system
- [x] `FieldOrderMismatch` warning when data field order differs from schema
- [x] `--deny-warnings` flag causes exit code 1 on warnings
- [x] Warnings rendered in both human and JSON output formats

### v0.9 — Schema Composition / Imports

- [x] `import "path"` syntax at the top of schema files
- [x] Imported enums and type aliases merged into importing schema
- [x] `SchemaResolver` trait keeps the library filesystem-free
- [x] Circular import detection with 10-level nesting cap
- [x] Name collision detection between imports and local types

### v0.10 — Custom Validation Rules (Annotations)

- [x] `@range(min, max)` for numeric bounds
- [x] `@min_length(n)` and `@max_length(n)` for string/list length
- [x] `@pattern("regex")` for string matching (feature-gated behind `regex`)
- [x] `@require(field op field)` for cross-field constraints
- [x] Parse-time validation of annotation arguments

### v1.0 — Schema Inference & Documentation

- [x] `ron-schema init` subcommand to infer schemas from example data
- [x] `format_schema()` library function for schema-to-text conversion
- [x] `infer_schema()` library function for RON-to-schema inference
- [x] Documentation polish

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| Custom `.ronschema` format | Mirrors data shape, reads like Rust type definitions. More readable than embedding schema rules in RON. |
| Custom RON parser | The `ron` crate's `Value` type discards bare identifier names, making enum validation impossible. |
| All fields required by default | Mirrors Rust semantics. `Option(T)` controls the value, not whether the field can be absent. |
| Collect all errors | Primary use case is batch-validating many files. Users need all problems at once. |
| Zero default dependencies | The library crate has no required dependencies. `regex` is opt-in via a feature flag. |

## License

MIT
