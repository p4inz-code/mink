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

// =========================================================================
// Capture semantics regressions
// =========================================================================

/// Captures are the generated function's LEADING parameters, in the sorted
/// order the call-site rewrite passes them. They used to be prepended one at
/// a time, which reversed the list and silently swapped the captured values.
/// Addition hid it; subtraction does not.
#[test]
fn closure_native_multi_capture_order_is_preserved() {
    assert_exit_code(
        "fn main() { let a = 50; let b = 8; let f = |x: Int| a - b + x; return f(0); }",
        42,
    );
}

/// The same with three captures, so a reversal of any pair is visible.
#[test]
fn closure_native_three_capture_order_is_preserved() {
    assert_exit_code(
        "fn main() { let a = 50; let b = 10; let c = 1; let f = |x: Int| a - b - c + x; return f(3); }",
        42,
    );
}

/// A call's callee that names a runtime intrinsic is a function, not a
/// captured value: capturing its name shadowed the intrinsic inside the
/// generated function (and passing an intrinsic as a capture is `E-B07`).
#[test]
fn closure_body_calls_an_intrinsic_by_name() {
    assert_exit_code(
        "fn main() { let f = |x: Int| rt_str_len(\"hello\") + x; return f(37); }",
        42,
    );
}

/// A source file that begins with a blank line (LF endings) used to break
/// every capturing closure: the synthetic span minted for a captured
/// parameter landed on `main`'s name and replaced its type, so lowering
/// failed with `E-H03 cannot lower function: not a function type`.
#[test]
fn closure_native_capture_with_leading_blank_line() {
    assert_exit_code(
        "\nfn main() { let y = 10; let f = |x: Int| x + y; return f(32); }",
        42,
    );
}

/// Three leading blank lines used to break capturing closures the same way
/// as one; the failure depended on the resulting offsets.
#[test]
fn closure_native_capture_with_three_leading_blank_lines() {
    assert_exit_code(
        "\n\n\nfn main() { let y = 10; let f = |x: Int| x + y; return f(32); }",
        42,
    );
}

// =========================================================================
// Nested closures
// =========================================================================

/// A closure defined inside another closure's body must be desugared and
/// callable like any other closure. It used to pass `check` and fail the
/// backend (`E-B06`), so `check` and `build` disagreed about a valid
/// program.
#[test]
fn closure_native_nested_definition() {
    assert_exit_code(
        "fn main() { let f = |x: Int| { let g = |y: Int| x + y; g(5) }; let r = f(10); return r; }",
        15,
    );
}

/// The nested closure captures its enclosing closure's parameter, which is
/// a parameter of the generated outer function.
#[test]
fn closure_native_nested_captures_outer_param() {
    assert_exit_code(
        "fn main() -> Int { let f = |x: Int| { let g = |y: Int| x * y; g(4) }; return f(10); }",
        40,
    );
}

/// `check` and `build` must agree: a nested-closure program accepted by the
/// checker must also build and run. They used to disagree (check passed,
/// build failed `E-B06`).
#[test]
fn closure_nested_check_and_build_agree() {
    let src = "fn main() { let f = |x: Int| { let g = |y: Int| x + y; g(5) }; let r = f(10); return r; }";
    assert_check_ok(src);
    assert_exit_code(src, 15);
}

/// A closure body may call a capturing closure declared in the enclosing
/// function. The captured values must be threaded through the generated
/// function; they used to be missing, so the call failed as `E-T05` even
/// though the program is valid.
#[test]
fn closure_native_body_calls_capturing_closure() {
    assert_exit_code(
        "fn main() -> Int { let base = 10; let add_base = |x: Int| x + base; let twice = |y: Int| add_base(y) + add_base(y); let r = twice(5); if r == 30 { return 42; } return 1; }",
        42,
    );
}

/// A zero-capture closure called from inside another closure body stays a
/// function value and is called indirectly, as before.
#[test]
fn closure_native_body_calls_zero_capture_closure() {
    assert_exit_code(
        "fn main() -> Int { let inc = |x: Int| x + 1; let twice = |y: Int| inc(inc(y)); return twice(40); }",
        42,
    );
}

/// Three levels of enclosing closures: each level's captures must reach the
/// call site of the next level.
#[test]
fn closure_native_three_levels() {
    assert_exit_code(
        "fn main() -> Int { let base = 1; let a = |x: Int| x + base; let b = |y: Int| a(y) + 1; let c = |z: Int| b(z) + 1; return c(39); }",
        42,
    );
}

// =========================================================================
// Statement-block closure bodies (M2 P1 regression)
//
// A closure whose body is a statement block with **no trailing expression** —
// the common `|x| { ...; return v; }` shape — used to crash the compiler with
// an internal error (`literal_operand is only called for literals`) in MIR
// lowering. The desugared closure function body is `return <closure body>`
// (see `desugar_closure_to_fn`), so a body block with no trailing expression
// was placed in operand position and lowered as if it were a literal.
// =========================================================================

/// A statement-block body ending in an explicit `return` compiles and
/// returns the right value (would panic before the fix).
#[test]
fn closure_native_statement_block_body_ending_in_return() {
    assert_exit_code(
        "fn main() -> Int { let g = |y: Int| { return y + 1; }; return g(7); }",
        8,
    );
}

/// `check` must also not panic on the same shape (it used to panic too), and
/// `check`/`build` must agree.
#[test]
fn closure_statement_block_body_checks_and_builds() {
    let src = "fn main() -> Int { let g = |y: Int| { return y + 1; }; return g(7); }";
    assert_check_ok(src);
    assert_exit_code(src, 8);
}

/// A statement-block body that returns a value computed from a captured
/// variable.
#[test]
fn closure_native_statement_block_body_captures_and_returns() {
    assert_exit_code(
        "fn main() -> Int { let base = 30; let g = |y: Int| { let t = y + base; return t; }; return g(12); }",
        42,
    );
}

/// A closure nested inside another closure's statement-block body (no
/// trailing expression) compiles and runs.
#[test]
fn closure_native_statement_block_body_with_nested_closure() {
    assert_exit_code(
        "fn main() -> Int { let g = |y: Int| { let h = |z: Int| z + y; return h(10); }; return g(7) + 25; }",
        42,
    );
}

/// Boundary: a closure body that is a statement block **with** a trailing
/// expression (no explicit `return`) still returns that value.
#[test]
fn closure_native_statement_block_body_with_trailing_expression() {
    assert_exit_code(
        "fn main() -> Int { let g = |y: Int| { let h = |z: Int| z + 1; h(10) + y - 7 }; return g(7); }",
        11,
    );
}

/// Boundary: a closure whose statement-block body returns nothing is a unit
/// closure and must build and run (exit 0), not crash the compiler.
#[test]
fn closure_native_unit_statement_block_body() {
    assert_exit_code(
        "fn main() -> Int { let g = |y: Int| { let t = y + 1; }; g(1); return 0; }",
        0,
    );
}

// =========================================================================
// Session 138: A4 — if-expressions in operand position
// =========================================================================

/// An if-expression used as a `return` value is valid MINK and must compile
/// and produce the taken branch's value. It previously panicked in MIR
/// lowering (`literal_operand is only called for literals`).
#[test]
fn if_expression_in_return_position() {
    assert_check_ok(
        "fn f(x: Int) -> Int { return if x > 0 { 11 } else { 22 }; } fn main() -> Int { return f(-1); }",
    );
    assert_exit_code(
        "fn f(x: Int) -> Int { return if x > 0 { 11 } else { 22 }; }
fn main() -> Int { return f(-1); }",
        22,
    );
}

/// An if-expression used as a call argument must be evaluated before the call.
#[test]
fn if_expression_as_call_argument() {
    assert_exit_code(
        "fn pick(x: Int) -> Int { return x * 2; }
fn main() -> Int { return pick(if false { 1 } else { 21 }); }",
        42,
    );
}

/// An if-expression used as a binary operand must produce its value; the
/// surrounding expression is evaluated after the branch merge.
#[test]
fn if_expression_as_binary_operand() {
    assert_exit_code(
        "fn main() -> Int { return 1 + if false { 2 } else { 3 }; }",
        4,
    );
}

/// A nested `else if` chain in operand position must take the correct arm.
#[test]
fn nested_if_expression_in_operand_position() {
    assert_exit_code(
        "fn main() -> Int { return if false { 1 } else if false { 2 } else { 9 }; }",
        9,
    );
}

/// A nested `else if` chain in a binding must assign the taken arm's value.
/// It previously lowered as a statement `if`, never assigning the result
/// local, so the binding silently kept its zero initializer.
#[test]
fn nested_else_if_expression_in_binding_returns_the_taken_arm() {
    assert_exit_code(
        "fn main() -> Int { let x = if false { 1 } else if true { 2 } else { 9 }; return x; }",
        2,
    );
    assert_exit_code(
        "fn main() -> Int { let x = if false { 1 } else if false { 2 } else { 9 }; return x; }",
        9,
    );
}

/// A block whose trailing expression is an if-expression is itself an
/// operand; its value must reach the surrounding expression.
#[test]
fn if_expression_as_block_trailing_operand() {
    assert_exit_code(
        "fn main() -> Int { return { if false { 1 } else { 6 } }; }",
        6,
    );
}

/// An if-expression on the right-hand side of an assignment must store the
/// taken branch's value (the assignment itself is an operand).
#[test]
fn if_expression_as_assignment_value() {
    assert_exit_code(
        "fn main() -> Int { let mut x = 0; x = if true { 5 } else { 6 }; return x; }",
        5,
    );
}

/// An if-expression inside an array literal must be evaluated element by
/// element, not replaced by a placeholder operand.
#[test]
fn if_expression_in_array_literal() {
    assert_exit_code(
        "fn main() -> Int { let a = [if true { 1 } else { 2 }, 3]; return a[0]; }",
        1,
    );
}

// =========================================================================
// Session 141: an if-expression operand in a closure's trailing expression
// =========================================================================

/// A closure whose trailing expression is a binary expression with an
/// if-expression operand must compile and return the computed value. The
/// block operand used to fall through to `StmtEval` and panic in MIR
/// lowering (`literal_operand is only called for literals`).
#[test]
fn closure_trailing_binary_with_if_expression_operand() {
    let src = "fn main() -> Int { let c = true; let f = |y: Int| { if c { y } else { 0 } + 5 }; return f(37); }";
    assert_check_ok(src);
    assert_exit_code(src, 42);
}

/// The mirrored operand order (`5 + if ...`) with the other branch taken.
#[test]
fn closure_trailing_binary_with_if_expression_on_the_right() {
    assert_exit_code(
        "fn main() -> Int { let c = false; let f = |y: Int| { 5 + if c { y } else { 2 } }; return f(37); }",
        7,
    );
}

/// A parenthesized if-expression operand with a multiplication.
#[test]
fn closure_trailing_parenthesized_if_expression_operand() {
    assert_exit_code(
        "fn main() -> Int { let c = false; let f = |y: Int| { (if c { y } else { 21 }) * 2 }; return f(0); }",
        42,
    );
}

/// A nested if-expression inside the operand.
#[test]
fn closure_trailing_binary_with_nested_if_expression_operand() {
    assert_exit_code(
        "fn main() -> Int { let c = true; let d = true; let f = |y: Int| { (if c { if d { y } else { 1 } } else { 0 }) + 5 }; return f(37); }",
        42,
    );
}

/// Branches that read locals rather than only literals.
#[test]
fn closure_trailing_if_expression_operand_reads_locals() {
    assert_exit_code(
        "fn main() -> Int { let c = true; let f = |y: Int| { let a = y; let b = 0; (if c { a } else { b }) + 5 }; return f(37); }",
        42,
    );
}

/// A block expression nested inside the trailing expression.
#[test]
fn closure_trailing_binary_with_block_wrapped_if_expression() {
    assert_exit_code(
        "fn main() -> Int { let c = true; let f = |y: Int| { { if c { y } else { 0 } } + 5 }; return f(37); }",
        42,
    );
}

/// The same shape outside a closure: a block in a binding initializer and a
/// block in `return` position.
#[test]
fn block_trailing_binary_with_if_expression_operand_outside_a_closure() {
    assert_exit_code(
        "fn main() -> Int { let c = false; let x = { if c { 1 } else { 2 } + 5 }; return x; }",
        7,
    );
    assert_exit_code(
        "fn main() -> Int { let c = true; return { if c { 37 } else { 0 } + 5 }; }",
        42,
    );
}

/// An ordinary trailing expression and a statement-prefixed trailing
/// expression keep working (the fix only redirects the trailing expression).
#[test]
fn ordinary_and_statement_prefixed_closure_trailing_expressions() {
    assert_exit_code(
        "fn main() -> Int { let g = |y: Int| { y + 5 }; return g(37); }",
        42,
    );
    assert_exit_code(
        "fn main() -> Int { let f = |y: Int| { let t = y; t + 5 }; return f(37); }",
        42,
    );
}
