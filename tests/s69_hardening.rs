//! S69 hardening tests — Session 108 Pass 2.
//!
//! Boundary, stress, and negative-path tests for strftime/strptime.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn time_lib() -> String {
    std::fs::read_to_string("stdlib/time.mink").expect("failed to read stdlib/time.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_s69h_{n}_{name}.mink"));
    std::fs::write(&path, content).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, String) {
    let lib = time_lib();
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
    let run = Command::new(&exe).output().unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let code = run.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, stdout)
}

fn lines_of(out: &str) -> Vec<String> {
    out.lines()
        .filter(|l| !l.is_empty() && !l.contains("runtime error") && !l.contains("memory leak"))
        .map(|l| l.trim().to_string())
        .collect()
}

fn ints_of(out: &str) -> Vec<i64> {
    out.lines().filter_map(|l| l.trim().parse().ok()).collect()
}

// ============================================================
// BOUNDARY TESTS
// ============================================================

#[test]
fn h01_strftime_min_epoch() {
    let body = r#"
fn main() {
    let d = time_strftime(0, "%F %T");
    rt_print_str(d);
    rt_str_free(d);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(lines_of(&out), vec!["1970-01-01 00:00:00"]);
}

#[test]
fn h02_strftime_large_epoch() {
    // 2099-12-31 23:59:59 UTC
    let body = r#"
fn main() {
    let d = time_strftime(4102444799, "%Y-%m-%d %H:%M:%S");
    rt_print_str(d);
    rt_str_free(d);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(lines_of(&out), vec!["2099-12-31 23:59:59"]);
}

#[test]
fn h03_strftime_empty_format() {
    let body = r#"
fn main() {
    let d = time_strftime(1000000, "");
    let l = rt_str_len(d);
    rt_print_int(l);
    rt_str_free(d);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(ints_of(&out), vec![0]);
}

#[test]
fn h04_strftime_literal_only() {
    let body = r#"
fn main() {
    let d = time_strftime(0, "hello world");
    rt_print_str(d);
    rt_str_free(d);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(lines_of(&out), vec!["hello world"]);
}

#[test]
fn h05_strptime_full_iso() {
    let body = r#"
fn main() {
    let (p, ok) = time_strptime("2026-09-15 14:30:00", "%F %T");
    let mut ok_i = 0;
    if ok { ok_i = 1; }
    rt_print_int(ok_i);
    let d = time_strftime(p, "%F %T");
    rt_print_str(d);
    rt_str_free(d);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    let lines = lines_of(&out);
    assert_eq!(lines[0], "1");
    assert_eq!(lines[1], "2026-09-15 14:30:00");
}

#[test]
fn h06_strptime_partial_input_fails() {
    let body = r#"
fn main() {
    let (p, ok) = time_strptime("2024", "%F");
    let mut ok_i = 0;
    if ok { ok_i = 1; }
    rt_print_int(ok_i);
    rt_print_int(p);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(ints_of(&out), vec![0, 0]);
}

#[test]
fn h07_strptime_wrong_delimiters_fails() {
    let body = r#"
fn main() {
    let (p, ok) = time_strptime("2024/03/15", "%F");
    let mut ok_i = 0;
    if ok { ok_i = 1; }
    rt_print_int(ok_i);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(ints_of(&out), vec![0]);
}

#[test]
fn h08_strptime_empty_input_fails() {
    let body = r#"
fn main() {
    let (p, ok) = time_strptime("", "%F");
    let mut ok_i = 0;
    if ok { ok_i = 1; }
    rt_print_int(ok_i);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(ints_of(&out), vec![0]);
}

// ============================================================
// STRESS / OWNERSHIP
// ============================================================

#[test]
fn h09_1000_strftime_calls() {
    let body = r#"
fn main() {
    let mut i = 0;
    while i < 1000 {
        let d = time_strftime(i * 86400, "%Y-%m-%d");
        rt_str_free(d);
        i = i + 1;
    }
    rt_print_int(1);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(ints_of(&out), vec![1]);
}

#[test]
fn h10_strptime_then_strftime_100_rounds() {
    let body = r#"
fn main() {
    let mut i = 0;
    while i < 100 {
        let (p, ok) = time_strptime("2024-06-15", "%F");
        let mut ok_i = 0;
        if ok { ok_i = 1; }
        let d = time_strftime(p, "%F");
        rt_str_free(d);
        i = i + 1;
    }
    rt_print_int(1);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(ints_of(&out), vec![1]);
}

#[test]
fn h11_strftime_all_directives() {
    let body = r#"
fn main() {
    let ts = time_epoch(2024, 7, 4, 15, 30, 45);
    let s = time_strftime(ts, "%Y %y %m %d %H %M %S %A %a %B %b %%");
    rt_print_str(s);
    rt_str_free(s);
    rt_exit(0);
}"#;
    let (code, out) = build_and_run(body);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        lines_of(&out),
        vec!["2024 24 07 04 15 30 45 Thursday Thu July Jul %"]
    );
}
