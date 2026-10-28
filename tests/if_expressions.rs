//! Tests for `if` statements versus if-expressions in expression blocks.
//!
//! Session 140 (M3 P1) closes the M2 A5 defect: inside a closure body (an
//! expression block) an `if` was always parsed through the if-*expression*
//! grammar, so a statement `if` with no `else` — and an `else if` chain with
//! no final `else` — was rejected with `E-P26` even though the identical body
//! compiles in a function. The parser now tries the expression form and falls
//! back to the statement form.
//!
//! These tests pin the whole compiler pipeline (build + execute), because the
//! defect surfaced as a *parse* error on a program the rest of the compiler
//! already handled correctly.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_ifexpr_{n}_{name}"));
    std::fs::write(&path, content).unwrap();
    path
}

/// Build and run a MINK source file, returning the process exit code.
fn native_exit_code(src: &str) -> i32 {
    let path = temp_source("e2e.mink", src);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.code() == Some(0),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    let run = Command::new(&exe).status().unwrap();
    let code = run.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    code
}

/// Build and run a MINK source file, returning its stdout as text.
fn native_stdout(src: &str) -> String {
    let path = temp_source("out.mink", src);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.code() == Some(0),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = path.with_extension("exe");
    let run = Command::new(&exe).output().unwrap();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    String::from_utf8_lossy(&run.stdout).to_string()
}

/// Assert exit code.
fn assert_exit_code(src: &str, expected: i32) {
    let actual = native_exit_code(src);
    assert_eq!(actual, expected, "expected exit {expected}, got {actual}");
}

/// Assert `check` succeeds (no diagnostics, no crash).
fn assert_check_ok(src: &str) {
    let path = temp_source("chk.mink", src);
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.code() == Some(0),
        "expected success, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// =========================================================================
// M2 A5 regression — the defect itself
// =========================================================================

/// The exact A5 shape: a statement `if` with no `else` as a non-final
/// statement of a closure body. Rejected with `E-P26` before the fix.
#[test]
fn statement_if_without_else_in_a_closure_block_builds_and_runs() {
    assert_exit_code(
        "fn main() -> Int { \
           let f = |x: Int| { \
             if x > 0 { let y = 1; } \
             return 0; \
           }; \
           return f(1); \
         }",
        0,
    );
}

/// The statement `if` actually runs: its then-block executes for a positive
/// argument and is skipped for a non-positive one.
#[test]
fn statement_if_without_else_in_a_closure_block_executes_its_branch() {
    let out = native_stdout(
        "fn main() { \
           let f = |x: Int| { \
             let mut hits = 0; \
             if x > 0 { hits = 1; } \
             return hits; \
           }; \
           rt_print_int(f(5) * 10 + f(-5)); \
         }",
    );
    assert_eq!(out.trim(), "10", "stdout was {out:?}");
}

/// `check` and `build` agree on the A5 shape.
#[test]
fn check_and_build_agree_on_a_statement_if_in_a_closure_block() {
    let src = "fn main() -> Int { \
                 let f = |x: Int| { \
                   if x > 0 { let y = 1; } \
                   return 0; \
                 }; \
                 return f(1); \
               }";
    assert_check_ok(src);
    assert_exit_code(src, 0);
}

/// An `else if` chain with no final `else` is a statement chain, and the
/// taken arm runs.
#[test]
fn else_if_chain_without_final_else_in_a_closure_block_builds_and_runs() {
    let out = native_stdout(
        "fn main() { \
           let g = |x: Int| { \
             let mut r = 0; \
             if x == 1 { r = 10; } else if x == 2 { r = 20; } \
             return r; \
           }; \
           rt_print_int(g(1) * 10000 + g(2) * 100 + g(3)); \
         }",
    );
    // 10, 20 and 0 in one integer: each arm of the chain is reached.
    assert_eq!(out.trim(), "102000", "stdout was {out:?}");
}

/// The same shapes must work in any expression block, not only a closure
/// body: here a plain block expression used as a `let` initializer.
#[test]
fn statement_if_without_else_in_a_block_expression_builds_and_runs() {
    assert_exit_code(
        "fn main() -> Int { \
           let r = { let a = 1; if a > 0 { let b = 2; } 7 }; \
           return r; \
         }",
        7,
    );
}

// =========================================================================
// Behaviour that must NOT change — an `if` that produces a value
// =========================================================================

/// A trailing `if`/`else` in a closure body still *is* an if-expression and
/// yields the taken arm's value.
#[test]
fn trailing_if_expression_in_a_closure_block_returns_its_value() {
    assert_exit_code(
        "fn main() -> Int { let f = |x: Int| { if x > 0 { 42 } else { 0 } }; return f(1); }",
        42,
    );
    assert_exit_code(
        "fn main() -> Int { let f = |x: Int| { if x > 0 { 42 } else { 0 } }; return f(-1); }",
        0,
    );
}

/// A trailing if-expression nested through `else if` still returns the taken
/// arm (the Session 138 nested-`else if` fix stays closed).
#[test]
fn nested_else_if_expression_in_a_closure_block_returns_the_taken_arm() {
    assert_exit_code(
        "fn main() -> Int { \
           let f = |x: Int| { if x == 1 { 10 } else if x == 2 { 20 } else { 30 } }; \
           return f(2); \
         }",
        20,
    );
}

/// The `else` requirement for an if-expression in value position is
/// unchanged: `let x = if .. { .. };` is a parse error, not a statement.
#[test]
fn if_expression_without_else_in_a_value_position_is_rejected() {
    let path = temp_source(
        "no_else.mink",
        "fn main() { let c = true; let x = if c { 1 }; return x; }",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.code() != Some(0),
        "expected a diagnostic, got success"
    );
    assert!(
        stderr.contains("E-P26"),
        "expected E-P26 for a valueless if-expression, stderr:\n{stderr}"
    );
}

/// A trailing `if` with no `else` at the very end of a closure body has no
/// value to produce: it must stay a diagnosed parse error, never a compiler
/// panic and never silent acceptance as `Unit`.
#[test]
fn trailing_if_without_else_in_a_closure_block_is_a_diagnosed_error() {
    let path = temp_source(
        "trailing_no_else.mink",
        "fn main() -> Int { let f = |x: Int| { if x > 0 { 1 } }; return f(1); }",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.code() != Some(0),
        "expected a diagnostic, got success"
    );
    assert!(
        stderr.contains("error["),
        "expected a structured diagnostic, stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("internal error"),
        "compiler must not panic, stderr:\n{stderr}"
    );
}
