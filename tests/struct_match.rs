//! Struct patterns in `match` statements and `match` expressions.
//!
//! The feature destructures a struct scrutinee field-by-field. Field patterns
//! may be bindings, wildcards, literals, nested structs, enum payloads,
//! tuples, or or-patterns. A struct pattern is exhaustive by itself only when
//! every field pattern is irrefutable; refutable field patterns need a
//! catch-all arm (`_` or a binding), matching the existing exhaustiveness
//! rules for scalars and enums.

use mink::parser;
use mink::source::SourceMap;
use std::process::Command;

fn parse_ok(src: &str) -> bool {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("source file present");
    parser::parse(file).is_valid()
}

fn analyze_ok(src: &str) -> bool {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("source file present");
    let parsed = parser::parse(file);
    if !parsed.is_valid() {
        return false;
    }
    let semantic = mink::semantics::analyze(parsed.ast());
    if !semantic.errors().is_empty() {
        return false;
    }
    let type_result = mink::typecheck::check(parsed.ast(), &semantic, &map);
    type_result.errors().is_empty()
}

/// Parse and type-check a source known to be syntactically valid, returning
/// the stable type-error codes in source order.
fn type_codes(src: &str) -> Vec<String> {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("source file present");
    let parsed = parser::parse(file);
    assert!(
        parsed.is_valid(),
        "test source must parse: {:?}",
        parsed.parse_errors()
    );
    let semantic = mink::semantics::analyze(parsed.ast());
    assert!(
        semantic.errors().is_empty(),
        "test source must be semantically clean: {:?}",
        semantic.errors()
    );
    let types = mink::typecheck::check(parsed.ast(), &semantic, &map);
    types.errors().iter().map(|error| error.code().to_string()).collect()
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn native_exit_code(src: &str) -> i32 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "mink_struct_match_test_{}_{}.mink",
        std::process::id(),
        n
    ));
    std::fs::write(&path, src).unwrap();
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.code() == Some(0),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    let run = Command::new(&exe).status().unwrap();
    let code = run.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(exe);
    code
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[test]
fn parser_accepts_struct_pattern_statement() {
    assert!(parse_ok(
        "struct P { x: Int, y: Int } fn f(p: P) { match p { P { x, y } => { let z = x + y; } } }"
    ));
}

#[test]
fn parser_accepts_struct_pattern_expression() {
    assert!(parse_ok(
        "struct P { x: Int, y: Int } fn f(p: P) -> Int { return match p { P { x, y } => x + y }; }"
    ));
}

#[test]
fn parser_accepts_nested_struct_pattern_and_trailing_comma() {
    assert!(parse_ok(
        "struct I { a: Int } struct O { i: I, b: Int } fn f(o: O) { match o { O { i: I { a, }, b, } => { let z = a + b; } } }"
    ));
}

#[test]
fn parser_accepts_empty_struct_pattern() {
    assert!(parse_ok(
        "struct P { } fn f(p: P) { match p { P {} => { } } }"
    ));
}

#[test]
fn parser_rejects_unclosed_struct_pattern() {
    assert!(!parse_ok(
        "struct P { x: Int } fn f(p: P) { match p { P { x => { } } }"
    ));
}

// ---------------------------------------------------------------------------
// Type checking and diagnostics
// ---------------------------------------------------------------------------

#[test]
fn typecheck_irrefutable_struct_pattern_is_exhaustive() {
    assert!(analyze_ok(
        "struct P { x: Int, y: Int } fn f(p: P) -> Int { return match p { P { x, y } => x + y }; }"
    ));
}

#[test]
fn typecheck_refutable_struct_pattern_requires_catch_all() {
    let codes = type_codes(
        "struct P { x: Int, y: Int } fn f(p: P) -> Int { return match p { P { x: 0, y } => y }; }",
    );
    assert!(
        codes.contains(&"E-T24".to_string()),
        "expected E-T24, got {codes:?}"
    );
}

#[test]
fn typecheck_unknown_field_is_e_t39() {
    let codes = type_codes(
        "struct P { x: Int } fn f(p: P) -> Int { return match p { P { x, z } => x, _ => 0 }; }",
    );
    assert!(
        codes.contains(&"E-T39".to_string()),
        "expected E-T39, got {codes:?}"
    );
}

#[test]
fn typecheck_missing_field_is_e_t40_without_false_unreachable() {
    let codes = type_codes(
        "struct P { x: Int, y: Int } fn f(p: P) -> Int { return match p { P { x } => x, _ => 0 }; }",
    );
    assert!(
        codes.contains(&"E-T40".to_string()),
        "expected E-T40, got {codes:?}"
    );
    assert!(
        !codes.contains(&"E-T25".to_string()),
        "a malformed pattern must not be reported as an unreachable arm: {codes:?}"
    );
}

#[test]
fn typecheck_wrong_struct_name_is_e_t41() {
    let codes = type_codes(
        "struct A { x: Int } struct B { x: Int } fn f(a: A) -> Int { return match a { B { x } => x }; }",
    );
    assert!(
        codes.contains(&"E-T41".to_string()),
        "expected E-T41, got {codes:?}"
    );
}

#[test]
fn typecheck_struct_pattern_on_non_struct_is_e_t41() {
    let codes = type_codes(
        "struct P { x: Int } fn f() -> Int { return match 1 { P { x } => x, _ => 0 }; }",
    );
    assert!(
        codes.contains(&"E-T41".to_string()),
        "expected E-T41, got {codes:?}"
    );
}

#[test]
fn typecheck_field_pattern_type_mismatch_is_e_t01() {
    let codes = type_codes(
        "struct P { x: Int, y: Int } fn f(p: P) -> Int { return match p { P { x: true, y } => y, _ => 0 }; }",
    );
    assert!(
        codes.contains(&"E-T01".to_string()),
        "expected E-T01, got {codes:?}"
    );
}

#[test]
fn typecheck_non_bool_guard_is_e_t01() {
    let codes = type_codes(
        "struct P { x: Int, y: Int } fn f(p: P) -> Int { return match p { P { x, y } if 1 => x, _ => 0 }; }",
    );
    assert!(
        codes.contains(&"E-T01".to_string()),
        "expected E-T01, got {codes:?}"
    );
}

#[test]
fn typecheck_or_pattern_requires_same_bindings_in_struct_fields() {
    let codes = type_codes(
        "struct P { x: Int, y: Int } fn f(p: P) -> Int { return match p { P { x: a, y } | P { x, y: b } => a, _ => 0 }; }",
    );
    assert!(
        codes.contains(&"E-T34".to_string()),
        "expected E-T34, got {codes:?}"
    );
}

#[test]
fn typecheck_nested_struct_field_pattern() {
    assert!(analyze_ok(
        "struct I { a: Int } struct O { i: I, b: Int } fn f(o: O) -> Int { return match o { O { i: I { a }, b } => a + b }; }"
    ));
}

#[test]
fn typecheck_struct_pattern_in_enum_payload() {
    assert!(analyze_ok(
        "enum E { V(P), U } struct P { x: Int, y: Int } fn f(e: E) -> Int { return match e { E::V(P { x, y }) => x + y, E::U => 0 }; }"
    ));
}

// ---------------------------------------------------------------------------
// Native execution
// ---------------------------------------------------------------------------

#[test]
fn native_struct_match_statement() {
    let exit = native_exit_code(
        "struct P { x: Int, y: Int } fn main() -> Int { let p = P { x: 3, y: 4 }; match p { P { x, y } => { return x * y; } } }\n",
    );
    assert_eq!(exit, 12);
}

#[test]
fn native_struct_match_expression() {
    let exit = native_exit_code(
        "struct P { x: Int, y: Int } fn classify(p: P) -> Int { return match p { P { x: 0, y: 7 } => 100, P { x: 0, y } => y, P { x, y } => x + y }; } fn main() -> Int { return classify(P { x: 0, y: 5 }); }\n",
    );
    assert_eq!(exit, 5);
}

#[test]
fn native_struct_match_literal_field() {
    let exit = native_exit_code(
        "struct P { x: Int, y: Int } fn main() -> Int { let p = P { x: 0, y: 7 }; match p { P { x: 0, y: 7 } => { return 100; }, P { x: 0, y } => { return y; }, P { x, y } => { return x + y; } } }\n",
    );
    assert_eq!(exit, 100);
}

#[test]
fn native_struct_match_nested_and_guard() {
    let exit = native_exit_code(
        "struct I { a: Int, b: Int } struct O { i: I, tag: Int } fn main() -> Int { let o = O { i: I { a: 4, b: 6 }, tag: 2 }; match o { O { i: I { a, b }, tag: 1 } => { return 1; }, O { i: I { a, b }, tag } if tag == 2 => { return a + b; }, _ => { return 0; } } }\n",
    );
    assert_eq!(exit, 10);
}

#[test]
fn native_struct_match_or_pattern() {
    let exit = native_exit_code(
        "struct P { x: Int, y: Int } fn main() -> Int { let p = P { x: 1, y: 5 }; match p { P { x: 1, y: v } | P { x: v, y: 1 } => { return v; }, P { x, y } => { return x + y; } } }\n",
    );
    assert_eq!(exit, 5);
}

#[test]
fn native_struct_match_wildcard_field_and_rename() {
    let exit = native_exit_code(
        "struct P { x: Int, y: Int } fn main() -> Int { let p = P { x: 10, y: 20 }; match p { P { x: _, y: b } => { return b; } } }\n",
    );
    assert_eq!(exit, 20);
}

#[test]
fn native_struct_pattern_in_enum_payload() {
    let exit = native_exit_code(
        "struct P { x: Int, y: Int } enum E { V(P), U } fn main() -> Int { let e = E::V(P { x: 8, y: 9 }); match e { E::V(P { x, y }) => { return x + y; }, E::U => { return 0; } } }\n",
    );
    assert_eq!(exit, 17);
}
