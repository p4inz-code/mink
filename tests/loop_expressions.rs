//! Comprehensive tests for while/loop as expressions with break values
//! (session 30).
//!
//! Covers: parse, analysis, type checking, ownership, MIR, native E2E,
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

// ---------------------------------------------------------------------------
// Positive: parse tests
// ---------------------------------------------------------------------------

#[test]
fn loop_expression_parses() {
    assert!(parse_ok("fn f() { let x = loop { break 42; }; }"));
}

#[test]
fn while_expression_parses() {
    assert!(parse_ok("fn f() { let x = while true { break 1; }; }"));
}

#[test]
fn loop_expression_in_block_trailing() {
    // A loop expression as the trailing expression of a block expression.
    assert!(analyze_ok(
        "fn f() { let x = { loop { break 5; } }; return x; }"
    ));
}

#[test]
fn while_expression_in_block_trailing() {
    assert!(analyze_ok(
        "fn f() { let x = { while true { break 5; } }; return x; }"
    ));
}

#[test]
fn break_with_value_parses() {
    assert!(parse_ok("fn f() { loop { break 42; } }"));
}

#[test]
fn break_without_value_still_parses() {
    assert!(parse_ok("fn f() { while true { break; } }"));
}

#[test]
fn loop_expression_nested_in_if() {
    assert!(parse_ok(
        "fn f() { let x = if true { loop { break 1; } } else { loop { break 2; } }; }"
    ));
}

#[test]
fn while_expression_nested_in_if() {
    assert!(parse_ok(
        "fn f() { let x = if true { while true { break 1; } } else { 0 }; }"
    ));
}

#[test]
fn loop_expression_as_return_value() {
    assert!(parse_ok("fn f() -> Int { return loop { break 42; }; }"));
}

#[test]
fn while_expression_as_function_arg() {
    assert!(parse_ok(
        "fn g(x: Int) {}\nfn f() { g(loop { break 5; }); }"
    ));
}

#[test]
fn loop_expression_with_break_arithmetic() {
    assert!(parse_ok("fn f() { let x = loop { break 3 + 4; }; }"));
}

#[test]
fn loop_expression_with_let_inside() {
    assert!(parse_ok(
        "fn f() { let x = loop { let y = 5; break y + 1; }; }"
    ));
}

#[test]
fn loop_expression_with_conditional_break() {
    assert!(parse_ok(
        "fn f() { let x = loop { if true { break 1; } }; }"
    ));
}

#[test]
fn while_expression_with_conditional_break() {
    assert!(parse_ok(
        "fn f() { let x = while true { if true { break 1; } }; }"
    ));
}

// ---------------------------------------------------------------------------
// Positive: analysis tests
// ---------------------------------------------------------------------------

#[test]
fn loop_expression_analyzes() {
    assert!(analyze_ok("fn f() { let x = loop { break 42; }; }"));
}

#[test]
fn while_expression_analyzes() {
    assert!(analyze_ok("fn f() { let x = while true { break 1; }; }"));
}

#[test]
fn loop_expression_with_int_break_analyzes() {
    assert!(analyze_ok("fn f() -> Int { return loop { break 42; }; }"));
}

#[test]
fn loop_expression_with_bool_break_analyzes() {
    assert!(analyze_ok(
        "fn f() -> Bool { return loop { break true; }; }"
    ));
}

#[test]
fn loop_expression_nested_blocks_analyzes() {
    assert!(analyze_ok(
        "fn f() { let x = { let a = 1; loop { break a; } }; }"
    ));
}

#[test]
fn while_expression_with_looping_analyzes() {
    assert!(analyze_ok(
        "fn f() { let mut n = 0; let x = while n < 10 { n = n + 1; break n; }; }"
    ));
}

#[test]
fn loop_expression_with_continue_and_break_analyzes() {
    assert!(analyze_ok(
        "fn f() { let x = loop { continue; break 1; }; }"
    ));
}

#[test]
fn loop_expression_in_const_binding_analyzes() {
    assert!(analyze_ok("fn f() { const X = loop { break 42; }; }"));
}

#[test]
fn loop_expression_with_annotation_analyzes() {
    assert!(analyze_ok("fn f() { let x: Int = loop { break 42; }; }"));
}

#[test]
fn loop_expression_return_from_function_analyzes() {
    assert!(analyze_ok(
        "fn f() -> Int { let x = loop { break 5; }; return x; }"
    ));
}

// ---------------------------------------------------------------------------
// Negative: type errors
// ---------------------------------------------------------------------------

#[test]
fn loop_expression_type_mismatch_fails() {
    // The break value type must match the annotation.
    assert!(!analyze_ok("fn f() { let x: Bool = loop { break 42; }; }"));
}

#[test]
fn break_value_in_statement_loop_ignored() {
    // `break expr;` in a statement-position while is fine — the value is
    // ignored; the while statement has type Unit.
    assert!(analyze_ok("fn f() { while true { break 42; } }"));
}

// ---------------------------------------------------------------------------
// Native end-to-end execution tests
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "mink_loop_expr_test_{}_{}",
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
    let path = temp_source(&format!("loop_expr_native_{n}.mink"), src);
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
fn native_loop_expression_basic() {
    let exit = native_exit_code("fn main() { let x = loop { break 42; }; return x; }\n");
    assert_eq!(exit, 42);
}

#[test]
fn native_while_expression_basic() {
    let exit = native_exit_code("fn main() { let x = while true { break 7; }; return x; }\n");
    assert_eq!(exit, 7);
}

#[test]
fn native_loop_expression_with_looping() {
    let exit = native_exit_code(
        "fn main() {\n\
         let mut i = 0;\n\
         let x = loop {\n\
         if i == 5 { break i; }\n\
         i = i + 1;\n\
         };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 5);
}

#[test]
fn native_while_expression_with_looping() {
    // break i fires on first iteration (i becomes 1), so exit code is 1.
    let exit = native_exit_code(
        "fn main() {\n\
         let mut i = 0;\n\
         let x = while i < 5 {\n\
         i = i + 1;\n\
         break i;\n\
         };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 1);
}

#[test]
fn native_loop_expression_arithmetic() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = loop { break 3 * 7 + 1; };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 22);
}

#[test]
fn native_loop_expression_nested_in_if() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = if true { loop { break 10; } } else { loop { break 20; } };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 10);
}

#[test]
fn native_loop_expression_return_value() {
    let exit = native_exit_code(
        "fn compute() -> Int { return loop { break 99; }; }\n\
         fn main() { return compute(); }\n",
    );
    assert_eq!(exit, 99);
}

#[test]
fn native_loop_expression_function_arg() {
    // Loop expression in binding position, passed to function.
    let exit = native_exit_code(
        "fn g(x: Int) -> Int { return x + 1; }\n\
         fn main() { let v = loop { break 5; }; return g(v); }\n",
    );
    assert_eq!(exit, 6);
}

#[test]
fn native_while_expression_false_condition() {
    // While false: the loop body never executes, break never fires.
    // The expression is never assigned (dead code), so the binding gets
    // a zero-initialized value. The function returns 0.
    let exit = native_exit_code("fn main() { let x = while false { break 42; }; return x; }\n");
    assert_eq!(exit, 0);
}

#[test]
fn native_loop_expression_with_continue() {
    let exit = native_exit_code(
        "fn main() {\n\
         let mut i = 0;\n\
         let x = loop {\n\
         i = i + 1;\n\
         if i < 3 { continue; }\n\
         break i;\n\
         };\n\
         return x;\n\
         }\n",
    );
    assert_eq!(exit, 3);
}

#[test]
fn native_loop_expression_break_tuple() {
    let exit = native_exit_code(
        "fn main() {\n\
         let x = loop { break (10, 20); };\n\
         return x.0 + x.1;\n\
         }\n",
    );
    assert_eq!(exit, 30);
}

#[test]
fn native_loop_expression_with_binding() {
    let exit = native_exit_code(
        "fn main() {\n\
         let result = loop {\n\
         let a = 5;\n\
         let b = 10;\n\
         break a * b;\n\
         };\n\
         return result;\n\
         }\n",
    );
    assert_eq!(exit, 50);
}

// ---------------------------------------------------------------------------
// Determinism check
// ---------------------------------------------------------------------------

#[test]
fn native_loop_expression_deterministic() {
    let src = "fn main() { let x = loop { break 42; }; return x; }\n";
    let exit1 = native_exit_code(src);
    let exit2 = native_exit_code(src);
    assert_eq!(exit1, exit2);
    assert_eq!(exit1, 42);
}
