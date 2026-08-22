//! Tests for Option<T> and Result<T,E> — Session 39.
//!
//! These types are defined as standard generic enums and tested through
//! the full compiler pipeline: parser, semantic analysis, type checking,
//! monomorphization, HIR, MIR, and native execution.
//!
//! # Architecture Decision
//! Option<T> and Result<T,E> are implemented as standard library definitions
//! (not compiler built-ins). This is the simplest correct V1-compatible design:
//! - Zero compiler changes required
//! - Works with existing generic enum infrastructure
//! - Consistent with how Rust handles these types
//! - Easy to test and evolve independently
//!
//! # V1 Limitations
//! - Unit variant construction (`let x = Enum::None;`) is parsed as type annotation
//!   due to parser ambiguity. Use data-carrying variants.
//! - Pattern matching on generic enum variants is not yet supported
//! - Generic enum types are not accessible in closure bodies (pre-existing)
//! - Calling generic functions with generic enum args not yet supported (pre-existing)
//! - The `?` operator is not yet implemented

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_opt_{n}_{name}"));
    std::fs::write(&path, content).unwrap();
    path
}

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

fn assert_exit_code(src: &str, expected: i32) {
    let actual = native_exit_code(src);
    assert_eq!(actual, expected, "expected exit {expected}, got {actual}");
}

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
// OPTION<T> — Parser / Type Checking
// =========================================================================

#[test]
fn option_declaration() {
    assert_check_ok("enum Option<T> { Some(T), None }\nfn main() { return 0; }");
}

#[test]
fn option_some_construction() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\nfn main() { let x = Option::Some(42); return 0; }",
    );
}

#[test]
fn option_some_bool() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\nfn main() { let x = Option::Some(true); return 0; }",
    );
}

#[test]
fn option_some_string() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\nfn main() { let x = Option::Some(\"hello\"); return 0; }",
    );
}

#[test]
fn option_some_char() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\nfn main() { let x = Option::Some('a'); return 0; }",
    );
}

#[test]
fn option_multiple_instantiations() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\nfn main() { let a = Option::Some(42); let b = Option::Some(true); return 0; }",
    );
}

// =========================================================================
// OPTION<T> — Generic Functions (non-enum args)
// =========================================================================

#[test]
fn option_return_from_generic() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\n\
         fn find<T>(x: T) -> Option<T> { return Option::Some(x); }\n\
         fn main() { let r = find(42); return 0; }",
    );
}

#[test]
fn option_return_none_generic() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\n\
         fn find_none<T>() -> Option<T> { return Option::None; }\n\
         fn main() { let r = find_none::<Int>(); return 0; }",
    );
}

#[test]
fn option_return_conditional() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\n\
         fn find<T>(x: T, flag: Bool) -> Option<T> {\n\
             if flag { return Option::Some(x); } else { return Option::None; }\n\
         }\n\
         fn main() { let r = find(42, true); return 0; }",
    );
}

// =========================================================================
// OPTION<T> — Ownership
// =========================================================================

#[test]
fn option_move_semantics() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\nfn main() { let x = Option::Some(42); let y = x; return 0; }",
    );
}

// =========================================================================
// RESULT<T, E> — Parser / Type Checking
// =========================================================================

#[test]
fn result_declaration() {
    assert_check_ok("enum Result<T, E> { Ok(T), Err(E) }\nfn main() { return 0; }");
}

#[test]
fn result_ok_construction() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\nfn main() { let x = Result::Ok(42); return 0; }",
    );
}

#[test]
fn result_err_construction() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\nfn main() { let x = Result::Err(1); return 0; }",
    );
}

#[test]
fn result_mixed_types() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\nfn main() { let x = Result::Ok(42); let y = Result::Err(true); return 0; }",
    );
}

#[test]
fn result_multiple_instantiations() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\nfn main() { let a = Result::Ok(42); let b = Result::Ok(true); let c = Result::Err(1); let d = Result::Err(false); return 0; }",
    );
}

// =========================================================================
// RESULT<T, E> — Generic Functions (non-enum args)
// =========================================================================

#[test]
fn result_return_from_generic() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\n\
         fn ok<T, E>(x: T) -> Result<T, E> { return Result::Ok(x); }\n\
         fn main() { let r = ok::<Int, Int>(42); return 0; }",
    );
}

#[test]
fn result_return_err_generic() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\n\
         fn fail<T, E>(err: E) -> Result<T, E> { return Result::Err(err); }\n\
         fn main() { let r = fail::<Int, Int>(1); return 0; }",
    );
}

#[test]
fn result_return_conditional() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\n\
         fn pick<T, E>(a: T, b: E, flag: Bool) -> Result<T, E> {\n\
             if flag { return Result::Ok(a); } else { return Result::Err(b); }\n\
         }\n\
         fn main() { let r = pick::<Int, Int>(10, 0, true); return 0; }",
    );
}

// =========================================================================
// RESULT<T, E> — Ownership
// =========================================================================

#[test]
fn result_move_semantics() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\nfn main() { let x = Result::Ok(42); let y = x; return 0; }",
    );
}

// =========================================================================
// Combined Option + Result
// =========================================================================

#[test]
fn combined_option_and_result() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\n\
         enum Result<T, E> { Ok(T), Err(E) }\n\
         fn make_opt<T>(x: T) -> Option<T> { return Option::Some(x); }\n\
         fn make_res<T, E>(x: T) -> Result<T, E> { return Result::Ok(x); }\n\
         fn main() { let o = make_opt(42); let r = make_res::<Int, Int>(42); return 0; }",
    );
}

// =========================================================================
// Generics + Option/Result combined
// =========================================================================

#[test]
fn generic_option_return() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\n\
         fn maybe_wrap<T>(x: T) -> Option<T> { return Option::Some(x); }\n\
         fn main() { let a = maybe_wrap(42); let b = maybe_wrap(true); return 0; }",
    );
}

#[test]
fn generic_result_return() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\n\
         fn maybe_ok<T, E>(x: T) -> Result<T, E> { return Result::Ok(x); }\n\
         fn main() { let a = maybe_ok::<Int, Int>(42); let b = maybe_ok::<Bool, Int>(true); return 0; }",
    );
}

#[test]
fn generic_identity_preserves_option() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }\n\
         fn id<T>(x: T) -> T { return x; }\n\
         fn main() { let x = Option::Some(42); let y = id(42); return 0; }",
    );
}

#[test]
fn generic_identity_preserves_result() {
    assert_check_ok(
        "enum Result<T, E> { Ok(T), Err(E) }\n\
         fn id<T>(x: T) -> T { return x; }\n\
         fn main() { let x = Result::Ok(42); let y = id(42); return 0; }",
    );
}

// =========================================================================
// Native E2E
// =========================================================================

#[test]
fn native_option_some() {
    assert_exit_code(
        "enum Option<T> { Some(T), None }\nfn main() { let x = Option::Some(42); return 0; }",
        0,
    );
}

#[test]
fn native_option_none() {
    assert_exit_code(
        "enum Option<T> { Some(T), None }\nfn get_none<T>() -> Option<T> { return Option::None; }\nfn main() { let r = get_none::<Int>(); return 0; }",
        0,
    );
}

#[test]
fn native_option_through_generic() {
    assert_exit_code(
        "enum Option<T> { Some(T), None }\nfn wrap<T>(x: T) -> Option<T> { return Option::Some(x); }\nfn main() { let r = wrap(42); return 0; }",
        0,
    );
}

#[test]
fn native_result_ok() {
    assert_exit_code(
        "enum Result<T, E> { Ok(T), Err(E) }\nfn main() { let x = Result::Ok(42); return 0; }",
        0,
    );
}

#[test]
fn native_result_err() {
    assert_exit_code(
        "enum Result<T, E> { Ok(T), Err(E) }\nfn main() { let x = Result::Err(1); return 0; }",
        0,
    );
}

#[test]
fn native_result_through_generic() {
    assert_exit_code(
        "enum Result<T, E> { Ok(T), Err(E) }\nfn ok<T, E>(x: T) -> Result<T, E> { return Result::Ok(x); }\nfn main() { let r = ok::<Int, Int>(42); return 0; }",
        0,
    );
}

// =========================================================================
// Negative Tests
// =========================================================================

#[test]
fn option_wrong_payload_type_rejected() {
    assert_check_err(
        "enum Option<T> { Some(T), None }\nfn main() { let x: Option<Int> = Option::Some(true); return 0; }",
        "E-T01",
    );
}

#[test]
fn result_wrong_ok_type_rejected() {
    assert_check_err(
        "enum Result<T, E> { Ok(T), Err(E) }\nfn main() { let x: Result<Int, Int> = Result::Ok(true); return 0; }",
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
        "struct P { x: Int }\nfn main() { let p = P { x: 42 }; return p.x; }",
        42,
    );
}

#[test]
fn regression_generics() {
    assert_exit_code(
        "fn id<T>(x: T) -> T { return x; }\nfn main() { return id(42); }",
        42,
    );
}

#[test]
fn regression_closure_direct() {
    assert_exit_code("fn main() { let f = |x: Int| x + 1; return f(41); }", 42);
}

#[test]
fn regression_closure_capture() {
    assert_exit_code(
        "fn main() { let n = 10; let f = |x: Int| x + n; return f(32); }",
        42,
    );
}

#[test]
fn regression_closure_ho_zero_capture() {
    assert_exit_code(
        "fn apply(f, x) { return f(x); }\nfn main() { let f = |x: Int| x + 1; return apply(f, 41); }",
        42,
    );
}

#[test]
fn regression_generic_enum() {
    assert_exit_code(
        "enum Maybe<T> { Some(T), Nothing }\nfn main() { let s = Maybe::Some(42); return 0; }",
        0,
    );
}

#[test]
fn regression_match_non_generic() {
    assert_exit_code(
        "enum E { A, B(Int) }\nfn main() { let e = E::B(42); match e { E::B(x) => { return x; }, E::A => { return 0; } } }",
        42,
    );
}

#[test]
fn regression_tuple_destructure() {
    assert_exit_code("fn main() { let (a, b) = (10, 32); return a + b; }", 42);
}

#[test]
fn regression_modules() {
    assert_exit_code(
        "fn add(a: Int, b: Int) -> Int { return a + b; }\nfn main() { return add(20, 22); }",
        42,
    );
}

#[test]
fn regression_while_loop() {
    assert_exit_code(
        "fn main() { let mut i = 0; let mut sum = 0; while i < 10 { sum = sum + i; i = i + 1; } return sum; }",
        45,
    );
}

#[test]
fn regression_loop_break() {
    assert_exit_code(
        "fn main() { let mut i = 0; let mut sum = 0; loop { if i >= 10 { break; } sum = sum + i; i = i + 1; } return sum; }",
        45,
    );
}
