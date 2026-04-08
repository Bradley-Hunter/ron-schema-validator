# ron-schema-validator

Schema validation for [RON (Rusty Object Notation)](https://github.com/ron-rs/ron) files. Define the expected structure of your `.ron` data in a `.ronschema` file, then validate against it — catching type mismatches, missing fields, invalid enum variants, and more.

RON has no equivalent of JSON Schema. This project fills that gap.

> **Status:** v0.1 MVP complete. Schema parser, RON parser, validator, and CLI are all functional with test coverage.

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

## Library Usage

The library crate (`ron-schema`) operates on `&str` — no file I/O, no formatting opinions.

```rust
use ron_schema::{parse_schema, parse_ron, validate, extract_source_line};

let schema = parse_schema(schema_source)?;
let value = parse_ron(ron_source)?;
let errors = validate(&schema, &value);

for error in &errors {
    let source_line = extract_source_line(ron_source, error.span);
    // Render however you like
}
```

Validation collects all errors rather than failing on the first — useful when batch-validating many files.

## Project Structure

```
ron-schema-validator/
├── ron-schema/          ← library crate (zero external dependencies)
│   └── src/
│       ├── lib.rs       ← public API re-exports
│       ├── span.rs      ← Position, Span, Spanned<T>
│       ├── error.rs     ← error types (schema, RON, validation)
│       ├── diagnostic.rs
│       ├── schema/      ← schema AST + parser
│       └── ron/         ← RON value types + parser
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

### Future (v1.0+)
- [ ] Schema composition / imports
- [ ] Custom validation rules (value ranges, string patterns)
- [ ] Optional field presence (`default` values)
- [ ] `--format json` output
- [ ] `init` subcommand (schema inference)
- [ ] Warnings and `--deny-warnings`

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| Custom `.ronschema` format | Mirrors data shape, reads like Rust type definitions. More readable than embedding schema rules in RON. |
| Custom RON parser | The `ron` crate's `Value` type discards bare identifier names, making enum validation impossible. |
| All fields required by default | Mirrors Rust semantics. `Option(T)` controls the value, not whether the field can be absent. |
| Collect all errors | Primary use case is batch-validating many files. Users need all problems at once. |
| Zero library dependencies | The library crate has no external dependencies. |

## License

MIT
