//! Environment library integration tests - Session 182.
//!
//! Session 178-181 recorded `mod environment;` as unloadable: the module
//! contained top-level `extern fn` declarations, which the parser rejects with
//! E-P01. Session 182 replaced them with wrappers over the predeclared
//! intrinsics. These tests build a real program that loads the module and
//! exercises the wrappers, including the ownership of the returned strings.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_env_test_{n}_{name}.mink"));
    std::fs::write(&path, content.replace("\r\n", "\n")).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, String, String) {
    let path = temp_source("test", test_body);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    let build_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let build_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = std::fs::remove_file(&path);
    if !output.status.success() {
        let _ = std::fs::remove_file(&exe);
        return (-1, build_stdout, build_stderr);
    }
    let run = Command::new(&exe).output().expect("failed to run test exe");
    let code = run.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let _ = std::fs::remove_file(&exe);
    (code, stdout, stderr)
}

// The original failure: `mod environment;` alone did not build. This test
// loads the module and calls every wrapper on the success path. Before the
// repair it fails at build time with E-P01 in stdlib/environment.mink.
#[test]
fn environment_module_loads_and_wraps_intrinsics() {
    let (code, out, err) = build_and_run(
        r#"
mod environment;

fn main() -> Int {
    env_set("MINK_M6_S182_ENV", "s182-value");
    if !env_has("MINK_M6_S182_ENV") { return 1; }
    let value = env_get("MINK_M6_S182_ENV");
    rt_print_str(value);
    rt_str_free(value);
    env_remove("MINK_M6_S182_ENV");
    if env_has("MINK_M6_S182_ENV") { return 2; }
    return 0;
}"#,
    );
    assert_eq!(code, 0, "exit={code}, stdout={out:?}, stderr={err:?}");
    assert!(
        out.contains("s182-value"),
        "expected the value set through the module: {out:?}"
    );
    assert!(err.is_empty(), "unexpected stderr: {err:?}");
}

// A missing variable must come back as an owned empty string, not a null or
// a runtime error, so the caller can free it unconditionally.
#[test]
fn environment_missing_value_is_owned_empty_string() {
    let (code, out, err) = build_and_run(
        r#"
mod environment;

fn main() -> Int {
    let missing = env_get("MINK_M6_S182_DEFINITELY_UNSET");
    let len = rt_str_len(missing);
    rt_str_free(missing);
    rt_print_int(len);
    let cwd = env_cwd();
    let cwd_len = rt_str_len(cwd);
    rt_str_free(cwd);
    if cwd_len == 0 { return 2; }
    return 0;
}"#,
    );
    assert_eq!(code, 0, "exit={code}, stdout={out:?}, stderr={err:?}");
    assert_eq!(out.trim(), "0", "missing value length: {out:?}");
    assert!(err.is_empty(), "unexpected stderr: {err:?}");
}

// M6-RC1: the wrappers consumed their `Str` parameters but only the temporary
// C-string copies were released, so every call given a heap string (anything
// built with `rt_str_concat` and friends) leaked one allocation and tripped
// E-R06 at exit. String literals hid the defect: they are not heap objects.
// Each argument below is built at run time, and exit 0 requires the leak
// checker to see a balanced allocator.
#[test]
fn environment_wrappers_release_heap_string_arguments() {
    let (code, out, err) = build_and_run(
        r#"
mod environment;

fn main() -> Int {
    if env_has(rt_str_concat("MINK_M6_RC_", "HEAP_NAME")) { return 1; }

    let set_ok = env_set(
        rt_str_concat("MINK_M6_RC_", "HEAP_NAME"),
        rt_str_concat("heap-", "value")
    );
    if set_ok != 0 { return 2; }

    if !env_has(rt_str_concat("MINK_M6_RC_", "HEAP_NAME")) { return 3; }

    let value = env_get(rt_str_concat("MINK_M6_RC_", "HEAP_NAME"));
    rt_print_str(value);
    rt_str_free(value);

    if env_remove(rt_str_concat("MINK_M6_RC_", "HEAP_NAME")) != 0 { return 4; }
    if env_has(rt_str_concat("MINK_M6_RC_", "HEAP_NAME")) { return 5; }
    return 0;
}"#,
    );
    assert_eq!(code, 0, "exit={code}, stdout={out:?}, stderr={err:?}");
    assert!(
        out.contains("heap-value"),
        "expected the value set through the module: {out:?}"
    );
    assert!(err.is_empty(), "unexpected stderr (leak?): {err:?}");
}
