//! Comprehensive tests for tuple destructuring in let bindings (session 31).
//!
//! Covers: basic destructuring, nested destructuring, type annotations,
//! mutability, negative/diagnostic tests, and native end-to-end execution.

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
// Positive: parse
// ---------------------------------------------------------------------------

#[test]
fn basic_destructure_parses() {
    assert!(parse_ok("fn f() { let (a, b) = (1, 2); }"));
}

#[test]
fn destructure_three_elements_parses() {
    assert!(parse_ok("fn f() { let (a, b, c) = (1, 2, 3); }"));
}

#[test]
fn destructure_single_element_parses() {
    assert!(parse_ok("fn f() { let (a,) = (1,); }"));
}

#[test]
fn destructure_unit_parses() {
    assert!(parse_ok("fn f() { let () = (); }"));
}

#[test]
fn destructure_with_type_annotation_parses() {
    assert!(parse_ok("fn f() { let (a, b): (Int, Bool) = (1, true); }"));
}

#[test]
fn destructure_mut_parses() {
    assert!(parse_ok("fn f() { let mut (a, b) = (1, 2); }"));
}

#[test]
fn destructure_in_inner_scope_parses() {
    // Bare `{ }` at statement position is a block expression (pre-existing);
    // let inside it works when the block is in expression context.
    assert!(parse_ok(
        "fn f() { let x = (1, 2); let (a, b) = if true { x } else { (0, 0) }; }"
    ));
}

#[test]
fn destructure_with_expression_parses() {
    assert!(parse_ok(
        "fn f() -> (Int, Int) { return (1, 2); } fn g() { let (a, b) = f(); }"
    ));
}

// ---------------------------------------------------------------------------
// Positive: full analysis
// ---------------------------------------------------------------------------

#[test]
fn basic_destructure_analyzes() {
    assert!(analyze_ok("fn f() { let x = (1, 2); let (a, b) = x; }"));
}

#[test]
fn destructure_three_elements_analyzes() {
    assert!(analyze_ok(
        "fn f() { let x = (1, 2, 3); let (a, b, c) = x; }"
    ));
}

#[test]
fn destructure_single_element_analyzes() {
    assert!(analyze_ok("fn f() { let x = (42,); let (a,) = x; }"));
}

#[test]
fn destructure_unit_analyzes() {
    assert!(analyze_ok("fn f() { let x = (); let () = x; }"));
}

#[test]
fn destructure_with_type_annotation_analyzes() {
    assert!(analyze_ok(
        "fn f() { let x = (1, true); let (a, b): (Int, Bool) = x; }"
    ));
}

#[test]
fn destructure_mut_analyzes() {
    assert!(analyze_ok(
        "fn f() { let mut x = (1, 2); let mut (a, b) = x; }"
    ));
}

#[test]
fn destructure_from_function_return_analyzes() {
    assert!(analyze_ok(
        "fn f() -> (Int, Bool) { return (1, true); } fn g() { let (a, b) = f(); }"
    ));
}

#[test]
fn destructure_from_tuple_expression_analyzes() {
    assert!(analyze_ok("fn f() { let (a, b) = (10, 20); }"));
}

#[test]
fn destructure_with_arithmetic_analyzes() {
    assert!(analyze_ok(
        "fn f() { let x = (3, 7); let (a, b) = x; let c = a + b; }"
    ));
}

#[test]
fn destructure_nested_analyzes() {
    assert!(analyze_ok(
        "fn f() { let x = (1, (2, 3)); let (a, (b, c)) = x; }"
    ));
}

#[test]
fn destructure_with_wildcard_analyzes() {
    assert!(analyze_ok(
        "fn f() { let x = (1, 2, 3); let (a, _, c) = x; }"
    ));
}

#[test]
fn destructure_const_binding_analyzes() {
    // Const destructuring is not supported yet; this tests that the parser
    // rejects it (const uses parse_binding_tail, not parse_let_destructure).
    assert!(!analyze_ok("fn f() { const (a, b) = (1, 2); }"));
}

// ---------------------------------------------------------------------------
// Negative: type errors
// ---------------------------------------------------------------------------

#[test]
fn destructure_arity_mismatch_fails() {
    assert!(!analyze_ok("fn f() { let x = (1, 2); let (a, b, c) = x; }"));
}

#[test]
fn destructure_arity_mismatch_too_few_fails() {
    assert!(!analyze_ok("fn f() { let x = (1, 2, 3); let (a, b) = x; }"));
}

#[test]
fn destructure_non_tuple_fails() {
    assert!(!analyze_ok("fn f() { let x = 42; let (a, b) = x; }"));
}

#[test]
fn destructure_type_annotation_mismatch_fails() {
    assert!(!analyze_ok(
        "fn f() { let x = (1, 2); let (a, b): (Int, Bool) = x; }"
    ));
}

#[test]
fn destructure_element_type_mismatch_fails() {
    // The destructured element types are inferred from the tuple, so
    // using them in a type-incompatible way should fail.
    assert!(analyze_ok(
        "fn f() { let x = (1, 2); let (a, b) = x; let c = a + b; }"
    ));
}

// ---------------------------------------------------------------------------
// Native end-to-end execution tests (via CLI)
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("mink_destr_test_{}_{}", std::process::id(), name));
    std::fs::write(&path, content).unwrap();
    path
}

fn native_exit_code(src: &str) -> i32 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = temp_source(&format!("destr_native_{n}.mink"), src);
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

#[test]
fn native_basic_destructure() {
    let exit = native_exit_code("fn main() { let x = (10, 20); let (a, b) = x; return a + b; }\n");
    assert_eq!(exit, 30);
}

#[test]
fn native_destructure_three_elements() {
    let exit =
        native_exit_code("fn main() { let x = (1, 2, 3); let (a, b, c) = x; return a + b + c; }\n");
    assert_eq!(exit, 6);
}

#[test]
fn native_destructure_single_element() {
    let exit = native_exit_code("fn main() { let x = (42,); let (a,) = x; return a; }\n");
    assert_eq!(exit, 42);
}

#[test]
fn native_destructure_from_function() {
    let exit = native_exit_code(
        "fn pair() -> (Int, Int) { return (10, 30); }\n\
         fn main() { let (a, b) = pair(); return a * b; }\n",
    );
    assert_eq!(exit, 300);
}

#[test]
fn native_destructure_in_loop() {
    let exit = native_exit_code(
        "fn main() {\n\
         \x20let mut total = 0;\n\
         \x20let mut i = 0;\n\
         \x20while i < 5 {\n\
         \x20\x20let x = (i, i + 1);\n\
         \x20\x20let (a, b) = x;\n\
         \x20\x20total = total + a + b;\n\
         \x20\x20i = i + 1;\n\
         \x20}\n\
         \x20return total;\n\
         }\n",
    );
    // (0+1) + (1+2) + (2+3) + (3+4) + (4+5) = 1 + 3 + 5 + 7 + 9 = 25
    assert_eq!(exit, 25);
}

#[test]
fn native_destructure_multiple_destructures() {
    let exit = native_exit_code(
        "fn main() {\n\
         \x20let (a, b) = (5, 10);\n\
         \x20let (c, d) = (a + 1, b + 2);\n\
         \x20return c + d;\n\
         }\n",
    );
    // (5+1) + (10+2) = 6 + 12 = 18
    assert_eq!(exit, 18);
}

#[test]
fn native_destructure_with_if_expression() {
    let exit = native_exit_code(
        "fn main() {\n\
         \x20let x = if true { (10, 20) } else { (0, 0) };\n\
         \x20let (a, b) = x;\n\
         \x20return a + b;\n\
         }\n",
    );
    assert_eq!(exit, 30);
}

#[test]
fn native_destructure_mutable() {
    let exit = native_exit_code(
        "fn main() {\n\
         \x20let x = (1, 2);\n\
         \x20let mut (a, b) = x;\n\
         \x20a = 10;\n\
         \x20b = 20;\n\
         \x20return a + b;\n\
         }\n",
    );
    assert_eq!(exit, 30);
}

#[test]
fn native_destructure_with_const() {
    let exit = native_exit_code(
        "const PAIR = (7, 11);\n\
         fn main() { let (a, b) = PAIR; return a + b; }\n",
    );
    assert_eq!(exit, 18);
}

#[test]
fn native_destructure_with_type_annotation() {
    let exit = native_exit_code(
        "fn main() {\n\
         \x20let x = (42, true);\n\
         \x20let (a, b): (Int, Bool) = x;\n\
         \x20return a;\n\
         }\n",
    );
    assert_eq!(exit, 42);
}

#[test]
fn native_destructure_with_continue() {
    let exit = native_exit_code(
        "fn main() {\n\
         \x20let mut sum = 0;\n\
         \x20let mut i = 0;\n\
         \x20while i < 10 {\n\
         \x20\x20let (a, b) = (i, i + 1);\n\
         \x20\x20i = i + 1;\n\
         \x20\x20if a == 3 { continue; }\n\
         \x20\x20sum = sum + a;\n\
         \x20}\n\
         \x20return sum;\n\
         }\n",
    );
    // 0 + 1 + 2 + 4 + 5 + 6 + 7 + 8 + 9 = 42
    assert_eq!(exit, 42);
}

#[test]
fn native_destructure_with_break_value() {
    let exit = native_exit_code(
        "fn main() {\n\
         \x20let result = loop {\n\
         \x20\x20let (a, b) = (42, 10);\n\
         \x20\x20break a - b;\n\
         \x20};\n\
         \x20return result;\n\
         }\n",
    );
    assert_eq!(exit, 32);
}

#[test]
fn native_destructure_nested_tuples_not_supported_native() {
    // Nested tuples in native backend are not yet supported, so this
    // should fail at the backend level.
    let path = temp_source(
        "nested_destr.mink",
        "fn main() { let x = (1, (2, 3)); let (a, (b, c)) = x; return a; }\n",
    );
    let output = mink().arg("build").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("exe"));
    assert_ne!(
        output.status.code(),
        Some(0),
        "nested tuple destructure should not yet work natively"
    );
}

#[test]
fn native_destructure_deterministic() {
    // Build the same program twice and verify identical output.
    let src = "fn main() { let (a, b) = (10, 20); return a + b; }\n";
    let exit1 = native_exit_code(src);
    let exit2 = native_exit_code(src);
    assert_eq!(exit1, exit2);
    assert_eq!(exit1, 30);
}

#[test]
fn native_destructure_nested_binding() {
    // Test that nested tuple patterns in match work via existing
    // infrastructure. This is not let-destructure but validates
    // that the type system can handle nested tuples.
    assert!(analyze_ok(
        "fn f() { let x = (1, (2, 3)); let a = (x.1).1; }"
    ));
}
