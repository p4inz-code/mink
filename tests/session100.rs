//! Session 100 regression tests: R06 — runtime error-location metadata.
//!
//! Every test compiles a real MINK source with the `mink` binary and runs
//! the generated Windows executable, so each assertion goes through the
//! full source -> compiler -> image metadata -> native PE -> runtime
//! fail path chain.
//!
//! Covered:
//! - a runtime failure reports its exact source file and line;
//! - a failure inside a called function reports that function's line;
//! - a failure after earlier instrumented sites on other lines still
//!   reports the *failing* line (per-site ids, not a stale earlier one);
//! - failures not tied to one user operation (the exit-time leak scan)
//!   print no location line;
//! - successful runs print no location and exit cleanly;
//! - the location survives copying the executable away from the source
//!   tree (no source files, no development checkout, no CWD dependency);
//! - source paths containing spaces are reported intact;
//! - repeated execution is byte-for-byte deterministic;
//! - string-byte out-of-range (E-R09) also carries its location.
//!
//! Also covers W14 — Windows home-directory discovery (`rt_home_dir`):
//! - returns the current `USERPROFILE` when set;
//! - falls back to `HOMEDRIVE` + `HOMEPATH` when `USERPROFILE` is absent;
//! - yields an owned empty string (no crash, no E-R08) when only a drive
//!   or nothing is available;
//! - frees every internal allocation (exit 0 keeps the leak checker
//!   silent).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn dir(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mink_s100_{label}_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs an executable, returning (exit code, stdout bytes, stderr bytes).
fn run_exe(exe: &Path) -> (i32, Vec<u8>, Vec<u8>) {
    let out = Command::new(exe).output().expect("run exe");
    (out.status.code().unwrap_or(-1), out.stdout, out.stderr)
}

/// Runs an executable with a controlled child environment: each entry in
/// `vars` is (name, Some(value)) to set or (name, None) to remove. This
/// lets the W14 `rt_home_dir` fallback paths (USERPROFILE absent, etc.)
/// be exercised deterministically without touching the test process's own
/// environment.
fn run_exe_env(exe: &Path, vars: &[(&str, Option<&str>)]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut cmd = Command::new(exe);
    for (name, value) in vars {
        match value {
            Some(v) => {
                cmd.env(name, v);
            }
            None => {
                cmd.env_remove(name);
            }
        }
    }
    let out = cmd.output().expect("run exe with env");
    (out.status.code().unwrap_or(-1), out.stdout, out.stderr)
}

/// Builds `source` (written to `<dir>/<file>.mink`) with the real CLI and
/// returns the generated executable's path.
fn build_file(dir: &Path, file: &str, source: &str) -> PathBuf {
    let path = dir.join(file);
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(
        out.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "generated executable missing");
    exe
}

/// The W14 home-dir probe program: prints the home length, the home
/// content, then frees the result and exits cleanly (exit 0 also proves
/// the leak checker stays silent — every internal allocation must be
/// freed or the exit-time scan fails).
const HOME_PROBE: &str = r#"fn main() {
    let h = rt_home_dir();
    rt_print_int(rt_str_len(h));
    rt_print_str(h);
    rt_str_free(h);
    rt_exit(0);
}
"#;

/// The expected runtime-error location line: `  at <file>:<line>\r\n`.
fn at_line(file: &Path, line: u32) -> Vec<u8> {
    format!("  at {}:{}\r\n", file.display(), line).into_bytes()
}

// =========================================================================
// R06 core: file + line reporting
// =========================================================================

/// A failing array access inside `main` reports the exact source line.
#[test]
fn runtime_error_reports_file_and_line() {
    let d = dir("loc");
    // `let x = a[i];` with `i = 9` is on line 4.
    let src = r#"fn main() {
    let a = [1, 2, 3];
    let i = 9;
    let x = a[i];
    rt_print_int(x);
    return 0;
}
"#;
    let path = d.join("arr.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(out.status.success(), "build failed");
    let exe = path.with_extension("exe");
    let (code, _stdout, stderr) = run_exe(&exe);

    assert_eq!(code, 110, "E-R10 exit code");
    let err = String::from_utf8_lossy(&stderr);
    assert!(
        err.contains("runtime error[E-R10]"),
        "stable error identity, got: {err}"
    );
    assert!(
        stderr.ends_with(&at_line(&path, 4)),
        "expected location line {:?}, got: {err}",
        String::from_utf8_lossy(&at_line(&path, 4))
    );
}

/// A failure inside a called function reports that function's own line,
/// not the call site's.
#[test]
fn called_function_reports_its_own_line() {
    let d = dir("call");
    // `return a[i];` (with `i = 9`) is on line 3 of `boom`.
    let src = r#"fn boom(i: Int) -> Int {
    let a = [10, 20, 30];
    return a[i];
}
fn main() {
    let x = boom(9);
    rt_print_int(x);
    return 0;
}
"#;
    let path = d.join("call.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(out.status.success(), "build failed");
    let exe = path.with_extension("exe");
    let (code, _stdout, stderr) = run_exe(&exe);

    assert_eq!(code, 110, "E-R10 exit code");
    assert!(
        stderr.ends_with(&at_line(&path, 3)),
        "expected boom's line 3, got: {}",
        String::from_utf8_lossy(&stderr)
    );
}

/// Valid instrumented accesses on earlier lines must not shadow the line
/// of the access that actually fails.
#[test]
fn failing_access_reports_its_own_line_after_valid_ones() {
    let d = dir("multi");
    let src = r#"fn main() {
    let a = [10, 20, 30, 40];
    let b = [1, 2, 3];
    let s = a[1] + a[2];
    rt_print_int(s);
    let j = 9;
    let t = b[j];
    rt_print_int(t);
    return 0;
}
"#;
    // The failing access `b[j]` is on line 7.
    let path = d.join("multi.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(out.status.success(), "build failed");
    let exe = path.with_extension("exe");
    let (code, _stdout, stderr) = run_exe(&exe);

    assert_eq!(code, 110, "E-R10 exit code");
    // The failing access is `b[j]` on line 7, not the valid `a[1] + a[2]`
    // on line 4.
    assert!(
        stderr.ends_with(&at_line(&path, 7)),
        "expected the failing line 7, got: {}",
        String::from_utf8_lossy(&stderr)
    );
}

/// String-byte out-of-range (E-R09) also carries its location.
#[test]
fn string_byte_error_reports_file_and_line() {
    let d = dir("strovr");
    // `rt_str_byte(s, 9)` is on line 3.
    let src = r#"fn main() {
    let s = "abc";
    let b = rt_str_byte(s, 9);
    rt_print_int(b);
    return 0;
}
"#;
    let path = d.join("s.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(out.status.success(), "build failed");
    let exe = path.with_extension("exe");
    let (code, _stdout, stderr) = run_exe(&exe);

    assert_eq!(code, 109, "E-R09 exit code");
    assert!(
        String::from_utf8_lossy(&stderr).contains("runtime error[E-R09]"),
        "stable error identity"
    );
    assert!(
        stderr.ends_with(&at_line(&path, 3)),
        "expected line 3, got: {}",
        String::from_utf8_lossy(&stderr)
    );
}

// =========================================================================
// R06 negative cases: no fabricated locations
// =========================================================================

/// A leak detected at exit (E-R06) is not tied to one user operation, so
/// it must print no location line.
#[test]
fn exit_leak_reports_no_location() {
    let d = dir("leak");
    let src = r#"fn main() {
    let p = rt_alloc(256);
    rt_exit(0);
    return 0;
}
"#;
    let path = d.join("leak.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(out.status.success(), "build failed");
    let exe = path.with_extension("exe");
    let (code, _stdout, stderr) = run_exe(&exe);

    assert_eq!(code, 106, "E-R06 exit code");
    let err = String::from_utf8_lossy(&stderr);
    assert!(err.contains("runtime error[E-R06]"), "got: {err}");
    assert!(
        !err.contains("  at "),
        "a leak at exit is not a user operation; no location may be reported: {err}"
    );
}

/// A successful run prints no location line and exits cleanly.
#[test]
fn success_path_reports_nothing() {
    let d = dir("ok");
    let src = r#"fn main() {
    let a = [1, 2, 3];
    rt_print_int(a[1]);
    return 0;
}
"#;
    let path = d.join("ok.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(out.status.success(), "build failed");
    let exe = path.with_extension("exe");
    let (code, stdout, stderr) = run_exe(&exe);

    assert_eq!(code, 0, "clean exit");
    assert_eq!(stdout, b"2\r\n", "program output");
    assert!(stderr.is_empty(), "no stderr on success: {:?}", stderr);
}

// =========================================================================
// R06 standalone behavior
// =========================================================================

/// The location is embedded in the image: after copying the executable to
/// an empty directory (no source files anywhere nearby) and renaming it,
/// it still reports the original source file and line.
#[test]
fn copied_executable_still_reports_location() {
    let d = dir("copy_src");
    let src = r#"fn main() {
    let a = [1, 2, 3];
    let i = 9;
    let x = a[i];
    rt_print_int(x);
    return 0;
}
"#;
    let path = d.join("orig name.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(out.status.success(), "build failed");
    let exe = path.with_extension("exe");

    // Copy into a fresh, empty directory under a new name.
    let target_dir = dir("copy_dst");
    let renamed = target_dir.join("relocated app.exe");
    std::fs::copy(&exe, &renamed).unwrap();
    let (code, _stdout, stderr) = run_exe(&renamed);

    assert_eq!(code, 110, "E-R10 exit code");
    // The location must still name the *original* source file.
    assert!(
        stderr.ends_with(&at_line(&path, 4)),
        "copied exe must report the original location, got: {}",
        String::from_utf8_lossy(&stderr)
    );
}

/// A source path containing spaces round-trips intact through the
/// embedded metadata.
#[test]
fn source_path_with_spaces_reports_intact() {
    let d = dir("spaced dir with spaces");
    let src = r#"fn main() {
    let a = [1, 2, 3];
    let i = 9;
    let x = a[i];
    rt_print_int(x);
    return 0;
}
"#;
    // The failing access `a[i]` is on line 4.
    let path = d.join("my program.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(out.status.success(), "build failed");
    let exe = path.with_extension("exe");
    let (code, _stdout, stderr) = run_exe(&exe);

    assert_eq!(code, 110, "E-R10 exit code");
    assert!(
        stderr.ends_with(&at_line(&path, 4)),
        "spaced path must be reported intact, got: {}",
        String::from_utf8_lossy(&stderr)
    );
}

// =========================================================================
// Determinism
// =========================================================================

/// Repeated executions produce byte-identical diagnostics.
#[test]
fn repeated_execution_is_deterministic() {
    let d = dir("det");
    let src = r#"fn boom(i: Int) -> Int {
    let a = [10, 20, 30];
    return a[i];
}
fn main() {
    let x = boom(9);
    rt_print_int(x);
    return 0;
}
"#;
    let path = d.join("det.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink().arg("build").arg(&path).output().expect("mink build");
    assert!(out.status.success(), "build failed");
    let exe = path.with_extension("exe");

    let mut first: Option<Vec<u8>> = None;
    for _ in 0..3 {
        let (code, _stdout, stderr) = run_exe(&exe);
        assert_eq!(code, 110, "E-R10 exit code");
        if let Some(ref prev) = first {
            assert_eq!(*prev, stderr, "stderr must be byte-identical across runs");
        } else {
            first = Some(stderr);
        }
    }
    let err = String::from_utf8_lossy(first.as_ref().unwrap());
    assert!(err.contains("  at "), "location line present: {err}");
}

// =========================================================================
// W14: Windows home-directory discovery (rt_home_dir)
// =========================================================================

/// Parses the two stdout lines of [`HOME_PROBE`]: length, then content.
fn parse_home_probe(stdout: &[u8]) -> (usize, String) {
    let s = String::from_utf8_lossy(stdout);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines.len(), 2, "expected <len>\r\n<content>\r\n, got: {s}");
    let len: usize = lines[0].parse().expect("length line");
    (len, lines[1].to_string())
}

/// With `USERPROFILE` set (the normal case), `rt_home_dir` returns its
/// exact value.
#[test]
fn home_dir_returns_userprofile() {
    let userprofile = std::env::var("USERPROFILE").expect("test machine sets USERPROFILE");
    assert!(!userprofile.is_empty());
    let d = dir("home_primary");
    let exe = build_file(&d, "home.mink", HOME_PROBE);
    let (code, stdout, stderr) = run_exe(&exe);

    assert_eq!(
        code,
        0,
        "clean exit; stderr: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(stderr.is_empty(), "no leak diagnostics: {:?}", stderr);
    let (len, content) = parse_home_probe(&stdout);
    assert_eq!(content, userprofile, "content must equal USERPROFILE");
    assert_eq!(len, userprofile.len(), "length must be exact");
}

/// With `USERPROFILE` removed, `HOMEDRIVE` + `HOMEPATH` are concatenated.
#[test]
fn home_dir_falls_back_to_drive_plus_path() {
    let d = dir("home_fallback");
    let exe = build_file(&d, "home.mink", HOME_PROBE);
    let (code, stdout, stderr) = run_exe_env(
        &exe,
        &[
            ("USERPROFILE", None),
            ("HOMEDRIVE", Some("C:")),
            ("HOMEPATH", Some("\\Users\\FallbackTest")),
        ],
    );

    assert_eq!(
        code,
        0,
        "clean exit; stderr: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(stderr.is_empty(), "no leak diagnostics: {:?}", stderr);
    let (len, content) = parse_home_probe(&stdout);
    assert_eq!(content, "C:\\Users\\FallbackTest", "drive + path concat");
    assert_eq!(len, content.len(), "length must be exact");
}

/// A drive without a path is not a usable home: the result is an owned
/// empty string, and the previously allocated drive blob is freed (exit 0
/// keeps the leak checker silent).
#[test]
fn home_dir_drive_without_path_is_empty() {
    let d = dir("home_drive_only");
    let exe = build_file(&d, "home.mink", HOME_PROBE);
    let (code, stdout, stderr) = run_exe_env(
        &exe,
        &[
            ("USERPROFILE", None),
            ("HOMEDRIVE", Some("C:")),
            ("HOMEPATH", None),
        ],
    );

    assert_eq!(
        code,
        0,
        "clean exit; stderr: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(stderr.is_empty(), "no leak diagnostics: {:?}", stderr);
    let (len, content) = parse_home_probe(&stdout);
    assert_eq!(len, 0, "empty owned string");
    assert!(content.is_empty());
}

/// With no home variables at all, the result is an owned empty string and
/// no runtime error occurs (a missing `HOMEDRIVE` previously fell through
/// into a negative allocation size → E-R08).
#[test]
fn home_dir_with_nothing_set_is_empty_and_clean() {
    let d = dir("home_none");
    let exe = build_file(&d, "home.mink", HOME_PROBE);
    let (code, stdout, stderr) = run_exe_env(
        &exe,
        &[
            ("USERPROFILE", None),
            ("HOMEDRIVE", None),
            ("HOMEPATH", None),
        ],
    );

    assert_eq!(
        code,
        0,
        "clean exit, no E-R08; stderr: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(stderr.is_empty(), "no leak diagnostics: {:?}", stderr);
    let (len, content) = parse_home_probe(&stdout);
    assert_eq!(len, 0);
    assert!(content.is_empty());
}

/// Repeated calls that switch resolution branch mid-process (via
/// `rt_env_set`/`rt_env_remove`, which mutate the process's own
/// environment) must not corrupt allocator state or leak: every call
/// frees its internals and the caller frees each result.
#[test]
fn home_dir_repeated_calls_are_clean() {
    let d = dir("home_repeat");
    let src = r#"fn main() {
    let a = rt_home_dir();
    rt_print_int(rt_str_len(a));
    rt_str_free(a);
    rt_env_remove("USERPROFILE");
    rt_env_set("HOMEDRIVE", "D:");
    rt_env_set("HOMEPATH", "\\Data\\Test");
    let b = rt_home_dir();
    rt_print_int(rt_str_len(b));
    rt_print_str(b);
    rt_str_free(b);
    rt_exit(0);
}
"#;
    let exe = build_file(&d, "rep.mink", src);
    // First call sees USERPROFILE; the process then removes it and sets
    // the fallback pair, so the second call must take the fallback path.
    let (code, stdout, stderr) =
        run_exe_env(&exe, &[("USERPROFILE", Some("C:\\Users\\OnlyForFirst"))]);
    assert_eq!(
        code,
        0,
        "clean exit; stderr: {}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(stderr.is_empty(), "no leak diagnostics: {:?}", stderr);
    let s = String::from_utf8_lossy(&stdout);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines.len(), 3, "len_a, len_b, content_b; got: {s}");
    assert_eq!(lines[0], "21", "USERPROFILE value length");
    assert_eq!(lines[1], "12", "fallback concat length");
    assert_eq!(lines[2], "D:\\Data\\Test", "fallback content");
}
