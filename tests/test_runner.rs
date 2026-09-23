//! Tests for the `mink test` CLI command — Session 108 (S75/T07).
//!
//! Verifies test discovery, pass/fail reporting, compile-error handling,
//! and the no-test-functions diagnostic.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn assert_lib() -> String {
    std::fs::read_to_string("stdlib/assert.mink").expect("failed to read stdlib/assert.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_tr_{n}_{name}.mink"));
    std::fs::write(&path, content).unwrap();
    path
}

/// Runs `mink test <path>` and returns (exit_code, stdout+stderr combined).
fn run_mink_test(path: &std::path::Path) -> (i32, String) {
    let output = mink()
        .arg("test")
        .arg(path)
        .output()
        .expect("failed to run mink test");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (code, format!("{stdout}{stderr}"))
}

fn test_with_source(body: &str) -> (i32, String) {
    let path = temp_source("test", body);
    let result = run_mink_test(&path);
    let _ = std::fs::remove_file(&path);
    result
}

fn test_with_full_source(body: &str) -> (i32, String) {
    let lib = assert_lib();
    let source = format!("{}\n{}", lib, body);
    let path = temp_source("full", &source);
    let result = run_mink_test(&path);
    let _ = std::fs::remove_file(&path);
    result
}

// ============================================================
// MODULE RESOLUTION
// ============================================================

/// A test file's project-local `mod` declarations must resolve.
///
/// The wrapper that `mink test` generates is compiled with the file under
/// test as its root, and module resolution is relative to that root file's
/// directory. Writing the wrapper to the system temp directory made
/// `mod helper;` look for `%TEMP%\helper.mink` and fail with
/// "module file ... not found".
#[test]
fn t13_test_command_resolves_a_sibling_module() {
    let dir = std::env::temp_dir().join(format!(
        "mink_tr_dir_{}_{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).expect("create test directory");
    std::fs::write(
        dir.join("helper.mink"),
        "pub fn triple(x: Int) -> Int { return x * 3; }\n",
    )
    .expect("write sibling module");
    let test_path = dir.join("test_helper.mink");
    std::fs::write(
        &test_path,
        "mod helper;\nuse helper::triple;\n\nfn test_triple() {\n    if triple(14) != 42 { rt_exit(1); }\n    return;\n}\n\nfn main() { return 0; }\n",
    )
    .expect("write test source");

    let (code, out) = run_mink_test(&test_path);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        code == 0,
        "the sibling module must resolve (exit {code}): {out}"
    );
    assert!(out.contains("PASS: test_triple"), "{out}");
    assert!(!out.contains("not found"), "{out}");
}

// ============================================================
// TEST DISCOVERY
// ============================================================

#[test]
fn t01_discovers_test_functions() {
    let body = r#"
fn test_one() { rt_exit(0); }
fn test_two() { rt_exit(0); }
fn helper() { }
fn main() { }
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 0, "expected 0, got {code}: {out}");
    assert!(out.contains("found 2 test(s)"), "count missing: {out}");
    assert!(out.contains("PASS: test_one"), "test_one missing: {out}");
    assert!(out.contains("PASS: test_two"), "test_two missing: {out}");
}

#[test]
fn t02_no_tests_found() {
    let body = r#"
fn helper() { }
fn main() { }
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 1, "expected 1, got {code}: {out}");
    assert!(
        out.contains("no test functions found"),
        "diagnostic missing: {out}"
    );
}

#[test]
fn t03_reports_pass_count() {
    let body = r#"
fn test_a() { rt_exit(0); }
fn test_b() { rt_exit(0); }
fn test_c() { rt_exit(0); }
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 0, "expected 0, got {code}: {out}");
    assert!(out.contains("3 passed"), "pass count missing: {out}");
    assert!(out.contains("0 failed"), "fail count missing: {out}");
}

// ============================================================
// PASS / FAIL REPORTING
// ============================================================

#[test]
fn t10_all_pass() {
    let body = r#"
fn test_ok1() { rt_exit(0); }
fn test_ok2() { rt_exit(0); }
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 0, "expected 0, got {code}: {out}");
    assert!(out.contains("2 passed"), "pass count: {out}");
    assert!(out.contains("0 failed"), "fail count: {out}");
    assert!(out.contains("PASS: test_ok1"), "test_ok1: {out}");
    assert!(out.contains("PASS: test_ok2"), "test_ok2: {out}");
}

#[test]
fn t11_one_fail() {
    let body = r#"
fn test_pass() { rt_exit(0); }
fn test_fail() { rt_exit(1); }
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 1, "expected 1, got {code}: {out}");
    assert!(out.contains("1 passed"), "pass count: {out}");
    assert!(out.contains("1 failed"), "fail count: {out}");
    assert!(out.contains("PASS: test_pass"), "test_pass: {out}");
    assert!(out.contains("FAIL: test_fail"), "test_fail: {out}");
    assert!(out.contains("Failed tests:"), "failure list: {out}");
}

#[test]
fn t12_all_fail() {
    let body = r#"
fn test_x() { rt_exit(1); }
fn test_y() { rt_exit(2); }
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 1, "expected 1, got {code}: {out}");
    assert!(out.contains("0 passed"), "pass count: {out}");
    assert!(out.contains("2 failed"), "fail count: {out}");
}

// ============================================================
// ASSERTION INTEGRATION
// ============================================================

#[test]
fn t20_assert_pass_in_test() {
    let body = r#"
fn test_assertions() {
    assert_true(true);
    assert_false(false);
    assert_eq_int(42, 42);
    assert_ne_int(1, 2);
    rt_print_str("done\n");
}
"#;
    let (code, out) = test_with_full_source(body);
    assert_eq!(code, 0, "expected 0, got {code}: {out}");
    assert!(
        out.contains("PASS: test_assertions"),
        "test_assertions: {out}"
    );
}

#[test]
fn t21_assert_fail_in_test() {
    let body = r#"
fn test_bad() {
    assert_eq_int(1, 2);
}
"#;
    let (code, out) = test_with_full_source(body);
    assert_eq!(code, 1, "expected 1, got {code}: {out}");
    assert!(out.contains("FAIL: test_bad"), "test_bad: {out}");
}

// ============================================================
// EDGE CASES
// ============================================================

#[test]
fn t30_single_test() {
    let body = r#"
fn test_only() { rt_exit(0); }
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 0, "expected 0, got {code}: {out}");
    assert!(out.contains("found 1 test(s)"), "count: {out}");
    assert!(out.contains("PASS: test_only"), "test_only: {out}");
}

#[test]
fn t31_test_with_params_ignored() {
    // Functions with parameters that look like tests should be skipped
    // by the simple scanner (they contain ':' in the param list).
    let body = r#"
fn test_with_args(x: Int) { rt_exit(0); }
fn test_no_args() { rt_exit(0); }
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 0, "expected 0, got {code}: {out}");
    // Should find only test_no_args (test_with_args has params)
    assert!(out.contains("found 1 test(s)"), "count: {out}");
    assert!(out.contains("PASS: test_no_args"), "test_no_args: {out}");
    assert!(
        !out.contains("test_with_args"),
        "should skip param test: {out}"
    );
}

#[test]
fn t32_existing_main_stripped() {
    // The source has a main; the test runner should strip it and add its own.
    let body = r#"
fn main() {
    rt_print_str("this should be stripped\n");
}
fn test_works() { rt_exit(0); }
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 0, "expected 0, got {code}: {out}");
    assert!(out.contains("PASS: test_works"), "test_works: {out}");
}

#[test]
fn t33_compile_error_reported() {
    let body = r#"
fn test_broken() {
    let x: Int = "not_an_int";
}
"#;
    let (code, out) = test_with_source(body);
    assert_eq!(code, 1, "expected 1, got {code}: {out}");
    assert!(out.contains("FAIL: test_broken"), "test_broken: {out}");
    assert!(
        out.contains("compile error"),
        "compile error missing: {out}"
    );
}
