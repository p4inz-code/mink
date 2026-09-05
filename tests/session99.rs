//! Session 99 regression tests: Windows Wave A tranche 1 capabilities.
//!
//! Each test compiles a real MINK source with the `mink` binary and runs
//! the generated executable, so every assertion goes through the full
//! source -> compiler -> native PE -> Windows runtime chain.
//!
//! Covered:
//! - environment intrinsics (`rt_env_set` / `rt_env_get` / `rt_env_has` /
//!   `rt_env_remove`) including the Session 99 free-list reuse regression
//!   (root cause: `rt_to_cstr`'s NUL terminator was an 8-byte zero store
//!   that overran into the adjacent CStr block, zeroing the next variable's
//!   name and making `SetEnvironmentVariableA` fail with error 87);
//! - command-line arguments (`rt_argc` / `rt_argv`) incl. spaces and empty
//!   arguments;
//! - stdin read to EOF;
//! - sleep;
//! - stderr write;
//! - float -> Str;
//! - `rt_str_format` (substitution, escapes, missing arguments).

use std::path::PathBuf;
use std::process::Command;

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn dir(label: &str) -> PathBuf {
    let n = std::process::id();
    let dir = std::env::temp_dir().join(format!("mink_s99_{label}_{n}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Builds `source` with the real CLI and returns the executable path.
fn build(source: &str, label: &str) -> PathBuf {
    let d = dir(label);
    let path = d.join("main.mink");
    std::fs::write(&path, source).unwrap();
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

/// Runs the executable with `args` and piped `stdin`, returning
/// (exit code, stdout bytes, stderr bytes).
fn run(exe: &PathBuf, args: &[&str], stdin: &[u8]) -> (i32, Vec<u8>, Vec<u8>) {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    if !stdin.is_empty() {
        use std::io::Write;
        let mut child = cmd
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(stdin)
            .expect("write stdin");
        let out = child.wait_with_output().expect("wait");
        (out.status.code().unwrap_or(-1), out.stdout, out.stderr)
    } else {
        let out = cmd.output().expect("run");
        (out.status.code().unwrap_or(-1), out.stdout, out.stderr)
    }
}

fn body(src: &str) -> String {
    format!("fn main() {{\n{src}\n}}\n")
}

// ---------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------

#[test]
fn env_set_get_has_remove_round_trip() {
    let exe = build(
        &body(
            r#"
    let r = rt_env_set("MINK_S99_RT", "hello worldX");
    rt_print_int(r);
    if rt_env_has("MINK_S99_RT") { rt_print_int(1); } else { rt_print_int(0); }
    let v = rt_env_get("MINK_S99_RT");
    rt_print_int(rt_str_len(v));
    rt_print_str(v);
    rt_str_free(v);
    rt_print_int(rt_env_remove("MINK_S99_RT"));
    if rt_env_has("MINK_S99_RT") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
"#,
        ),
        "env_roundtrip",
    );
    let (code, out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0, "exit code");
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "0", "set result");
    assert_eq!(lines[1], "1", "has after set");
    assert_eq!(lines[2], "12", "value length");
    assert_eq!(lines[3], "hello worldX", "value content");
    assert_eq!(lines[4], "0", "remove result");
    assert_eq!(lines[5], "0", "has after remove");
}

#[test]
fn env_missing_variable_returns_owned_empty_string() {
    let exe = build(
        &body(
            r#"
    let v = rt_env_get("MINK_S99_NOPE_XYZ");
    rt_print_int(rt_str_len(v));
    rt_str_free(v);
    if rt_env_has("MINK_S99_NOPE_XYZ") { rt_print_int(1); } else { rt_print_int(0); }
    rt_exit(0);
"#,
        ),
        "env_missing",
    );
    let (code, out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0);
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "0", "missing value length is 0");
    assert_eq!(lines[1], "0", "has is false");
}

/// The Session 99 regression: the free-list layout that used to corrupt the
/// name CStr (rt_to_cstr's 8-byte NUL overrun) must set the variable
/// successfully. This is the exact repro from the checkpoint document.
#[test]
fn env_set_free_list_reuse_regression() {
    let exe = build(
        &body(
            r#"
    let a = rt_str_alloc(5);
    let b = rt_str_alloc(1900);
    rt_str_free(a);
    rt_str_free(b);
    let r1 = rt_env_set("MINK_S99_REGR", "hello worldX");
    rt_print_int(r1);
    // big->small free order variant
    let c = rt_str_alloc(5);
    let d = rt_str_alloc(1900);
    rt_str_free(d);
    rt_str_free(c);
    let r2 = rt_env_set("MINK_S99_REGR2", "another long value");
    rt_print_int(r2);
    // heap-built value after the same frees
    let e = rt_str_alloc(5);
    let f = rt_str_alloc(1900);
    rt_str_free(e);
    rt_str_free(f);
    let v = rt_str_concat("hel", "lo worldX");
    let r3 = rt_env_set("MINK_S99_REGR3", v);
    rt_print_int(r3);
    rt_str_free(v);
    // clean up all three
    rt_print_int(rt_env_remove("MINK_S99_REGR"));
    rt_print_int(rt_env_remove("MINK_S99_REGR2"));
    rt_print_int(rt_env_remove("MINK_S99_REGR3"));
    rt_exit(0);
"#,
        ),
        "env_freelist",
    );
    let (code, out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0, "exit code (no leak: all strings freed)");
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "0", "r1 set must succeed");
    assert_eq!(lines[1], "0", "r2 set must succeed");
    assert_eq!(lines[2], "0", "r3 set must succeed");
    assert_eq!(lines[3], "0", "remove r1");
    assert_eq!(lines[4], "0", "remove r2");
    assert_eq!(lines[5], "0", "remove r3");
}

#[test]
fn env_empty_and_spaced_values() {
    let exe = build(
        &body(
            r#"
    // set empty -> Windows semantics: an empty string value is a valid
    // value (SetEnvironmentVariableA only deletes on NULL, which the MINK
    // API has no way to express), so the variable exists with length 0.
    rt_print_int(rt_env_set("MINK_S99_EMPTY", ""));
    if rt_env_has("MINK_S99_EMPTY") { rt_print_int(1); } else { rt_print_int(0); }
    let e = rt_env_get("MINK_S99_EMPTY");
    rt_print_int(rt_str_len(e));
    rt_str_free(e);
    rt_print_int(rt_env_remove("MINK_S99_EMPTY"));
    if rt_env_has("MINK_S99_EMPTY") { rt_print_int(1); } else { rt_print_int(0); }
    // value with spaces round-trips exactly
    let rc = rt_env_set("MINK_S99_SPC", "two words  spaced");
    rt_print_int(rc);
    let v = rt_env_get("MINK_S99_SPC");
    rt_print_int(rt_str_len(v));
    rt_print_str(v);
    rt_str_free(v);
    rt_print_int(rt_env_remove("MINK_S99_SPC"));
    rt_exit(0);
"#,
        ),
        "env_spaced",
    );
    let (code, out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0);
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "0", "set empty succeeds");
    assert_eq!(lines[1], "1", "empty value keeps the variable present");
    assert_eq!(lines[2], "0", "empty value length");
    assert_eq!(lines[3], "0", "remove empty-valued variable");
    assert_eq!(lines[4], "0", "has after remove");
    assert_eq!(lines[5], "0", "spaced set succeeds");
    assert_eq!(lines[6], "17", "spaced value length");
    assert_eq!(lines[7], "two words  spaced", "spaced value content");
    assert_eq!(lines[8], "0", "remove");
}

// ---------------------------------------------------------------------------
// argv
// ---------------------------------------------------------------------------

#[test]
fn argv_preserves_count_order_spaces_and_empty() {
    let exe = build(
        &body(
            r#"
    rt_print_int(rt_argc());
    let a0 = rt_argv(0);
    rt_print_str(a0);
    rt_str_free(a0);
    let a1 = rt_argv(1);
    rt_print_int(rt_str_len(a1));
    rt_print_str(a1);
    rt_str_free(a1);
    let a2 = rt_argv(2);
    rt_print_int(rt_str_len(a2));
    rt_str_free(a2);
    rt_exit(0);
"#,
        ),
        "argv",
    );
    let (code, out, _err) = run(&exe, &["one", "two words", ""], b"");
    assert_eq!(code, 0, "exit code");
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "3", "argc");
    assert_eq!(lines[1], "one", "argv0");
    assert_eq!(lines[2], "9", "argv1 length");
    assert_eq!(lines[3], "two words", "argv1 content with space");
    assert_eq!(lines[4], "0", "argv2 empty length");
}

#[test]
fn argv_zero_arguments() {
    let exe = build(
        &body(
            r#"
    rt_print_int(rt_argc());
    rt_exit(0);
"#,
        ),
        "argv_zero",
    );
    let (code, out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0);
    assert_eq!(
        String::from_utf8_lossy(&out).trim(),
        "0",
        "argc with no args"
    );
}

// ---------------------------------------------------------------------------
// stdin / sleep / stderr
// ---------------------------------------------------------------------------

#[test]
fn stdin_reads_piped_input_to_eof() {
    let exe = build(
        &body(
            r#"
    let s = rt_stdin_read();
    rt_print_int(rt_str_len(s));
    rt_print_str(s);
    rt_str_free(s);
    rt_exit(0);
"#,
        ),
        "stdin",
    );
    let input = b"line one\nline two\n";
    let (code, out, _err) = run(&exe, &[], input);
    assert_eq!(code, 0);
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "18", "piped input length");
    // The string content itself contains bare LF bytes, so it survives
    // the \r\n split as one part.
    assert_eq!(lines[1], "line one\nline two\n", "exact input content");
}

#[test]
fn stdin_empty_read_returns_empty_string() {
    let exe = build(
        &body(
            r#"
    let s = rt_stdin_read();
    rt_print_int(rt_str_len(s));
    rt_str_free(s);
    rt_exit(0);
"#,
        ),
        "stdin_empty",
    );
    let (code, out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0);
    assert_eq!(
        String::from_utf8_lossy(&out).trim(),
        "0",
        "empty stdin length"
    );
}

#[test]
fn sleep_zero_and_short_returns_cleanly() {
    let exe = build(
        &body(
            r#"
    rt_sleep(0);
    rt_sleep(1);
    rt_exit(0);
"#,
        ),
        "sleep",
    );
    let (code, _out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0, "sleep must return cleanly");
}

#[test]
fn stderr_write_is_separate_from_stdout() {
    let exe = build(
        &body(
            r#"
    let n = rt_stderr_write("ERR1\n");
    rt_print_int(n);
    rt_print_str("OUT1");
    rt_exit(0);
"#,
        ),
        "stderr",
    );
    let (code, out, err) = run(&exe, &[], b"");
    assert_eq!(code, 0);
    // stdout carries the write count (5 bytes: "ERR1\n") and "OUT1";
    // stderr carries only the raw stderr bytes.
    let out_s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = out_s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "5", "stderr write byte count");
    assert_eq!(lines[1], "OUT1", "stdout content");
    let err_s = String::from_utf8_lossy(&err);
    assert_eq!(err_s, "ERR1\n", "stderr content exact");
    assert!(!err_s.contains("OUT1"), "stderr must not contain stdout");
}

// ---------------------------------------------------------------------------
// float -> Str
// ---------------------------------------------------------------------------

#[test]
fn str_from_float_matches_print_float() {
    let exe = build(
        &body(
            r#"
    let f1 = rt_str_from_float(0.0);
    rt_print_str(f1);
    rt_str_free(f1);
    let f2 = rt_str_from_float(-7.0);
    rt_print_str(f2);
    rt_str_free(f2);
    let f3 = rt_str_from_float(123456.0);
    rt_print_str(f3);
    rt_str_free(f3);
    let f4 = rt_str_from_float(2.5);
    rt_print_str(f4);
    rt_str_free(f4);
    rt_exit(0);
"#,
        ),
        "float",
    );
    let (code, out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0);
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "0");
    assert_eq!(lines[1], "-7");
    assert_eq!(lines[2], "123456");
    assert_eq!(lines[3], "2.5");
}

// ---------------------------------------------------------------------------
// str_format
// ---------------------------------------------------------------------------

#[test]
fn str_format_substitutes_escapes_and_drops_missing() {
    let exe = build(
        &body(
            r#"
    let s1 = rt_str_format("Hello {} and {}!", "A", "B", "CD");
    rt_print_str(s1);
    rt_str_free(s1);
    let s2 = rt_str_format("{{lit}} x{} done", "Z", "", "");
    rt_print_str(s2);
    rt_str_free(s2);
    let s3 = rt_str_format("missing: {} {} {}", "only", "", "");
    rt_print_str(s3);
    rt_str_free(s3);
    rt_exit(0);
"#,
        ),
        "format",
    );
    let (code, out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0);
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "Hello A and B!");
    assert_eq!(lines[1], "{lit} xZ done");
    // The format literally contains spaces between the placeholders, so
    // the two removed `{}` leave those spaces in the output.
    assert_eq!(lines[2], "missing: only  ");
}

#[test]
fn str_format_exact_length_and_ownership() {
    let exe = build(
        &body(
            r#"
    let s = rt_str_format("x{}y", "ABCDEFGH", "", "");
    rt_print_int(rt_str_len(s));
    rt_print_str(s);
    rt_str_free(s);
    rt_exit(0);
"#,
        ),
        "format_len",
    );
    let (code, out, _err) = run(&exe, &[], b"");
    assert_eq!(code, 0, "no leak: the result was freed");
    let s = String::from_utf8_lossy(&out);
    let lines: Vec<&str> = s.split_terminator("\r\n").collect();
    assert_eq!(lines[0], "10", "x + 8 + y");
    assert_eq!(lines[1], "xABCDEFGHy");
}
