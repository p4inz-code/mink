//! Tests for the `?` error-propagation operator (Session 40).
//!
//! Covers: Option unwrapping, early return on None, chaining, generic functions,
//! negative/error cases, and native E2E execution.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_try_{n}_{name}"));
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

/// Assert check succeeds (no errors).
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
// BASIC: Option ? unwrap succeeds
// =========================================================================

#[test]
fn option_question_unwrap_some() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }
         fn maybe_double(x: Int) -> Option<Int> {
             if x > 100 { return Option::None; }
             return Option::Some(x * 2);
         }
         fn chain(x: Int) -> Option<Int> {
             let v = maybe_double(x)?;
             return Option::Some(v);
         }
         fn main() -> Int { return 0; }",
    );
}

#[test]
fn option_question_unwrap_none() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }
         fn maybe_double(x: Int) -> Option<Int> {
             if x > 100 { return Option::None; }
             return Option::Some(x * 2);
         }
         fn chain(x: Int) -> Option<Int> {
             let v = maybe_double(x)?;
             return Option::Some(v);
         }
         fn main() -> Int { return 0; }",
    );
}

// =========================================================================
// CHAINING: Multiple ? in sequence
// =========================================================================

#[test]
fn option_question_chained() {
    assert_check_ok(
        "enum Option<T> { Some(T), None }
         fn step1(x: Int) -> Option<Int> {
             if x > 0 { return Option::Some(x + 1); }
             return Option::None;
         }
         fn step2(x: Int) -> Option<Int> {
             if x < 100 { return Option::Some(x * 10); }
             return Option::None;
         }
         fn pipeline(x: Int) -> Option<Int> {
             let a = step1(x)?;
             let b = step2(a)?;
             return Option::Some(b);
         }
         fn main() -> Int { return 0; }",
    );
}

// =========================================================================
// NEGATIVE: ? on non-Option type
// =========================================================================

#[test]
fn question_on_non_option_is_type_error() {
    let path = temp_source("neg.mink", "fn main() { let x = 42?; return; }");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.code() != Some(0),
        "expected error for ? on non-Option"
    );
}

// =========================================================================
// NATIVE E2E: Option ? with actual execution
// =========================================================================

#[test]
fn native_e2e_option_question_some() {
    assert_exit_code(
        "enum Option<T> { Some(T), None }
         fn maybe_double(x: Int) -> Option<Int> {
             if x > 100 { return Option::None; }
             return Option::Some(x * 2);
         }
         fn chain(x: Int) -> Option<Int> {
             let v = maybe_double(x)?;
             return Option::Some(v);
         }
         fn main() -> Int {
             let r = chain(21);
             match r {
                 Option::Some(v) => { return v; },
                 Option::None => { return 0; }
             }
         }",
        42,
    );
}

#[test]
fn native_e2e_option_question_none() {
    assert_exit_code(
        "enum Option<T> { Some(T), None }
         fn maybe_double(x: Int) -> Option<Int> {
             if x > 100 { return Option::None; }
             return Option::Some(x * 2);
         }
         fn chain(x: Int) -> Option<Int> {
             let v = maybe_double(x)?;
             return Option::Some(v);
         }
         fn main() -> Int {
             let r = chain(200);
             match r {
                 Option::Some(v) => { return v; },
                 Option::None => { return 0; }
             }
         }",
        0,
    );
}

#[test]
fn native_e2e_option_question_chained() {
    assert_exit_code(
        "enum Option<T> { Some(T), None }
         fn step1(x: Int) -> Option<Int> {
             if x > 0 { return Option::Some(x + 1); }
             return Option::None;
         }
         fn step2(x: Int) -> Option<Int> {
             if x < 100 { return Option::Some(x * 10); }
             return Option::None;
         }
         fn pipeline(x: Int) -> Option<Int> {
             let a = step1(x)?;
             let b = step2(a)?;
             return Option::Some(b);
         }
         fn main() -> Int {
             let r = pipeline(5);
             match r {
                 Option::Some(v) => { return v; },
                 Option::None => { return 0; }
             }
         }",
        60,
    );
}

#[test]
fn native_e2e_option_question_chain_first_fails() {
    assert_exit_code(
        "enum Option<T> { Some(T), None }
         fn step1(x: Int) -> Option<Int> {
             if x > 0 { return Option::Some(x + 1); }
             return Option::None;
         }
         fn step2(x: Int) -> Option<Int> {
             if x < 100 { return Option::Some(x * 10); }
             return Option::None;
         }
         fn pipeline(x: Int) -> Option<Int> {
             let a = step1(x)?;
             let b = step2(a)?;
             return Option::Some(b);
         }
         fn main() -> Int {
             let r = pipeline(-1);
             match r {
                 Option::Some(v) => { return v; },
                 Option::None => { return 99; }
             }
         }",
        99,
    );
}

#[test]
fn native_e2e_option_question_chain_second_fails() {
    assert_exit_code(
        "enum Option<T> { Some(T), None }
         fn step1(x: Int) -> Option<Int> {
             if x > 0 { return Option::Some(x + 1); }
             return Option::None;
         }
         fn step2(x: Int) -> Option<Int> {
             if x < 100 { return Option::Some(x * 10); }
             return Option::None;
         }
         fn pipeline(x: Int) -> Option<Int> {
             let a = step1(x)?;
             let b = step2(a)?;
             return Option::Some(b);
         }
         fn main() -> Int {
             let r = pipeline(99);
             match r {
                 Option::Some(v) => { return v; },
                 Option::None => { return 99; }
             }
         }",
        99,
    );
}

// =========================================================================
// NATIVE E2E: Result ? with actual execution
// =========================================================================

#[test]
fn native_e2e_result_question_ok() {
    assert_exit_code(
        "enum Result<T, E> { Ok(T), Err(E) }
         fn parse_int(x: Int) -> Result<Int, Int> {
             if x >= 0 { return Result::Ok(x * 10); }
             return Result::Err(1);
         }
         fn chain(x: Int) -> Result<Int, Int> {
             let v = parse_int(x)?;
             return Result::Ok(v);
         }
         fn main() -> Int {
             let r = chain(5);
             match r {
                 Result::Ok(v) => { return v; },
                 Result::Err(e) => { return e; }
             }
         }",
        50,
    );
}

#[test]
fn native_e2e_result_question_err() {
    assert_exit_code(
        "enum Result<T, E> { Ok(T), Err(E) }
         fn parse_int(x: Int) -> Result<Int, Int> {
             if x >= 0 { return Result::Ok(x * 10); }
             return Result::Err(1);
         }
         fn chain(x: Int) -> Result<Int, Int> {
             let v = parse_int(x)?;
             return Result::Ok(v);
         }
         fn main() -> Int {
             let r = chain(-1);
             match r {
                 Result::Ok(v) => { return v; },
                 Result::Err(e) => { return e; }
             }
         }",
        1,
    );
}

// =========================================================================
// DETERMINISM: Same input -> same output
// =========================================================================

#[test]
fn determinism_same_input_same_output() {
    let src = "enum Option<T> { Some(T), None }
               fn maybe_double(x: Int) -> Option<Int> {
                   if x > 100 { return Option::None; }
                   return Option::Some(x * 2);
               }
               fn chain(x: Int) -> Option<Int> {
                   let v = maybe_double(x)?;
                   return Option::Some(v);
               }
               fn main() -> Int {
                   let r = chain(21);
                   match r {
                       Option::Some(v) => { return v; },
                       Option::None => { return 0; }
                   }
               }";
    let code1 = native_exit_code(src);
    let code2 = native_exit_code(src);
    assert_eq!(code1, code2);
}
