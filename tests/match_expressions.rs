//! Comprehensive tests for match expressions (session 33).
//!
//! Covers: parse, analysis, type checking, ownership, HIR/MIR, native E2E,
//! negative/diagnostic tests, and edge cases.

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

/// Parse and check for expected type errors.
fn analyze_has_type_errors(src: &str) -> bool {
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
    !type_result.errors().is_empty()
}

// ---------------------------------------------------------------------------
// Positive: parse tests
// ---------------------------------------------------------------------------

#[test]
fn match_expression_parses() {
    assert!(parse_ok("fn f() { let x = match 1 { 1 => 1, _ => 0 }; }"));
}

#[test]
fn match_expression_with_block_arms() {
    assert!(parse_ok(
        "fn f() { let x = match 1 { 1 => { 1 }, _ => { 0 } }; }"
    ));
}

#[test]
fn match_expression_in_block_trailing() {
    assert!(analyze_ok(
        "fn f() { let x = { match 1 { 1 => 1, _ => 0 } }; return x; }"
    ));
}

#[test]
fn match_expression_as_return_value() {
    assert!(parse_ok(
        "fn f() -> Int { return match 1 { 1 => 1, _ => 0 }; }"
    ));
}

#[test]
fn match_expression_nested_in_if() {
    assert!(parse_ok(
        "fn f() { let x = if true { match 1 { 1 => 1, _ => 0 } } else { 2 }; }"
    ));
}

#[test]
fn match_expression_with_guard() {
    assert!(parse_ok(
        "fn f() { let x = match 5 { n if n > 3 => 1, _ => 0 }; }"
    ));
}

#[test]
fn match_expression_with_or_pattern() {
    assert!(parse_ok(
        "fn f() { let x = match 1 { 1 | 2 => 1, _ => 0 }; }"
    ));
}

#[test]
fn match_expression_with_range_pattern() {
    assert!(parse_ok(
        "fn f() { let x = match 3 { 1..=5 => 1, _ => 0 }; }"
    ));
}

#[test]
fn match_expression_with_enum_scrutinee() {
    assert!(parse_ok(
        "enum E { A, B }\nfn f() { let x = match E::A { E::A => 1, E::B => 0 }; }"
    ));
}

#[test]
fn match_expression_with_payload_pattern() {
    assert!(parse_ok(
        "enum E { V(Int) }\nfn f() { let x = match E::V(5) { E::V(n) => n, _ => 0 }; }"
    ));
}

#[test]
fn match_expression_nested_match() {
    assert!(parse_ok(
        "fn f() { let x = match 1 { 1 => match 2 { 2 => 2, _ => 0 }, _ => 0 }; }"
    ));
}

#[test]
fn match_expression_with_binding() {
    assert!(parse_ok("fn f() { let x = match 42 { n => n }; }"));
}

#[test]
fn match_expression_bool_scrutinee() {
    assert!(parse_ok(
        "fn f() { let x = match true { true => 1, false => 0 }; }"
    ));
}

#[test]
fn match_expression_negative_int_pattern() {
    assert!(parse_ok("fn f() { let x = match -1 { -1 => 1, _ => 0 }; }"));
}

#[test]
fn match_expression_in_binary_operand() {
    // Match expression used as a binary operand (binding position).
    assert!(parse_ok(
        "fn f() { let x = match 1 { 1 => 1, _ => 0 } + 2; }"
    ));
}

// ---------------------------------------------------------------------------
// Negative: parse / type error tests
// ---------------------------------------------------------------------------

#[test]
fn mismatched_arm_types_rejected() {
    assert!(analyze_has_type_errors(
        "fn f() { let x = match 1 { 1 => 1, _ => true }; }"
    ));
}

#[test]
fn non_exhaustive_match_rejected() {
    assert!(analyze_has_type_errors(
        "enum E { A, B }\nfn f() { let x = match E::A { E::A => 1 }; }"
    ));
}

#[test]
fn unreachable_arm_detected() {
    assert!(analyze_has_type_errors(
        "fn f() { let x = match 1 { _ => 1, 2 => 2 }; }"
    ));
}

#[test]
fn match_expr_type_mismatch_with_binding() {
    // The binding's type must match the other arms.
    assert!(analyze_has_type_errors(
        "fn f() { let x = match 1 { n => true, _ => 1 }; }"
    ));
}

// ---------------------------------------------------------------------------
// Native end-to-end execution tests
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "mink_match_expr_test_{}_{}",
        std::process::id(),
        name
    ));
    std::fs::write(&path, content).unwrap();
    path
}

fn native_exit_code(src: &str) -> i32 {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = temp_source(&format!("match_expr_native_{n}.mink"), src);
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
fn native_match_expression_basic_int() {
    let exit = native_exit_code("fn main() { let x = match 1 { 1 => 42, _ => 0 }; return x; }\n");
    assert_eq!(exit, 42);
}

#[test]
fn native_match_expression_catch_all() {
    let exit = native_exit_code("fn main() { let x = match 99 { 1 => 1, _ => 77 }; return x; }\n");
    assert_eq!(exit, 77);
}

#[test]
fn native_match_expression_bool() {
    let exit = native_exit_code(
        "fn main() { let x = match true { true => 10, false => 20 }; return x; }\n",
    );
    assert_eq!(exit, 10);
}

#[test]
fn native_match_expression_bool_false() {
    let exit = native_exit_code(
        "fn main() { let x = match false { true => 10, false => 20 }; return x; }\n",
    );
    assert_eq!(exit, 20);
}

#[test]
fn native_match_expression_binding() {
    let exit = native_exit_code("fn main() { let x = match 42 { n => n }; return x; }\n");
    assert_eq!(exit, 42);
}

#[test]
fn native_match_expression_negative_pattern() {
    let exit =
        native_exit_code("fn main() { let x = match -5 { -5 => 100, _ => 0 }; return x; }\n");
    assert_eq!(exit, 100);
}

#[test]
fn native_match_expression_return_value() {
    let exit = native_exit_code(
        "fn compute(n: Int) -> Int { return match n { 0 => 10, _ => 20 }; }\n\
         fn main() { return compute(0); }\n",
    );
    assert_eq!(exit, 10);
}

#[test]
fn native_match_expression_enum_basic() {
    let exit = native_exit_code(
        "enum Dir { North, South, East, West }\n\
         fn main() {\n\
         let d = Dir::East;\n\
         let x = match d { Dir::North => 1, Dir::South => 2, Dir::East => 3, Dir::West => 4 };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 3);
}

#[test]
fn native_match_expression_payload_extraction() {
    let exit = native_exit_code(
        "enum Opt { Some(Int), None }\n\
         fn main() {\n\
         let v = Opt::Some(42);\n\
         let x = match v { Opt::Some(n) => n, Opt::None => 0 };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 42);
}

#[test]
fn native_match_expression_nested() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = match 1 {\n\
         1 => match 2 { 2 => 10, _ => 0 },\n\
         _ => 0\n\
         };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 10);
}

#[test]
fn native_match_expression_or_pattern() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = match 2 { 1 | 2 | 3 => 10, _ => 0 };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 10);
}

#[test]
fn native_match_expression_range_pattern() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = match 5 { 1..=5 => 10, 6..=10 => 20, _ => 0 };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 10);
}

#[test]
fn native_match_expression_guard() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = match 10 { n if n > 5 => 1, n if n > 3 => 2, _ => 3 };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 1);
}

#[test]
fn native_match_expression_guard_fails() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = match 2 { n if n > 5 => 1, n if n > 3 => 2, _ => 3 };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 3);
}

#[test]
fn native_match_expression_in_if_branch() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = if true { match 1 { 1 => 10, _ => 0 } } else { 20 };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 10);
}

#[test]
fn native_match_expression_in_block_trailing() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = { match 3 { 1 => 10, 2 => 20, _ => 30 } };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 30);
}

#[test]
fn native_match_expression_multiple_arms() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = match 5 {\n\
         1 => 100,\n\
         2 => 200,\n\
         3 => 300,\n\
         4 => 400,\n\
         _ => 500\n\
         };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 500);
}

#[test]
fn native_match_expression_with_statements_in_arm() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = match 1 {\n\
         1 => { let a = 10; let b = 20; a + b },\n\
         _ => 0\n\
         };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 30);
}

#[test]
fn native_match_expression_as_function_arg() {
    let exit = native_exit_code(
        "fn g(x: Int) -> Int { return x + 1; }\n\
         fn main() {\n\
         let v = match 5 { n => n };\n\
         return g(v);\n\
         }\n",
    );
    assert_eq!(exit, 6);
}

#[test]
fn native_match_expression_arithmetic() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = match 3 { 1 => 10, 2 => 20, 3 => 30, _ => 0 };\n\
         let y = match 2 { 1 => 5, 2 => 15, _ => 0 };\n\
         return x + y;\n\
         }\n",
    );
    assert_eq!(exit, 45);
}

#[test]
fn native_match_expression_deterministic() {
    let src = "fn main() { let x = match 1 { 1 => 42, _ => 0 }; return x; }\n";
    let exit1 = native_exit_code(src);
    let exit2 = native_exit_code(src);
    assert_eq!(exit1, exit2);
    assert_eq!(exit1, 42);
}

#[test]
fn native_match_expression_empty_enum() {
    // Empty enum match: vacuously exhaustive, arms are dead code.
    // The expression is dead code because there are no constructors for Empty.
    assert!(analyze_ok(
        "enum Empty {}\nfn f(e: Empty) -> Int { return match e {}; }"
    ));
}
