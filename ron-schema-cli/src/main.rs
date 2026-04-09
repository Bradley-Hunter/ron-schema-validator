use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::fs;
use std::process;
use ron_schema::{
    parse_schema, parse_ron, validate, extract_source_line,
    ValidationError, ErrorKind,
};

/// Maps an ErrorKind to its short error code for display.
fn error_code(kind: &ErrorKind) -> &'static str {
    match kind {
        ErrorKind::MissingField { .. } => "missing-field",
        ErrorKind::UnknownField { .. } => "unknown-field",
        ErrorKind::TypeMismatch { .. } => "type-mismatch",
        ErrorKind::InvalidEnumVariant { .. } => "invalid-variant",
        ErrorKind::InvalidOptionValue { .. } => "invalid-option",
        ErrorKind::InvalidListElement { .. } => "invalid-element",
        ErrorKind::ExpectedStruct { .. } => "expected-struct",
        ErrorKind::ExpectedList { .. } => "expected-list",
        ErrorKind::ExpectedOption { .. } => "expected-option",
        ErrorKind::InvalidVariantData { .. } => "invalid-variant-data",
        ErrorKind::ExpectedMap { .. } => "expected-map",
        ErrorKind::InvalidMapKey { .. } => "invalid-map-key",
        ErrorKind::InvalidMapValue { .. } => "invalid-map-value",
        ErrorKind::ExpectedTuple { .. } => "expected-tuple",
        ErrorKind::TupleLengthMismatch { .. } => "tuple-length",
        ErrorKind::InvalidTupleElement { .. } => "invalid-tuple-element",
    }
}

/// Produces the human-readable message line for an error.
fn error_message(error: &ValidationError) -> String {
    match &error.kind {
        ErrorKind::MissingField { field_name } => {
            format!("missing required field `{}`", field_name)
        }
        ErrorKind::UnknownField { field_name } => {
            format!("field `{}` is not defined in the schema", field_name)
        }
        ErrorKind::TypeMismatch { expected, found } => {
            format!("field `{}`: expected {}, found {}", error.path, expected, found)
        }
        ErrorKind::InvalidEnumVariant { enum_name, variant, valid } => {
            format!(
                "field `{}`: `{}` is not a valid {} variant, expected one of: {}",
                error.path, variant, enum_name, valid.join(", ")
            )
        }
        ErrorKind::InvalidOptionValue { expected, found } => {
            format!("field `{}`: expected {}, found {}", error.path, expected, found)
        }
        ErrorKind::InvalidListElement { index, expected, found } => {
            format!("field `{}`: element {} expected {}, found {}", error.path, index, expected, found)
        }
        ErrorKind::ExpectedStruct { found } => {
            format!("field `{}`: expected struct, found {}", error.path, found)
        }
        ErrorKind::ExpectedList { found } => {
            format!("field `{}`: expected list, found {}", error.path, found)
        }
        ErrorKind::ExpectedOption { found } => {
            format!("field `{}`: expected Some(...) or None, found {}", error.path, found)
        }
        ErrorKind::InvalidVariantData { enum_name, variant, expected, found } => {
            format!("field `{}`: variant `{}::{}` expected {}, found {}", error.path, enum_name, variant, expected, found)
        }
        ErrorKind::ExpectedMap { found } => {
            format!("field `{}`: expected map, found {}", error.path, found)
        }
        ErrorKind::InvalidMapKey { key, expected, found } => {
            format!("field `{}`: map key {} expected {}, found {}", error.path, key, expected, found)
        }
        ErrorKind::InvalidMapValue { key, expected, found } => {
            format!("field `{}`[{}]: expected {}, found {}", error.path, key, expected, found)
        }
        ErrorKind::ExpectedTuple { found } => {
            format!("field `{}`: expected tuple, found {}", error.path, found)
        }
        ErrorKind::TupleLengthMismatch { expected, found } => {
            format!("field `{}`: expected {} elements, found {}", error.path, expected, found)
        }
        ErrorKind::InvalidTupleElement { index, expected, found } => {
            format!("field `{}`: element {} expected {}, found {}", error.path, index, expected, found)
        }
    }
}

/// Short label for the underline beneath the source line.
fn underline_label(kind: &ErrorKind) -> String {
    match kind {
        ErrorKind::MissingField { field_name } => {
            format!("struct ends here without field `{}`", field_name)
        }
        ErrorKind::UnknownField { .. } => "unknown field".to_string(),
        ErrorKind::TypeMismatch { expected, .. } => format!("expected {}", expected),
        ErrorKind::InvalidEnumVariant { valid, .. } => {
            format!("expected one of: {}", valid.join(", "))
        }
        ErrorKind::InvalidOptionValue { expected, .. } => format!("expected {}", expected),
        ErrorKind::InvalidListElement { expected, .. } => format!("expected {}", expected),
        ErrorKind::ExpectedStruct { .. } => "expected struct".to_string(),
        ErrorKind::ExpectedList { .. } => "expected list".to_string(),
        ErrorKind::ExpectedOption { .. } => "expected Some(...) or None".to_string(),
        ErrorKind::InvalidVariantData { expected, .. } => format!("expected {expected}"),
        ErrorKind::ExpectedMap { .. } => "expected map".to_string(),
        ErrorKind::InvalidMapKey { expected, .. } => format!("expected {expected}"),
        ErrorKind::InvalidMapValue { expected, .. } => format!("expected {expected}"),
        ErrorKind::ExpectedTuple { .. } => "expected tuple".to_string(),
        ErrorKind::TupleLengthMismatch { expected, .. } => format!("expected {expected} elements"),
        ErrorKind::InvalidTupleElement { expected, .. } => format!("expected {expected}"),
    }
}

/// Formats a single validation error in rustc-style output.
///
/// ```text
/// error[type-mismatch] at path/to/file.ron:6:16
///     field `cost.generic`: expected Integer, found String
///    6 │     generic: "two",
///      │              ^^^^^ expected Integer
/// ```
fn format_error(error: &ValidationError, source: &str, file_path: &str) -> String {
    let line = error.span.start.line;
    let col = error.span.start.column;
    let source_line = extract_source_line(source, error.span);

    let line_num_width = source_line.line_number.to_string().len();
    let gutter_pad = " ".repeat(line_num_width);

    let underline_start = source_line.highlight_start;
    let underline_len = if source_line.highlight_end > source_line.highlight_start {
        source_line.highlight_end - source_line.highlight_start
    } else {
        1
    };
    let underline_pad = " ".repeat(underline_start);
    let underline = "^".repeat(underline_len);
    let label = underline_label(&error.kind);

    format!(
        "error[{}] at {}:{}:{}\n    {}\n  {} │ {}\n  {} │ {}{} {}",
        error_code(&error.kind),
        file_path,
        line,
        col,
        error_message(error),
        source_line.line_number,
        source_line.line_text,
        gutter_pad,
        underline_pad,
        underline,
        label,
    )
}

#[derive(Parser)]
#[command(name = "ron-schema", version, about = "Validate RON files against schemas")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate RON files against a schema
    Validate {
        /// Path to the .ronschema file
        #[arg(long)]
        schema: PathBuf,

        /// Path to a .ron file or directory of .ron files
        target: PathBuf,
    },
}

/// Validates a single .ron file against a parsed schema.
/// Returns the number of errors found.
fn validate_file(
    schema: &ron_schema::Schema,
    file_path: &PathBuf,
    display_path: &str,
) -> usize {
    let source = match fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read {}: {}", display_path, e);
            return 1;
        }
    };

    let ron_value = match parse_ron(&source) {
        Ok(v) => v,
        Err(e) => {
            let source_line = extract_source_line(&source, e.span);
            eprintln!(
                "error[parse] at {}:{}:{}\n    {:?}\n  {} │ {}",
                display_path,
                e.span.start.line,
                e.span.start.column,
                e.kind,
                source_line.line_number,
                source_line.line_text,
            );
            return 1;
        }
    };

    let errors = validate(schema, &ron_value);
    if errors.is_empty() {
        return 0;
    }

    for error in &errors {
        println!("{}", format_error(error, &source, display_path));
        println!();
    }

    println!("Found {} error{} in {}", errors.len(), if errors.len() == 1 { "" } else { "s" }, display_path);
    errors.len()
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { schema, target } => {
            // Read and parse the schema
            let schema_source = match fs::read_to_string(&schema) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: could not read schema {}: {}", schema.display(), e);
                    process::exit(2);
                }
            };
            let parsed_schema = match parse_schema(&schema_source) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "error[schema] at {}:{}:{}\n    {:?}",
                        schema.display(),
                        e.span.start.line,
                        e.span.start.column,
                        e.kind,
                    );
                    process::exit(2);
                }
            };

            // Single file or directory?
            if target.is_file() {
                let display_path = target.display().to_string();
                let error_count = validate_file(&parsed_schema, &target, &display_path);
                if error_count > 0 {
                    process::exit(1);
                }
            } else if target.is_dir() {
                let mut total_files = 0;
                let mut files_with_errors = 0;
                let mut total_errors = 0;

                // Walk the directory for .ron files
                let entries = collect_ron_files(&target);
                for file_path in &entries {
                    let display_path = file_path.display().to_string();
                    total_files += 1;
                    let error_count = validate_file(&parsed_schema, file_path, &display_path);
                    if error_count > 0 {
                        files_with_errors += 1;
                        total_errors += error_count;
                    }
                }

                println!(
                    "Validated {} file{}: {} valid, {} with errors ({} error{} total)",
                    total_files,
                    if total_files == 1 { "" } else { "s" },
                    total_files - files_with_errors,
                    files_with_errors,
                    total_errors,
                    if total_errors == 1 { "" } else { "s" },
                );

                if total_errors > 0 {
                    process::exit(1);
                }
            } else {
                eprintln!("error: {} is not a file or directory", target.display());
                process::exit(2);
            }
        }
    }
}

/// Recursively collects all .ron files in a directory.
fn collect_ron_files(dir: &PathBuf) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_ron_files(&path));
            } else if path.extension().is_some_and(|ext| ext == "ron") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}