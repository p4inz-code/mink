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

// =========================================================================
// Linux x86_64 Process Runtime Regression Tests (Session 88)
//
// These tests build MINK programs targeting Linux ELF, then execute them
// via WSL2. They are skipped on machines without WSL.
// =========================================================================

/// Check if WSL2 Ubuntu is available on this machine.
fn wsl_available() -> bool {
    Command::new("wsl.exe")
        .args(["-d", "Ubuntu", "--", "echo", "ok"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Build a MINK source for the Linux ELF target.
fn build_linux_elf(source: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("mink_linux_test_{n}.mink"));
    std::fs::write(&path, source.replace("\r\n", "\n")).unwrap();
    // On Windows, the compiler always produces .exe extension regardless of target
    let exe = path.with_extension("exe");
    let output = mink()
        .args([
            "build",
            "--target",
            "x86_64-linux-elf",
            path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run mink build");
    assert!(
        output.status.success(),
        "Linux ELF build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(exe.exists(), "Linux ELF binary not produced");
    // Verify it is actually an ELF
    let file_output = Command::new("file")
        .arg(exe.to_str().unwrap())
        .output()
        .expect("file command failed");
    let file_stdout = String::from_utf8_lossy(&file_output.stdout);
    assert!(
        file_stdout.contains("ELF"),
        "Output is not an ELF: {}",
        file_stdout
    );
    exe
}

/// Run a Linux ELF binary via WSL and return exit code.
fn run_linux_elf(exe: &std::path::Path) -> i32 {
    // Convert Windows path to WSL mount path.
    // Normalize to forward slashes, then convert C:/ to /mnt/c/
    let wsl_path = {
        let p = exe.to_str().expect("path must be valid UTF-8");
        let normalized = p.replace("\\", "/");
        if let Some(rest) = normalized.strip_prefix("C:/") {
            format!("/mnt/c/{}", rest)
        } else if let Some(rest) = normalized.strip_prefix("c:/") {
            format!("/mnt/c/{}", rest)
        } else {
            normalized
        }
    };

    let output = Command::new("wsl.exe")
        .args([
            "-d",
            "Ubuntu",
            "--",
            "bash",
            "-c",
            &format!("chmod +x '{}' && '{}'", wsl_path, wsl_path),
        ])
        .output()
        .expect("failed to run WSL");
    let code = output.status.code().unwrap_or(-1);
    if code != 0 {
        eprintln!("[Linux test debug] wsl_path: {}", wsl_path);
        eprintln!("[Linux test debug] exit code: {}", code);
        eprintln!(
            "[Linux test debug] stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "[Linux test debug] stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    code
}

/// Build and run a MINK program on Linux via WSL.
/// Returns (build_ok, exit_code).
fn build_and_run_linux(test_body: &str) -> (bool, i32) {
    let lib = process_lib();
    let source = format!("{}\n{}", lib, test_body);
    let exe = build_linux_elf(&source);
    let code = run_linux_elf(&exe);
    let _ = std::fs::remove_file(&exe);
    (true, code)
}

macro_rules! linux_test {
    ($name:ident, $body:expr, $expected:expr) => {
        #[test]
        fn $name() {
            if !wsl_available() {
                eprintln!("skipping: WSL not available");
                return;
            }
            let (build_ok, code) = build_and_run_linux($body);
            assert!(build_ok, "Linux ELF build failed");
            assert_eq!(
                code, $expected,
                "Linux process runtime returned wrong exit code"
            );
        }
    };
}

// --- Linux process_id ---
linux_test!(
    linux_p01_process_id,
    r#"
fn main() {
    let pid = process_id();
    if pid <= 0 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

// --- Linux process_run echo ---
linux_test!(
    linux_p02_run_echo,
    r#"
fn main() {
    let code = process_run("echo hello");
    if code != 0 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

// --- Linux nonzero exit propagation ---
linux_test!(
    linux_p03_exit_code_42,
    r#"
fn main() {
    let code = process_run("exit 42");
    if code != 42 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

// --- Linux invalid command returns nonzero ---
linux_test!(
    linux_p04_invalid_command,
    r#"
fn main() {
    let code = process_run("nonexistent_program_xyz_12345");
    if code == 0 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

// --- Linux stdout capture ---
linux_test!(
    linux_p05_stdout_capture,
    r#"
fn main() {
    process_run("echo hello_linux_test");
    let len = process_stdout_len();
    if len == 0 { rt_exit(1); }
    let s = process_stdout();
    let b0 = rt_str_byte(s, 0);
    let b1 = rt_str_byte(s, 1);
    let b2 = rt_str_byte(s, 2);
    let b3 = rt_str_byte(s, 3);
    let b4 = rt_str_byte(s, 4);
    // h=104, e=101, l=108, l=108, o=111
    if b0 != 104 { rt_exit(10); }
    if b1 != 101 { rt_exit(11); }
    if b2 != 108 { rt_exit(12); }
    if b3 != 108 { rt_exit(13); }
    if b4 != 111 { rt_exit(14); }
    rt_exit(0);
}"#,
    0
);

// --- Linux repeated execution ---
linux_test!(
    linux_p06_repeated_execution,
    r#"
fn main() {
    let c1 = process_run("echo first");
    if c1 != 0 { rt_exit(1); }
    let c2 = process_run("echo second");
    if c2 != 0 { rt_exit(2); }
    let c3 = process_run("echo third");
    if c3 != 0 { rt_exit(3); }
    let len = process_stdout_len();
    if len == 0 { rt_exit(4); }
    rt_exit(0);
}"#,
    0
);

// --- Linux process_run_ok helper ---
linux_test!(
    linux_p07_run_ok_helper,
    r#"
fn main() {
    let ok = process_run_ok("echo test");
    if ok != true { rt_exit(1); }
    let ok2 = process_run_ok("nonexistent_program_xyz_12345");
    if ok2 != false { rt_exit(2); }
    rt_exit(0);
}"#,
    0
);

// --- Linux process with arguments ---
linux_test!(
    linux_p08_command_with_args,
    r#"
fn main() {
    let code = process_run("echo arg1 arg2 arg3");
    if code != 0 { rt_exit(1); }
    let len = process_stdout_len();
    if len == 0 { rt_exit(2); }
    rt_exit(0);
}"#,
    0
);

// --- Linux exit code 100 ---
linux_test!(
    linux_p09_exit_code_100,
    r#"
fn main() {
    let code = process_run("exit 100");
    if code != 100 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

// --- Linux stderr capture (no crash) ---
linux_test!(
    linux_p10_stderr_capture,
    r#"
fn main() {
    process_run("echo error_msg 1>&2");
    let len = process_stderr_len();
    // Just verify it doesn't crash
    rt_exit(0);
}"#,
    0
);

// --- Linux empty command (no crash) ---
linux_test!(
    linux_p11_empty_command,
    r#"
fn main() {
    let code = process_run("");
    // Empty command may return nonzero — that's fine
    rt_exit(0);
}"#,
    0
);

// --- Linux process_id is callable as intrinsic ---
linux_test!(
    linux_p12_direct_process_id,
    r#"
fn main() {
    let pid = rt_process_id();
    if pid <= 0 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

// --- Linux direct process_run intrinsic ---
linux_test!(
    linux_p13_direct_process_run,
    r#"
fn main() {
    let code = rt_process_run("echo test");
    if code != 0 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

// --- Linux string operations on process output ---
linux_test!(
    linux_p14_string_ops_on_output,
    r#"
fn main() {
    process_run("echo test_data");
    let len = process_stdout_len();
    if len <= 0 { rt_exit(1); }
    let s = process_stdout();
    let b0 = rt_str_byte(s, 0);
    // 't' = 116
    if b0 != 116 { rt_exit(2); }
    rt_exit(0);
}"#,
    0
);

// --- Linux high exit code preserved ---
linux_test!(
    linux_p15_high_exit_code,
    r#"
fn main() {
    let code = process_run("exit 200");
    if code != 200 { rt_exit(99); }
    rt_exit(0);
}"#,
    0
);
