//! Integration tests for the MINK JSON library (Session 52).

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn json_lib() -> String {
    std::fs::read_to_string("stdlib/json.mink").expect("failed to read stdlib/json.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_json_test_{n}_{name}.mink"));
    std::fs::write(&path, content.replace("\r\n", "\n")).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, Vec<u8>) {
    let lib = json_lib();
    let source = format!("{}\n{}", lib, test_body);
    let path = temp_source("test", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        panic!("build failed:\n{stderr}");
    }
    let run = Command::new(&exe).output().unwrap();
    let code = run.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, run.stdout)
}

/// Escape a string for use as a MINK string literal
fn mink_str(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Assert that raw JSON text parses successfully
fn assert_parse_ok(json: &str) {
    let mink_literal = mink_str(json);
    let test = format!(
        r#"
fn main() -> Int {{
    let arena = rt_alloc(65536);
    let v = json_parse({mink_literal}, arena);
    if v == 0 {{ rt_free(arena); rt_exit(1); }}
    rt_free(arena);
    rt_exit(0);
}}"#
    );
    let (code, _) = build_and_run(&test);
    assert_eq!(
        code, 0,
        "expected parse of {json:?} to succeed (exit code: {code})"
    );
}

/// Assert that raw JSON text fails to parse
fn assert_parse_err(json: &str) {
    let mink_literal = mink_str(json);
    let test = format!(
        r#"
fn main() -> Int {{
    let arena = rt_alloc(65536);
    let v = json_parse({mink_literal}, arena);
    if v == 0 {{ rt_free(arena); rt_exit(0); }}
    rt_free(arena);
    rt_exit(1);
}}"#
    );
    let (code, _) = build_and_run(&test);
    assert_eq!(code, 0, "expected parse of {json:?} to fail");
}

// =========================================================================
// Primitives
// =========================================================================

#[test]
fn parse_null() {
    assert_parse_ok("null");
}

#[test]
fn parse_true() {
    assert_parse_ok("true");
}

#[test]
fn parse_false() {
    assert_parse_ok("false");
}

#[test]
fn parse_int_zero() {
    assert_parse_ok("0");
}

#[test]
fn parse_int_positive() {
    assert_parse_ok("42");
}

#[test]
fn parse_int_negative() {
    assert_parse_ok("-7");
}

#[test]
fn parse_int_large() {
    assert_parse_ok("1234567890");
}

#[test]
fn parse_empty_string() {
    assert_parse_ok("\"\"");
}

#[test]
fn parse_hello_string() {
    assert_parse_ok("\"hello\"");
}

#[test]
fn parse_empty_array() {
    assert_parse_ok("[]");
}

#[test]
fn parse_empty_object() {
    assert_parse_ok("{}");
}

// =========================================================================
// Invalid inputs
// =========================================================================

#[test]
fn reject_empty_input() {
    assert_parse_err("");
}

#[test]
fn reject_trailing_garbage() {
    assert_parse_err("true false");
}

#[test]
fn reject_trailing_char() {
    assert_parse_err("trueX");
}

#[test]
fn reject_unclosed_string() {
    assert_parse_err("\"hello");
}

#[test]
fn reject_unclosed_array() {
    assert_parse_err("[1, 2");
}

#[test]
fn reject_unclosed_object() {
    assert_parse_err("{\"a\": 1");
}

#[test]
fn reject_missing_colon() {
    assert_parse_err("{\"a\" 1}");
}

#[test]
fn reject_missing_value() {
    assert_parse_err("{\"a\":}");
}

#[test]
fn reject_trailing_comma_array() {
    assert_parse_err("[1,]");
}

#[test]
fn reject_trailing_comma_object() {
    assert_parse_err("{\"a\":1,}");
}

// =========================================================================
// Whitespace
// =========================================================================

#[test]
fn whitespace_before_value() {
    assert_parse_ok("  true");
}

#[test]
fn whitespace_after_value() {
    assert_parse_ok("true  ");
}

#[test]
fn whitespace_around_value() {
    assert_parse_ok("  \n  true  \n  ");
}

#[test]
fn whitespace_in_array() {
    assert_parse_ok("[ 1 , 2 , 3 ]");
}

#[test]
fn whitespace_in_object() {
    assert_parse_ok("{ \"a\" : 1 }");
}

// =========================================================================
// Strings with escapes
// =========================================================================

#[test]
fn string_with_newline_escape() {
    assert_parse_ok("\"a\\nb\"");
}

#[test]
fn string_with_tab_escape() {
    assert_parse_ok("\"a\\tb\"");
}

#[test]
fn string_with_backslash_escape() {
    assert_parse_ok("\"a\\\\b\"");
}

#[test]
fn string_with_quote_escape() {
    assert_parse_ok("\"a\\\"b\"");
}

#[test]
fn string_with_slash_escape() {
    assert_parse_ok("\"a\\/b\"");
}

// =========================================================================
// Adversarial
// =========================================================================

#[test]
fn single_brace() {
    assert_parse_err("{");
}

#[test]
fn single_bracket() {
    assert_parse_err("[");
}

#[test]
fn just_number_sign() {
    assert_parse_err("-");
}

#[test]
fn double_open_bracket() {
    assert_parse_err("[[1]");
}

#[test]
fn invalid_literal_nulll() {
    assert_parse_err("nulll");
}

#[test]
fn invalid_literal_truee() {
    assert_parse_err("truee");
}
