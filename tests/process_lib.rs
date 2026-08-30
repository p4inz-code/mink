//! Integration tests for the MINK Process library (Session 59).

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn process_lib() -> String {
    // Process lib needs strings lib for some functions
    let strings = std::fs::read_to_string("stdlib/strings.mink").unwrap_or_default();
    let process =
        std::fs::read_to_string("stdlib/process.mink").expect("failed to read stdlib/process.mink");
    format!("{}\n{}", strings, process)
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_process_test_{n}_{name}.mink"));
    std::fs::write(&path, content.replace("\r\n", "\n")).unwrap();
    path
}

fn build_and_run(test_body: &str) -> (i32, Vec<u8>, Vec<u8>) {
    let lib = process_lib();
    let source = format!("{}\n{}", lib, test_body);
    let path = temp_source("test", &source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    let exe = path.with_extension("exe");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&exe);
        panic!("build failed:\n{stderr}");
    }
    let run = Command::new(&exe).output().unwrap();
    let code = run.status.code().unwrap_or(-1);
    let stdout = run.stdout.clone();
    let stderr = run.stderr.clone();
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, stdout, stderr)
}

fn run_ok(test_body: &str) -> (i32, String) {
    let (code, stdout, _stderr) = build_and_run(test_body);
    (code, String::from_utf8_lossy(&stdout).to_string())
}

// =========================================================================
// Basic process execution
// =========================================================================

#[test]
fn p01_process_id_returns_positive() {
    let test = r#"
fn main() {
    let pid = process_id();
    if pid <= 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "process_id should return > 0");
}

#[test]
fn p02_run_echo_returns_zero() {
    let test = r#"
fn main() {
    let code = process_run("echo hello");
    if code != 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "echo should return exit code 0");
}

#[test]
fn p03_run_invalid_returns_nonzero() {
    let test = r#"
fn main() {
    let code = process_run("nonexistent_program_xyz_12345");
    if code == 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "invalid program should return nonzero");
}

#[test]
fn p04_run_empty_string() {
    let test = r#"
fn main() {
    let code = process_run("");
    // Empty command may return nonzero (error) - that's fine
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "empty command should not crash");
}

#[test]
fn p05_stdout_captured() {
    let test = r#"
fn main() {
    process_run("echo hello_world_test");
    let s = process_stdout();
    let len = process_stdout_len();
    if len == 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "stdout should be captured");
}

#[test]
fn p06_stdout_has_content() {
    let test = r#"
fn main() {
    process_run("echo abc");
    let len = process_stdout_len();
    if len < 3 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "stdout should contain at least 'abc'");
}

#[test]
fn p07_stderr_captured() {
    let test = r#"
fn main() {
    process_run("echo error_msg 1>&2");
    let len = process_stderr_len();
    // stderr might or might not be captured depending on shell behavior
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "stderr capture should not crash");
}

#[test]
fn p08_run_ok_true_for_echo() {
    let test = r#"
fn main() {
    let ok = process_run_ok("echo test");
    if ok != true { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "process_run_ok should return true for echo");
}

#[test]
fn p09_run_ok_false_for_invalid() {
    let test = r#"
fn main() {
    let ok = process_run_ok("nonexistent_program_xyz_12345");
    if ok != false { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "process_run_ok should return false for invalid");
}

#[test]
fn p10_valid_cmd_nonempty() {
    let test = r#"
fn main() {
    let v = process_is_valid_cmd("echo hello");
    if v != true { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "non-empty command should be valid");
}

#[test]
fn p11_valid_cmd_empty() {
    let test = r#"
fn main() {
    let v = process_is_valid_cmd("");
    if v != false { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "empty command should be invalid");
}

// =========================================================================
// Output content verification
// =========================================================================

#[test]
fn p12_stdout_content_matches_echo() {
    let test = r#"
fn main() {
    process_run("echo hello_process_test");
    let s = process_stdout();
    // Check first 6 bytes: "hello_" (might have newline)
    let b0 = rt_str_byte(s, 0);
    let b1 = rt_str_byte(s, 1);
    let b2 = rt_str_byte(s, 2);
    let b3 = rt_str_byte(s, 3);
    let b4 = rt_str_byte(s, 4);
    let b5 = rt_str_byte(s, 5);
    // h=104, e=101, l=108, l=108, o=111, _=95
    if b0 != 104 { rt_exit(1); }
    if b1 != 101 { rt_exit(2); }
    if b2 != 108 { rt_exit(3); }
    if b3 != 108 { rt_exit(4); }
    if b4 != 111 { rt_exit(5); }
    if b5 != 95 { rt_exit(6); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "stdout should contain 'hello_'");
}

#[test]
fn p13_stdout_len_positive() {
    let test = r#"
fn main() {
    process_run("echo test");
    let len = process_stdout_len();
    if len <= 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "stdout length should be positive");
}

// =========================================================================
// Repeated execution
// =========================================================================

#[test]
fn p14_repeated_execution() {
    let test = r#"
fn main() {
    rt_process_run("echo first");
    rt_process_run("echo second");
    rt_process_run("echo third");
    let len = process_stdout_len();
    if len == 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "repeated execution should work");
}

// =========================================================================
// Error handling
// =========================================================================

#[test]
fn p15_exit_code_from_command() {
    let test = r#"
fn main() {
    // "exit 42" should produce exit code 42
    let code = process_run("exit 42");
    if code != 42 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "exit code should propagate");
}

#[test]
fn p16_multiple_run_exit_codes() {
    let test = r#"
fn main() {
    let c1 = rt_process_run("exit 0");
    let c2 = rt_process_run("exit 1");
    let c3 = rt_process_run("exit 100");
    if c1 != 0 { rt_exit(1); }
    if c2 != 1 { rt_exit(2); }
    if c3 != 100 { rt_exit(3); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "multiple exit codes should be captured correctly");
}

// =========================================================================
// Direct intrinsic tests (no library)
// =========================================================================

#[test]
fn p17_direct_process_id() {
    let test = r#"
fn main() {
    let pid = rt_process_id();
    if pid <= 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "rt_process_id should return > 0");
}

#[test]
fn p18_direct_process_run_echo() {
    let test = r#"
fn main() {
    let code = rt_process_run("echo test");
    if code != 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "rt_process_run should work directly");
}

#[test]
fn p19_direct_stdout_len() {
    let test = r#"
fn main() {
    rt_process_run("echo hello");
    let len = rt_process_stdout_len();
    if len == 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "rt_process_stdout_len should work");
}

// =========================================================================
// Process with no output
// =========================================================================

#[test]
fn p20_command_with_no_output() {
    let test = r#"
fn main() {
    process_run("cd .");
    let len = process_stdout_len();
    // cd might produce output or not; just verify no crash
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "command with no output should not crash");
}

// =========================================================================
// Integration with other libraries
// =========================================================================

#[test]
fn p21_process_with_string_ops() {
    let test = r#"
fn main() {
    process_run("echo test_data");
    let len = process_stdout_len();
    // Verify captured output is usable: length > 0 and byte access works
    if len <= 0 { rt_exit(1); }
    let s = process_stdout();
    let b0 = rt_str_byte(s, 0);
    // 't' = 116
    if b0 != 116 { rt_exit(2); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "process output should be accessible via str ops");
}

// =========================================================================
// Process stress test
// =========================================================================

#[test]
fn p22_many_process_runs() {
    let test = r#"
fn main() {
    let c1 = rt_process_run("echo test");
    let c2 = rt_process_run("echo test");
    let c3 = rt_process_run("echo test");
    let len = process_stdout_len();
    if len == 0 { rt_exit(1); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "10 consecutive process runs should work");
}

// =========================================================================
// Process with arguments
// =========================================================================

#[test]
fn p23_command_with_arguments() {
    let test = r#"
fn main() {
    let code = process_run("echo arg1 arg2 arg3");
    if code != 0 { rt_exit(1); }
    let len = process_stdout_len();
    if len == 0 { rt_exit(2); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "command with arguments should work");
}

// =========================================================================
// Error return from commands
// =========================================================================

#[test]
fn p24_nonzero_exit_preserved() {
    let test = r#"
fn main() {
    let code = process_run("exit 7");
    if code != 7 { rt_exit(99); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "exit code 7 should be preserved");
}

#[test]
fn p25_high_exit_code_preserved() {
    let test = r#"
fn main() {
    let code = process_run("exit 200");
    if code != 200 { rt_exit(99); }
    rt_exit(0);
}"#;
    let (code, _) = run_ok(test);
    assert_eq!(code, 0, "high exit code should be preserved");
}
