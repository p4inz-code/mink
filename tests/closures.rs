//! Tests for closures/lambdas — Session 37.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_clos_{n}_{name}"));
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

/// Assert exit code.
fn assert_exit_code(src: &str, expected: i32) {
    let actual = native_exit_code(src);
    assert_eq!(actual, expected, "expected exit {expected}, got {actual}");
}

/// Assert check succeeds.
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

/// Assert check fails with given error code.
fn assert_check_err(src: &str, error_code: &str) {
    let path = temp_source("chk.mink", src);
    let output = mink().arg("check").arg(&path).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.code() != Some(0),
        "expected error but succeeded"
    );
    assert!(
        stderr.contains(error_code),
        "expected `{error_code}` in:\n{stderr}"
    );
}

// =========================================================================
// Parser / basic syntax (check only)
// =========================================================================

#[test]
fn closure_syntax_zero_capture() {
    assert_check_ok("fn main() { let f = |x: Int| x; let r = f(42); return r; }");
}

#[test]
fn closure_syntax_block_body() {
    assert_check_ok("fn main() { let f = |x: Int| { x + 1 }; let r = f(10); return r; }");
}

#[test]
fn closure_syntax_multiple_params() {
    assert_check_ok("fn main() { let f = |a: Int, b: Int| a + b; let r = f(3, 4); return r; }");
}

#[test]
fn closure_syntax_no_params() {
    assert_check_ok("fn main() { let f = | | 42; let r = f(); return r; }");
}

#[test]
fn closure_syntax_nested() {
    assert_check_ok(
        "fn main() { let f = |x: Int| { let g = |y: Int| x + y; g(5) }; let r = f(10); return r; }",
    );
}

// =========================================================================
// Zero-capture closures — end-to-end
// =========================================================================

#[test]
fn closure_native_identity() {
    assert_exit_code("fn main() { let f = |x: Int| x; return f(42); }", 42);
}

#[test]
fn closure_native_add_one() {
    assert_exit_code("fn main() { let f = |x: Int| x + 1; return f(41); }", 42);
}

#[test]
fn closure_native_multi_params() {
    assert_exit_code(
        "fn main() { let f = |a: Int, b: Int| a + b; return f(20, 22); }",
        42,
    );
}

#[test]
fn closure_native_block_body() {
    assert_exit_code(
        "fn main() { let f = |x: Int| { x * 2 + 1 }; return f(20); }",
        41,
    );
}

// =========================================================================
// Closures with captures — end-to-end
// =========================================================================

#[test]
fn closure_native_single_capture() {
    assert_exit_code(
        "fn main() { let y = 10; let f = |x: Int| x + y; return f(32); }",
        42,
    );
}

#[test]
fn closure_native_capture_multiply() {
    assert_exit_code(
        "fn main() { let m = 3; let f = |x: Int| x * m; return f(14); }",
        42,
    );
}

#[test]
fn closure_native_multi_captures() {
    assert_exit_code(
        "fn main() { let a = 10; let b = 20; let f = |x: Int| x + a + b; return f(12); }",
        42,
    );
}

#[test]
fn closure_native_capture_block() {
    assert_exit_code(
        "fn main() { let base = 100; let f = |x: Int| { base - x }; return f(58); }",
        42,
    );
}

// =========================================================================
// Closures passed to functions
// =========================================================================

#[test]
fn closure_native_applied_zero_capture() {
    assert_exit_code(
        "fn apply(f, x) { return f(x); } fn main() { return apply(|x: Int| x + 1, 41); }",
        42,
    );
}

#[test]
fn closure_native_applied_as_var_zero_capture() {
    assert_exit_code(
        "fn apply(f, x) { return f(x); } fn main() { let inc = |x: Int| x + 1; return apply(inc, 41); }",
        42,
    );
}

// =========================================================================
// Closures in control flow
// =========================================================================

#[test]
fn closure_native_in_if() {
    assert_exit_code(
        "fn main() { let flag = 1; let f = |x: Int| x * 2; if flag == 1 { return f(21); } else { return 0; } }",
        42,
    );
}

#[test]
fn closure_native_ternary() {
    assert_exit_code(
        "fn main() { let f = |x: Int| x; let r = if 1 == 1 { f(42) } else { 0 }; return r; }",
        42,
    );
}

// =========================================================================
// Closures interacting with features
// =========================================================================

#[test]
fn closure_native_in_block() {
    assert_exit_code(
        "fn main() { let f = |x: Int| x; let r = { f(42) }; return r; }",
        42,
    );
}

#[test]
fn closure_native_with_struct() {
    assert_exit_code(
        "struct P { x: Int, y: Int } fn main() { let p = P { x: 10, y: 32 }; let f = |v: Int| v; return f(42); }",
        42,
    );
}

#[test]
fn closure_native_with_enum() {
    assert_exit_code(
        "enum O { Some(Int), None } fn main() { let v = O::Some(42); let f = |x: Int| x; match v { O::Some(n) => { return f(n); }, O::None => { return 0; } } }",
        42,
    );
}

// =========================================================================
// Negative tests
// =========================================================================

#[test]
fn closure_wrong_arg_count_rejected() {
    assert_check_err(
        "fn main() { let f = |x: Int| x; let r = f(1, 2); return r; }",
        "E-T05",
    );
}

#[test]
fn closure_type_mismatch_rejected() {
    assert_check_err(
        "fn main() { let f = |x: Int| x; let r = f(true); return r; }",
        "E-T01",
    );
}

// =========================================================================
// Regression
// =========================================================================

#[test]
fn regression_basic_fn() {
    assert_exit_code("fn main() { return 42; }", 42);
}

#[test]
fn regression_if_expr() {
    assert_exit_code(
        "fn main() { let r = if 1 == 1 { 42 } else { 0 }; return r; }",
        42,
    );
}

#[test]
fn regression_struct_access() {
    assert_exit_code(
        "struct P { x: Int } fn main() { let p = P { x: 42 }; return p.x; }",
        42,
    );
}

#[test]
fn regression_generics() {
    assert_exit_code(
        "fn id<T>(x: T) -> T { return x; } fn main() { return id(42); }",
        42,
    );
}
