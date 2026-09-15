//! Assertion library tests — Session 108 (S78).
//!
//! Builds `stdlib/assert.mink` together with each test body through the real
//! `mink build` CLI and runs the resulting Windows PE, asserting pass/fail
//! exit codes and diagnostic output.

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
    let path = std::env::temp_dir().join(format!("mink_assert_{n}_{name}.mink"));
    std::fs::write(&path, content).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, String) {
    let lib = assert_lib();
    let source = format!("{}\n{}", lib, test_body);
    let path = temp_source("test", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        return (-1, stderr);
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let code = run.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, format!("{stdout}{stderr}"))
}

fn lines_of(out: &str) -> Vec<String> {
    out.lines()
        .filter(|l| !l.is_empty() && !l.contains("runtime error") && !l.contains("memory leak"))
        .map(|l| l.trim().to_string())
        .collect()
}

// ============================================================
// PASS PATHS
// ============================================================

#[test]
fn a01_assert_true_passes() {
    let body = r#"
fn main() {
    assert_true(true);
    rt_print_str("ok\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "exit 0 expected, got {code}: {out}");
    assert_eq!(lines_of(&out), vec!["ok"]);
}

#[test]
fn a02_assert_false_passes() {
    let body = r#"
fn main() {
    assert_false(false);
    rt_print_str("ok\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "exit 0 expected, got {code}: {out}");
    assert_eq!(lines_of(&out), vec!["ok"]);
}

#[test]
fn a03_assert_eq_int_passes() {
    let body = r#"
fn main() {
    assert_eq_int(42, 42);
    assert_eq_int(0, 0);
    assert_eq_int(-1, -1);
    rt_print_str("ok\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "exit 0 expected, got {code}: {out}");
    assert_eq!(lines_of(&out), vec!["ok"]);
}

#[test]
fn a04_assert_ne_int_passes() {
    let body = r#"
fn main() {
    assert_ne_int(1, 2);
    assert_ne_int(0, -1);
    rt_print_str("ok\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "exit 0 expected, got {code}: {out}");
    assert_eq!(lines_of(&out), vec!["ok"]);
}

#[test]
fn a05_assert_eq_str_passes() {
    let body = r#"
fn main() {
    assert_eq_str("hello", "hello");
    assert_eq_str("", "");
    rt_print_str("ok\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "exit 0 expected, got {code}: {out}");
    assert_eq!(lines_of(&out), vec!["ok"]);
}

#[test]
fn a06_assert_ne_str_passes() {
    let body = r#"
fn main() {
    assert_ne_str("hello", "world");
    rt_print_str("ok\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "exit 0 expected, got {code}: {out}");
    assert_eq!(lines_of(&out), vec!["ok"]);
}

// ============================================================
// FAIL PATHS
// ============================================================

#[test]
fn a10_assert_true_fails() {
    let body = r#"
fn main() {
    assert_true(false);
    rt_print_str("REACHED\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 1, "exit 1 expected on failure, got {code}");
    assert!(
        out.contains("assertion failed"),
        "diagnostic missing: {out}"
    );
    assert!(
        !out.contains("REACHED"),
        "should not reach past assertion: {out}"
    );
}

#[test]
fn a11_assert_false_fails() {
    let body = r#"
fn main() {
    assert_false(true);
    rt_print_str("REACHED\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 1, "exit 1 expected on failure, got {code}");
    assert!(
        out.contains("assertion failed"),
        "diagnostic missing: {out}"
    );
    assert!(
        !out.contains("REACHED"),
        "should not reach past assertion: {out}"
    );
}

#[test]
fn a12_assert_eq_int_fails() {
    let body = r#"
fn main() {
    assert_eq_int(42, 99);
    rt_print_str("REACHED\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 1, "exit 1 expected on failure, got {code}");
    assert!(out.contains("42"), "expected value in diagnostic: {out}");
    assert!(out.contains("99"), "got value in diagnostic: {out}");
    assert!(
        !out.contains("REACHED"),
        "should not reach past assertion: {out}"
    );
}

#[test]
fn a13_assert_ne_int_fails() {
    let body = r#"
fn main() {
    assert_ne_int(7, 7);
    rt_print_str("REACHED\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 1, "exit 1 expected on failure, got {code}");
    assert!(
        out.contains("assertion failed"),
        "diagnostic missing: {out}"
    );
    assert!(
        !out.contains("REACHED"),
        "should not reach past assertion: {out}"
    );
}

#[test]
fn a14_assert_eq_str_fails() {
    let body = r#"
fn main() {
    assert_eq_str("foo", "bar");
    rt_print_str("REACHED\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 1, "exit 1 expected on failure, got {code}");
    assert!(
        out.contains("assertion failed"),
        "diagnostic missing: {out}"
    );
    assert!(
        !out.contains("REACHED"),
        "should not reach past assertion: {out}"
    );
}

#[test]
fn a15_assert_ne_str_fails() {
    let body = r#"
fn main() {
    assert_ne_str("same", "same");
    rt_print_str("REACHED\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 1, "exit 1 expected on failure, got {code}");
    assert!(
        out.contains("assertion failed"),
        "diagnostic missing: {out}"
    );
    assert!(
        !out.contains("REACHED"),
        "should not reach past assertion: {out}"
    );
}

// ============================================================
// BOUNDARY / EDGE CASES
// ============================================================

#[test]
fn a20_assert_eq_int_zeros() {
    let body = r#"
fn main() {
    assert_eq_int(0, 0);
    rt_print_str("ok\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0);
    assert_eq!(lines_of(&out), vec!["ok"]);
}

#[test]
fn a21_assert_eq_int_extremes() {
    let body = r#"
fn main() {
    assert_eq_int(9223372036854775807, 9223372036854775807);
    assert_eq_int(-9223372036854775808, -9223372036854775808);
    rt_print_str("ok\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0);
    assert_eq!(lines_of(&out), vec!["ok"]);
}

#[test]
fn a22_assert_eq_str_empty() {
    let body = r#"
fn main() {
    assert_eq_str("", "");
    rt_print_str("ok\n");
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0);
    assert_eq!(lines_of(&out), vec!["ok"]);
}

// ============================================================
// OWNERSHIP / REPEATED CALLS
// ============================================================

#[test]
fn a30_ownership_repeated_assertions() {
    let body = r#"
fn main() {
    let mut i = 0;
    while i < 200 {
        assert_eq_int(i, i);
        assert_true(true);
        assert_false(false);
        assert_ne_int(i, i + 1);
        i = i + 1;
    }
    rt_print_int(1);
    rt_exit(0);
}
"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "exit 0 expected, got {code}: {out}");
    let ints: Vec<i64> = out.lines().filter_map(|l| l.trim().parse().ok()).collect();
    assert_eq!(ints, vec![1]);
}
