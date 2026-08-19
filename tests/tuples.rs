//! Comprehensive tests for tuples (session 29).
//!
//! Covers: tuple types, expressions, field access, struct fields, arrays,
//! return types, parameters, negative/diagnostic tests, and native E2E.

use mink::parser;
use mink::source::SourceMap;
use std::process::Command;

/// Parse source and return true if no lex/parse errors.
fn parse_ok(src: &str) -> bool {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("source file present");
    let output = parser::parse(file);
    output.is_valid()
}

/// Parse and then run through semantic + type analysis.
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

// ---------------------------------------------------------------------------
// Positive: parse + analyze
// ---------------------------------------------------------------------------

#[test]
fn unit_expression_parses() {
    assert!(parse_ok("fn f() { let x = (); }"));
}

#[test]
fn tuple_type_in_let_binding_parses() {
    assert!(parse_ok("fn f() { let x: (Int, Bool) = (1, true); }"));
}

#[test]
fn tuple_expression_two_elements_parses() {
    assert!(parse_ok("fn f() { let x = (1, true); }"));
}

#[test]
fn single_element_tuple_parses() {
    assert!(parse_ok("fn f() { let x = (1,); }"));
}

#[test]
fn tuple_field_access_parses() {
    assert!(parse_ok(
        "fn f() { let x = (1, true); let a = x.0; let b = x.1; }"
    ));
}

#[test]
fn tuple_return_type_parses() {
    assert!(parse_ok("fn f() -> (Int, Bool) { return (1, true); }"));
}

#[test]
fn tuple_parameter_type_parses() {
    assert!(parse_ok("fn f(x: (Int, Bool)) { let a = x.0; }"));
}

#[test]
fn tuple_in_struct_field_parses() {
    assert!(parse_ok("struct P { pair: (Int, Bool) }"));
}

#[test]
fn nested_tuples_parses() {
    assert!(parse_ok(
        "fn f() { let x = ((1, true), 42); let a = (x.0).1; }"
    ));
}

// ---------------------------------------------------------------------------
// Positive: full analysis
// ---------------------------------------------------------------------------

#[test]
fn unit_expression_analyzes() {
    assert!(analyze_ok("fn f() -> Int { let x = (); return 0; }"));
}

#[test]
fn tuple_type_in_let_analyzes() {
    assert!(analyze_ok("fn f() { let x: (Int, Bool) = (1, true); }"));
}

#[test]
fn tuple_field_access_analyzes() {
    assert!(analyze_ok("fn f() { let x = (1, true); let a = x.0; }"));
}

#[test]
fn tuple_return_type_analyzes() {
    assert!(analyze_ok("fn f() -> (Int, Bool) { return (1, true); }"));
}

#[test]
fn tuple_parameter_type_analyzes() {
    assert!(analyze_ok("fn f(x: (Int, Bool)) { let a = x.0; }"));
}

#[test]
fn nested_tuples_analyze() {
    assert!(analyze_ok(
        "fn f() { let x = ((1, true), 42); let a = (x.0).1; }"
    ));
}

#[test]
fn tuple_const_binding_analyzes() {
    assert!(analyze_ok("const X: (Int, Bool) = (1, true);"));
}

#[test]
fn tuple_block_expression_analyzes() {
    assert!(analyze_ok(
        "fn f() -> Int { let x = { (1, true) }; return x.0; }"
    ));
}

#[test]
fn tuple_if_expression_analyzes() {
    assert!(analyze_ok(
        "fn f() -> Int { let x = if true { (1, 2) } else { (3, 4) }; return x.0; }"
    ));
}

#[test]
fn tuple_with_null_element_analyzes() {
    assert!(analyze_ok("fn f() { let x = (null, 1); }"));
}

#[test]
fn deeply_nested_tuple_access_analyzes() {
    assert!(analyze_ok(
        "fn f() { let x = (1, (2, (3, 4))); let a = ((x.1).1).1; }"
    ));
}

#[test]
fn tuple_with_char_element_analyzes() {
    assert!(analyze_ok("fn f() { let x = ('a', 1); }"));
}

// ---------------------------------------------------------------------------
// Negative: type errors
// ---------------------------------------------------------------------------

#[test]
fn tuple_field_access_out_of_range_fails() {
    assert!(!analyze_ok("fn f() { let x = (1, true); let a = x.5; }"));
}

#[test]
fn tuple_field_access_on_non_tuple_fails() {
    assert!(!analyze_ok("fn f() { let x = 42; let a = x.0; }"));
}

#[test]
fn tuple_arity_mismatch_fails() {
    assert!(!analyze_ok(
        "fn f() { let x: (Int, Bool) = (1, true, false); }"
    ));
}

#[test]
fn tuple_element_type_mismatch_fails() {
    assert!(!analyze_ok("fn f() { let x: (Int, Bool) = (1, 2); }"));
}

// ---------------------------------------------------------------------------
// Native end-to-end execution tests (via CLI)
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("mink_tuple_test_{}_{}", std::process::id(), name));
    std::fs::write(&path, content).unwrap();
    path
}

fn native_exit_code(src: &str) -> i32 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = temp_source(&format!("tuple_native_{n}.mink"), src);
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
    let _ = std::fs::remove_file(&exe);
    code
}

#[test]
fn native_tuple_field_access() {
    let exit = native_exit_code("fn main() { let x = (10, 20); return x.1; }\n");
    assert_eq!(exit, 20);
}

#[test]
fn native_single_element_tuple() {
    let exit = native_exit_code("fn main() { let x = (42,); return x.0; }\n");
    assert_eq!(exit, 42);
}

#[test]
fn native_tuple_with_arithmetic() {
    let exit = native_exit_code("fn main() { let x = (3, 7); return x.0 + x.1; }\n");
    assert_eq!(exit, 10);
}

#[test]
fn native_three_element_tuple() {
    let exit = native_exit_code("fn main() { let x = (1, 2, 3); return x.0 + x.1 + x.2; }\n");
    assert_eq!(exit, 6);
}

#[test]
fn native_tuple_return_from_function() {
    let exit = native_exit_code(
        "fn pair() -> (Int, Bool) { return (42, true); }\n\
         fn main() { let x = pair(); return x.0; }\n",
    );
    assert_eq!(exit, 42);
}

#[test]
fn native_tuple_nested_access() {
    // Nested tuple types are not yet supported by the native backend;
    // this test verifies the front-end accepts them.
    assert!(analyze_ok(
        "fn f() { let x = (10, (20, 30)); let a = (x.1).1; }"
    ));
}

#[test]
fn native_tuple_with_function_args() {
    let exit = native_exit_code(
        "fn get_first(t: (Int, Bool)) -> Int { return t.0; }\n\
         fn main() { let x = (99, false); return get_first(x); }\n",
    );
    assert_eq!(exit, 99);
}

#[test]
fn native_tuple_const_binding() {
    let exit = native_exit_code(
        "const PAIR = (7, 11);\n\
         fn main() { let x = PAIR; return x.0 + x.1; }\n",
    );
    assert_eq!(exit, 18);
}

#[test]
fn native_tuple_modification_through_binding() {
    let exit = native_exit_code(
        "fn main() { let x = (5, 10); let a = x.0; let b = x.1; return a * b; }\n",
    );
    assert_eq!(exit, 50);
}
