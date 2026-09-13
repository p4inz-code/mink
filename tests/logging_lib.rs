//! Logging library integration tests — Session 108 (S74).
//!
//! Exercises `stdlib/logging.mink` end to end: native Windows PE builds via
//! the real `mink build` CLI, threshold filtering captured from stderr,
//! level-name round trips, boundary inputs, and repeated-call ownership.
//!
//! The module keeps its threshold in the process environment
//! (`MINK_LOG_LEVEL`) because MINK has no mutable globals; each generated
//! executable is its own process, so configuration never leaks between
//! tests.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn log_lib() -> String {
    std::fs::read_to_string("stdlib/logging.mink").expect("failed to read stdlib/logging.mink")
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_log_test_{n}_{name}.mink"));
    std::fs::write(&path, content.replace("\r\n", "\n")).unwrap();
    path
}

/// Builds `stdlib/logging.mink` + `test_body` and runs the resulting PE.
/// Returns `(exit code, stdout, stderr)`.
fn build_and_run(test_body: &str) -> (i32, String, String) {
    let lib = log_lib();
    let source = format!("{}\n{}", lib, test_body);
    let path = temp_source("test", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        return (-1, stdout, stderr);
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let code = run.status.code().unwrap_or(-1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, stdout, stderr)
}

fn first_int(output: &str) -> i64 {
    output
        .lines()
        .next()
        .unwrap_or("0")
        .trim()
        .parse()
        .unwrap_or(0)
}

fn all_ints(output: &str) -> Vec<i64> {
    output
        .lines()
        .filter(|l| !l.is_empty() && !l.contains("runtime error") && !l.contains("memory leak"))
        .filter_map(|l| l.trim().parse().ok())
        .collect()
}

fn assert_clean(code: i32, out: &str, err: &str) {
    assert!(
        code == 0,
        "unexpected exit code {code}: stdout={out:?} stderr={err:?}"
    );
}

// ============================================================
// DEFAULT THRESHOLD (INFO)
// ============================================================

#[test]
fn l01_default_threshold_emits_info_and_above() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(20);
    log_debug("dbg");
    log_info("inf");
    log_warning("wrn");
    log_error("err");
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(
        err, "INFO: inf\nWARNING: wrn\nERROR: err\n",
        "stderr: {err:?}"
    );
}

#[test]
fn l02_threshold_filters_lower_levels() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(30);
    log_debug("dbg");
    log_info("inf");
    log_warning("wrn");
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(err, "WARNING: wrn\n", "stderr: {err:?}");
}

#[test]
fn l03_critical_visible_at_every_threshold() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(50);
    log_warning("no");
    log_critical("boom");
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(err, "CRITICAL: boom\n", "stderr: {err:?}");
}

#[test]
fn l04_suppressed_record_reports_zero_bytes() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(40);
    let a = log_info("nope");
    rt_print_int(a);
    let b = log_error("yes");
    if b > 0 { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(all_ints(&out), vec![0, 1], "stdout: {out:?}");
    assert_eq!(err, "ERROR: yes\n", "stderr: {err:?}");
}

// ============================================================
// LEVEL NAMES
// ============================================================

#[test]
fn l05_level_name_round_trip() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    rt_print_int(log_level_from_name("DEBUG"));
    rt_print_int(log_level_from_name("INFO"));
    rt_print_int(log_level_from_name("WARNING"));
    rt_print_int(log_level_from_name("WARN"));
    rt_print_int(log_level_from_name("ERROR"));
    rt_print_int(log_level_from_name("CRITICAL"));
    rt_print_int(log_level_from_name("CRIT"));
    rt_print_int(log_level_from_name("nonsense"));
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(
        all_ints(&out),
        vec![10, 20, 30, 30, 40, 50, 50, 0],
        "stdout: {out:?}"
    );
}

#[test]
fn l06_level_name_lookup() {
    let (code, out, err) = build_and_run(
        r#"
fn check(level: Int, want: Str) -> Int {
    let name = log_level_name(level);
    let r = rt_str_eq(name, want);
    let mut v = 0;
    if r { v = 1; }
    rt_str_free(name);
    rt_str_free(want);
    return v;
}
fn main() {
    rt_print_int(check(10, "DEBUG"));
    rt_print_int(check(20, "INFO"));
    rt_print_int(check(30, "WARNING"));
    rt_print_int(check(40, "ERROR"));
    rt_print_int(check(50, "CRITICAL"));
    rt_print_int(check(99, "LEVEL"));
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(all_ints(&out), vec![1, 1, 1, 1, 1, 1], "stdout: {out:?}");
    assert_eq!(err, "", "unexpected stderr: {err:?}");
}

#[test]
fn l07_set_level_by_name() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    rt_print_int(log_set_level_name("DEBUG"));
    log_debug("dbg");
    rt_print_int(log_set_level_name("bogus"));
    log_debug("dbg2");
    rt_print_int(log_get_level());
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(all_ints(&out), vec![0, -1, 10], "stdout: {out:?}");
    assert_eq!(err, "DEBUG: dbg\nDEBUG: dbg2\n", "stderr: {err:?}");
}

// ============================================================
// log_enabled
// ============================================================

#[test]
fn l08_enabled_respects_threshold() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(30);
    if log_enabled(10) { rt_print_int(1); } else { rt_print_int(0); }
    if log_enabled(30) { rt_print_int(1); } else { rt_print_int(0); }
    if log_enabled(50) { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(all_ints(&out), vec![0, 1, 1], "stdout: {out:?}");
    assert_eq!(err, "", "unexpected stderr: {err:?}");
}

// ============================================================
// BOUNDARIES
// ============================================================

#[test]
fn l09_empty_message() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(20);
    log_info("");
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(err, "INFO: \n", "stderr: {err:?}");
}

#[test]
fn l10_unknown_numeric_level_uses_generic_label() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(0);
    log_log(77, "odd");
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(err, "LEVEL: odd\n", "stderr: {err:?}");
}

#[test]
fn l11_large_message_round_trips() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(20);
    // Build a 4000-byte message without a length constant.
    let mut msg = rt_str_alloc(4000);
    let mut i = 0;
    while i < 4000 {
        rt_str_set_byte(msg, i, 120); // 'x'
        i = i + 1;
    }
    let n = log_info(msg);
    rt_print_int(n);
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    // "INFO: " + 4000 + "\n"
    assert_eq!(first_int(&out), 4007, "stdout: {out:?}");
    assert_eq!(err.len(), 4007, "stderr length: {}", err.len());
    assert!(err.starts_with("INFO: "), "stderr prefix: {err:?}");
    assert!(err.ends_with('\n'), "stderr suffix: {err:?}");
    assert!(
        err[6..4006].bytes().all(|b| b == b'x'),
        "stderr body must be all 'x'"
    );
}

// ============================================================
// OWNERSHIP / REPETITION
// ============================================================

#[test]
fn l12_repeated_calls_no_leak() {
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(30);
    let mut i = 0;
    while i < 200 {
        log_debug("d");
        log_info("i");
        log_warning("w");
        i = i + 1;
    }
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    let lines = err.lines().count();
    assert_eq!(lines, 200, "one line per emitted record: {lines}");
    assert!(err.starts_with("WARNING: w\n"), "stderr: {err:?}");
}

#[test]
fn l13_heap_message_ownership() {
    // The message is a heap string produced by an intrinsic; log_info must
    // consume it. A leak or double-free would surface as a nonzero exit.
    let (code, out, err) = build_and_run(
        r#"
fn main() {
    log_set_level(20);
    let mut i = 0;
    while i < 50 {
        let m = rt_str_from_int(i);
        log_info(m);
        i = i + 1;
    }
    rt_exit(0);
}
"#,
    );
    assert_clean(code, &out, &err);
    assert_eq!(err.lines().count(), 50, "stderr: {err:?}");
    assert_eq!(err.lines().next(), Some("INFO: 0"), "stderr: {err:?}");
    assert_eq!(err.lines().last(), Some("INFO: 49"), "stderr: {err:?}");
}
