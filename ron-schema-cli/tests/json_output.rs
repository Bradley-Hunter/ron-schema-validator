use assert_cmd::Command;
use serde_json::Value;

fn cmd() -> Command {
    Command::cargo_bin("ron-schema").unwrap()
}

fn schema() -> &'static str {
    "tests/fixtures/item.ronschema"
}

fn parse_json(output: &[u8]) -> Value {
    serde_json::from_slice(output).expect("output should be valid JSON")
}

// ─── Valid file ───

#[test]
fn json_valid_file_exits_with_zero() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "json"])
        .assert()
        .success();
}

#[test]
fn json_valid_file_has_success_true() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["success"], true);
}

#[test]
fn json_valid_file_has_one_result() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"].as_array().unwrap().len(), 1);
}

#[test]
fn json_valid_file_has_empty_errors() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["results"][0]["errors"].as_array().unwrap().is_empty());
}

#[test]
fn json_valid_file_has_empty_warnings() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["results"][0]["warnings"].as_array().unwrap().is_empty());
}

#[test]
fn json_valid_file_has_correct_file_path() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["file"], "tests/fixtures/valid.ron");
}

#[test]
fn json_valid_file_has_no_error_field() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json.get("error").is_none() || json["error"].is_null());
}

// ─── Validation error ───

#[test]
fn json_validation_error_exits_with_one() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .assert()
        .code(1);
}

#[test]
fn json_validation_error_has_success_true() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["success"], true);
}

#[test]
fn json_validation_error_has_one_error() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["errors"].as_array().unwrap().len(), 1);
}

#[test]
fn json_validation_error_has_correct_code() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["errors"][0]["code"], "type-mismatch");
}

#[test]
fn json_validation_error_has_severity_error() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["errors"][0]["severity"], "error");
}

#[test]
fn json_validation_error_has_correct_path() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["errors"][0]["path"], "name");
}

#[test]
fn json_validation_error_has_message() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    let message = json["results"][0]["errors"][0]["message"].as_str().unwrap();
    assert!(message.contains("expected String"));
}

#[test]
fn json_validation_error_has_line() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["errors"][0]["line"], 2);
}

#[test]
fn json_validation_error_has_column() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["results"][0]["errors"][0]["column"].as_u64().unwrap() > 0);
}

#[test]
fn json_validation_error_has_span_start() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    let span = &json["results"][0]["errors"][0]["span"];
    assert!(span["start"]["line"].as_u64().unwrap() > 0);
    assert!(span["start"]["column"].as_u64().unwrap() > 0);
}

#[test]
fn json_validation_error_has_span_end() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    let span = &json["results"][0]["errors"][0]["span"];
    assert!(span["end"]["line"].as_u64().unwrap() > 0);
    assert!(span["end"]["column"].as_u64().unwrap() > 0);
}

// ─── Missing field error ───

#[test]
fn json_missing_field_has_correct_code() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/missing-field.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["errors"][0]["code"], "missing-field");
}

#[test]
fn json_missing_field_message_contains_field_name() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/missing-field.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    let message = json["results"][0]["errors"][0]["message"].as_str().unwrap();
    assert!(message.contains("category"));
}

// ─── RON parse error ───

#[test]
fn json_parse_error_exits_with_one() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/parse-error.ron", "--format", "json"])
        .assert()
        .code(1);
}

#[test]
fn json_parse_error_has_success_true() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/parse-error.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["success"], true);
}

#[test]
fn json_parse_error_has_code_parse() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/parse-error.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["errors"][0]["code"], "parse");
}

#[test]
fn json_parse_error_has_severity_error() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/parse-error.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["errors"][0]["severity"], "error");
}

#[test]
fn json_parse_error_has_line_info() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/parse-error.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["results"][0]["errors"][0]["line"].as_u64().unwrap() > 0);
}

// ─── Schema parse error ───

#[test]
fn json_schema_error_exits_with_two() {
    cmd()
        .args(["validate", "--schema", "tests/fixtures/bad.ronschema", "tests/fixtures/valid.ron", "--format", "json"])
        .assert()
        .code(2);
}

#[test]
fn json_schema_error_has_success_false() {
    let output = cmd()
        .args(["validate", "--schema", "tests/fixtures/bad.ronschema", "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["success"], false);
}

#[test]
fn json_schema_error_has_error_message() {
    let output = cmd()
        .args(["validate", "--schema", "tests/fixtures/bad.ronschema", "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["error"].as_str().is_some());
    assert!(!json["error"].as_str().unwrap().is_empty());
}

#[test]
fn json_schema_error_has_empty_results() {
    let output = cmd()
        .args(["validate", "--schema", "tests/fixtures/bad.ronschema", "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["results"].as_array().unwrap().is_empty());
}

// ─── Missing schema file ───

#[test]
fn json_missing_schema_exits_with_two() {
    cmd()
        .args(["validate", "--schema", "tests/fixtures/nonexistent.ronschema", "tests/fixtures/valid.ron", "--format", "json"])
        .assert()
        .code(2);
}

#[test]
fn json_missing_schema_has_success_false() {
    let output = cmd()
        .args(["validate", "--schema", "tests/fixtures/nonexistent.ronschema", "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["success"], false);
}

// ─── Invalid target path ───

#[test]
fn json_invalid_target_exits_with_two() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/nonexistent.ron", "--format", "json"])
        .assert()
        .code(2);
}

#[test]
fn json_invalid_target_has_success_false() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/nonexistent.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["success"], false);
}

// ─── Output is valid JSON ───

#[test]
fn json_output_is_valid_json_on_success() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    parse_json(&output.stdout); // panics if not valid JSON
}

#[test]
fn json_output_is_valid_json_on_error() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "json"])
        .output()
        .unwrap();
    parse_json(&output.stdout);
}

#[test]
fn json_output_is_valid_json_on_schema_error() {
    let output = cmd()
        .args(["validate", "--schema", "tests/fixtures/bad.ronschema", "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    parse_json(&output.stdout);
}

// ─── Human format still works ───

#[test]
fn human_format_valid_file_exits_with_zero() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "human"])
        .assert()
        .success();
}

#[test]
fn human_format_error_exits_with_one() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "human"])
        .assert()
        .code(1);
}

#[test]
fn human_format_does_not_output_json() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron", "--format", "human"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("error["));
    assert!(!stdout.starts_with('{'));
}

// ─── Default format is human ───

#[test]
fn default_format_is_human() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/type-mismatch.ron"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("error["));
    assert!(!stdout.starts_with('{'));
}

// ─── Directory batch mode ───

#[test]
fn json_directory_validates_all_ron_files() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    let results = json["results"].as_array().unwrap();
    // fixtures dir has valid.ron, type-mismatch.ron, missing-field.ron, parse-error.ron,
    // out-of-order.ron, import-valid.ron, import-invalid-variant.ron
    assert_eq!(results.len(), 7);
}

#[test]
fn json_directory_has_success_true() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["success"], true);
}

#[test]
fn json_directory_exits_with_one_when_errors_present() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/", "--format", "json"])
        .assert()
        .code(1);
}

// ─── Warnings (JSON) ───

#[test]
fn json_warning_exits_with_zero() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .assert()
        .success();
}

#[test]
fn json_warning_has_success_true() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["success"], true);
}

#[test]
fn json_warning_has_empty_errors() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["results"][0]["errors"].as_array().unwrap().is_empty());
}

#[test]
fn json_warning_has_one_warning() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["warnings"].as_array().unwrap().len(), 1);
}

#[test]
fn json_warning_has_correct_code() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["warnings"][0]["code"], "field-order");
}

#[test]
fn json_warning_has_severity_warning() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["warnings"][0]["severity"], "warning");
}

#[test]
fn json_warning_has_path() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["warnings"][0]["path"], "name");
}

#[test]
fn json_warning_has_message() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    let message = json["results"][0]["warnings"][0]["message"].as_str().unwrap();
    assert!(message.contains("name"));
    assert!(message.contains("count"));
}

#[test]
fn json_warning_has_line() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["results"][0]["warnings"][0]["line"].as_u64().unwrap() > 0);
}

#[test]
fn json_warning_has_span() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    let span = &json["results"][0]["warnings"][0]["span"];
    assert!(span["start"]["line"].as_u64().unwrap() > 0);
    assert!(span["end"]["line"].as_u64().unwrap() > 0);
}

// ─── Warnings (human) ───

#[test]
fn human_warning_exits_with_zero() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "human"])
        .assert()
        .success();
}

#[test]
fn human_warning_output_contains_warning_prefix() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "human"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("warning[field-order]"));
}

// ─── --deny-warnings ───

#[test]
fn deny_warnings_exits_with_one_on_warning() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--deny-warnings"])
        .assert()
        .code(1);
}

#[test]
fn deny_warnings_exits_with_zero_when_no_warnings() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--deny-warnings"])
        .assert()
        .success();
}

#[test]
fn deny_warnings_json_exits_with_one_on_warning() {
    cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json", "--deny-warnings"])
        .assert()
        .code(1);
}

#[test]
fn deny_warnings_json_still_outputs_valid_json() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/out-of-order.ron", "--format", "json", "--deny-warnings"])
        .output()
        .unwrap();
    parse_json(&output.stdout);
}

// ─── No warnings on valid file ───

#[test]
fn json_valid_file_has_no_warnings() {
    let output = cmd()
        .args(["validate", "--schema", schema(), "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["results"][0]["warnings"].as_array().unwrap().is_empty());
}

// ─── Imports ───

fn importing_schema() -> &'static str {
    "tests/fixtures/importing.ronschema"
}

#[test]
fn import_valid_file_exits_with_zero() {
    cmd()
        .args(["validate", "--schema", importing_schema(), "tests/fixtures/import-valid.ron"])
        .assert()
        .success();
}

#[test]
fn import_valid_file_json_has_no_errors() {
    let output = cmd()
        .args(["validate", "--schema", importing_schema(), "tests/fixtures/import-valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["results"][0]["errors"].as_array().unwrap().is_empty());
}

#[test]
fn import_validates_imported_enum() {
    cmd()
        .args(["validate", "--schema", importing_schema(), "tests/fixtures/import-invalid-variant.ron"])
        .assert()
        .code(1);
}

#[test]
fn import_invalid_variant_json_has_error() {
    let output = cmd()
        .args(["validate", "--schema", importing_schema(), "tests/fixtures/import-invalid-variant.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["results"][0]["errors"][0]["code"], "invalid-variant");
}

#[test]
fn import_missing_file_exits_with_two() {
    cmd()
        .args(["validate", "--schema", "tests/fixtures/import-missing.ronschema", "tests/fixtures/valid.ron"])
        .assert()
        .code(2);
}

#[test]
fn import_missing_file_json_has_success_false() {
    let output = cmd()
        .args(["validate", "--schema", "tests/fixtures/import-missing.ronschema", "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert_eq!(json["success"], false);
}

#[test]
fn import_missing_file_json_has_error_message() {
    let output = cmd()
        .args(["validate", "--schema", "tests/fixtures/import-missing.ronschema", "tests/fixtures/valid.ron", "--format", "json"])
        .output()
        .unwrap();
    let json = parse_json(&output.stdout);
    assert!(json["error"].as_str().unwrap().contains("nonexistent.ronschema"));
}
