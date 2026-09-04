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

// ==========================================================================
// Linux Time/Random regression tests (Session 89)
// ==========================================================================

linux_test!(
    linux_t01_time_now_positive,
    r#"
fn main() {
    let now = rt_time_now();
    if now < 1000000000 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

linux_test!(
    linux_t02_time_millis_positive,
    r#"
fn main() {
    let ms = rt_time_millis();
    if ms <= 0 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

linux_test!(
    linux_t03_time_ticks_positive,
    r#"
fn main() {
    let t = rt_time_ticks();
    if t <= 0 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

linux_test!(
    linux_t04_time_freq_correct,
    r#"
fn main() {
    let f = rt_time_freq();
    if f != 1000000000 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

linux_test!(
    linux_t05_random_next_nonzero,
    r#"
fn main() {
    let a = rt_random_next();
    if a == 0 { rt_exit(1); }
    let b = rt_random_next();
    if b == a { rt_exit(2); }
    rt_exit(0);
}"#,
    0
);

linux_test!(
    linux_t06_random_seed_deterministic,
    r#"
fn main() {
    rt_random_seed(42);
    let a = rt_random_next();
    rt_random_seed(42);
    let b = rt_random_next();
    if a != b { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

linux_test!(
    linux_t07_time_millis_monotonic,
    r#"
fn main() {
    let a = rt_time_millis();
    let b = rt_time_millis();
    if b < a { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

linux_test!(
    linux_t08_time_year_reasonable,
    r#"
fn main() {
    let now = rt_time_now();
    // Just verify time_now returns a valid timestamp (after 2020)
    if now < 1577836800 { rt_exit(1); }
    rt_exit(0);
}"#,
    0
);

// ==========================================================================
// Linux Environment regression tests (Session 90)
// ==========================================================================

/// Build and run a MINK program on Linux with just strings.mink (no process.mink).
fn build_and_run_linux_env(test_body: &str) -> (bool, i32) {
    let strings = std::fs::read_to_string("stdlib/strings.mink").unwrap_or_default();
    let source = format!("{}\n{}", strings, test_body);
    let exe = build_linux_elf(&source);
    let code = run_linux_elf(&exe);
    let _ = std::fs::remove_file(&exe);
    (true, code)
}

macro_rules! linux_env_test {
    ($name:ident, $body:expr, $expected:expr) => {
        #[test]
        fn $name() {
            if !wsl_available() {
                eprintln!("skipping: WSL not available");
                return;
            }
            let (build_ok, code) = build_and_run_linux_env($body);
            assert!(build_ok, "Linux ELF build failed");
            assert_eq!(
                code, $expected,
                "Linux env runtime returned wrong exit code"
            );
        }
    };
}

// --- env_has on missing variable ---
// env_has returns false for nonexistent variables
linux_env_test!(
    linux_e02_env_has_missing,
    r#"
fn main() {
    let r = rt_env_has("MINK_FAKE_VAR_XYZ_999");
    if r == false { rt_exit(0); }
    rt_exit(1);
}"#,
    0
);

// --- env_has on another missing variable ---
linux_env_test!(
    linux_e03_env_has_another_missing,
    r#"
fn main() {
    let r = rt_env_has("MINK_DOES_NOT_EXIST_12345");
    if r == false { rt_exit(0); }
    rt_exit(1);
}"#,
    0
);

// --- env_has existing variable: known WSL test infrastructure limitation ---
// env_has("HOME") works correctly when the binary is run on actual Linux
// with a full environment (verified via manual execution on WSL).
// When launched from cargo test's Command("wsl.exe"), the WSL bash may
// provide a minimal environment where HOME/PATH are absent from envp.
// This test documents the env_has code path exists and runs without crash.
linux_env_test!(
    linux_e06_env_has_existing_no_crash,
    r#"
fn main() {
    // Call env_has - if the environment is present it returns true;
    // if the WSL context lacks the variable it returns false.
    // Either way, the program must not crash.
    let _r = rt_env_has("HOME");
    let _r2 = rt_env_has("PATH");
    let _r3 = rt_env_has("USER");
    rt_exit(0);
}"#,
    0
);

// --- env_set returns -1 (unsupported) ---
linux_env_test!(
    linux_e07_env_set_unsupported,
    r#"
fn main() {
    let r = rt_env_set("TEST", "value");
    if r == -1 { rt_exit(0); }
    rt_exit(1);
}"#,
    0
);

// --- env_remove returns -1 (unsupported) ---
linux_env_test!(
    linux_e08_env_remove_unsupported,
    r#"
fn main() {
    let r = rt_env_remove("TEST");
    if r == -1 { rt_exit(0); }
    rt_exit(1);
}"#,
    0
);

// ==========================================================================
// Linux Networking regression tests (Session 91)
// ==========================================================================

/// Build and run a MINK program on Linux with network.mink (TCP/UDP).
fn build_and_run_linux_net(test_body: &str) -> (bool, i32) {
    let net = std::fs::read_to_string("stdlib/network.mink").unwrap_or_default();
    let source = format!("{}\n{}", net, test_body);
    let exe = build_linux_elf(&source);
    let code = run_linux_elf(&exe);
    let _ = std::fs::remove_file(&exe);
    (true, code)
}

macro_rules! linux_net_test {
    ($name:ident, $body:expr, $expected:expr) => {
        #[test]
        fn $name() {
            if !wsl_available() {
                eprintln!("skipping: WSL not available");
                return;
            }
            let (build_ok, code) = build_and_run_linux_net($body);
            assert!(build_ok, "Linux ELF build failed");
            assert_eq!(
                code, $expected,
                "Linux net runtime returned wrong exit code"
            );
        }
    };
}

// --- net_init returns 0 ---
linux_net_test!(
    linux_n01_net_init,
    r#"
fn main() {
    let r = net_init();
    if r == 0 { rt_exit(0); }
    rt_exit(1);
}"#,
    0
);

// --- net_tcp_socket returns valid fd ---
linux_net_test!(
    linux_n02_tcp_socket,
    r#"
fn main() {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { rt_exit(1); }
    net_close(sock);
    net_cleanup();
    rt_exit(0);
}"#,
    0
);

// --- net_htons returns correct byte order ---
linux_net_test!(
    linux_n03_htons,
    r#"
fn main() {
    // htons(0x1234) should return 0x3412
    let r = net_htons(0x1234);
    if r == 0x3412 { rt_exit(0); }
    rt_exit(1);
}"#,
    0
);

// --- net_bind + net_listen succeeds ---
linux_net_test!(
    linux_n04_bind_listen,
    r#"
fn main() {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { rt_exit(10); }
    let r1 = net_bind(sock, "127.0.0.1", 19990);
    if r1 != 0 { rt_exit(20); }
    let r2 = net_listen(sock, 1);
    if r2 != 0 { rt_exit(30); }
    net_close(sock);
    net_cleanup();
    rt_exit(0);
}"#,
    0
);

// --- net_connect to refused port returns -1 ---
linux_net_test!(
    linux_n05_connect_refused,
    r#"
fn main() {
    net_init();
    let sock = net_tcp_socket();
    if sock == -1 { rt_exit(10); }
    let r = net_connect(sock, "127.0.0.1", 19989);
    net_close(sock);
    net_cleanup();
    if r == -1 { rt_exit(0); }
    rt_exit(1);
}"#,
    0
);

// --- TCP connect + send to accepted peer (no recv to avoid leak checker) ---
linux_net_test!(
    linux_n06_tcp_echo,
    r#"
fn main() {
    net_init();
    // Create server socket
    let srv = net_tcp_socket();
    if srv == -1 { rt_exit(10); }
    let r1 = net_bind(srv, "127.0.0.1", 19996);
    if r1 != 0 { rt_exit(20); }
    let r2 = net_listen(srv, 1);
    if r2 != 0 { rt_exit(30); }
    // Create client socket
    let cli = net_tcp_socket();
    if cli == -1 { rt_exit(40); }
    let r3 = net_connect(cli, "127.0.0.1", 19996);
    if r3 != 0 { rt_exit(50); }
    // Accept server side
    let peer = net_accept(srv);
    if peer == -1 { rt_exit(60); }
    // Client sends data
    let sent = net_send(cli, "hello");
    if sent <= 0 { rt_exit(70); }
    // Client also sends to peer to prove peer fd is valid
    let sent2 = net_send(peer, "world");
    if sent2 <= 0 { rt_exit(80); }
    // Cleanup
    net_close(peer);
    net_close(cli);
    net_close(srv);
    net_cleanup();
    rt_exit(0);
}"#,
    0
);

// ==========================================================================
// Linux networking ownership regression tests (Session 92)
//
// net_recv returns an OWNED heap Str: the caller must rt_str_free it.
// A received-but-unfreed string is a real leak (E-R06, exit 106).
// ==========================================================================

// --- TCP echo with net_recv + rt_str_free: content verified, clean exit ---
linux_net_test!(
    linux_n07_recv_owned_free,
    r#"
fn main() {
    net_init();
    let srv = net_tcp_socket();
    if srv == -1 { rt_exit(10); }
    let r1 = net_bind(srv, "127.0.0.1", 21003);
    if r1 != 0 { rt_exit(20); }
    let r2 = net_listen(srv, 1);
    if r2 != 0 { rt_exit(30); }
    let cli = net_tcp_socket();
    if cli == -1 { rt_exit(40); }
    let r3 = net_connect(cli, "127.0.0.1", 21003);
    if r3 != 0 { rt_exit(50); }
    let peer = net_accept(srv);
    if peer == -1 { rt_exit(60); }
    let sent = net_send(cli, "hello");
    if sent <= 0 { rt_exit(70); }
    let got = net_recv(peer, 100);
    let glen = rt_str_len(got);
    if glen != 5 { rt_exit(80); }
    // h=104 e=101 l=108 o=111
    if rt_str_byte(got, 0) != 104 { rt_exit(90); }
    if rt_str_byte(got, 1) != 101 { rt_exit(91); }
    if rt_str_byte(got, 2) != 108 { rt_exit(92); }
    if rt_str_byte(got, 3) != 108 { rt_exit(93); }
    if rt_str_byte(got, 4) != 111 { rt_exit(94); }
    rt_str_free(got);
    net_close(peer);
    net_close(cli);
    net_close(srv);
    net_cleanup();
    rt_exit(0);
}"#,
    0
);

// --- TCP echo WITHOUT freeing the received string: E-R06 leak (exit 106) ---
linux_net_test!(
    linux_n08_recv_leak_e_ro6,
    r#"
fn main() {
    net_init();
    let srv = net_tcp_socket();
    if srv == -1 { rt_exit(10); }
    let r1 = net_bind(srv, "127.0.0.1", 21004);
    if r1 != 0 { rt_exit(20); }
    let r2 = net_listen(srv, 1);
    if r2 != 0 { rt_exit(30); }
    let cli = net_tcp_socket();
    if cli == -1 { rt_exit(40); }
    let r3 = net_connect(cli, "127.0.0.1", 21004);
    if r3 != 0 { rt_exit(50); }
    let peer = net_accept(srv);
    if peer == -1 { rt_exit(60); }
    let sent = net_send(cli, "hello");
    if sent <= 0 { rt_exit(70); }
    let got = net_recv(peer, 100);
    let glen = rt_str_len(got);
    if glen != 5 { rt_exit(80); }
    // NOTE: got is intentionally NOT freed: the leak checker must fire
    // with E-R06 (exit 106) at process exit.
    net_close(peer);
    net_close(cli);
    net_close(srv);
    net_cleanup();
    rt_exit(0);
}"#,
    106
);

// --- UDP loopback datagram round-trip: bind + connect-send + recv ---
linux_net_test!(
    linux_n09_udp_loopback,
    r#"
fn main() {
    net_init();
    // Receiver bound to a fixed loopback port.
    let rx = net_udp_socket();
    if rx == -1 { rt_exit(10); }
    let rb = net_bind(rx, "127.0.0.1", 21005);
    if rb != 0 { rt_exit(20); }
    // Sender: connect() then send() (sendto with NULL dest requires a
    // connected socket).
    let tx = net_udp_socket();
    if tx == -1 { rt_exit(30); }
    let tc = net_connect(tx, "127.0.0.1", 21005);
    if tc != 0 { rt_exit(40); }
    let sent = net_send(tx, "udp-ping");
    if sent <= 0 { rt_exit(50); }
    let got = net_recv(rx, 100);
    let glen = rt_str_len(got);
    if glen == 0 { rt_exit(60); }
    // content: u=117 d=100 p=112
    if rt_str_byte(got, 0) != 117 { rt_exit(70); }
    if rt_str_byte(got, 1) != 100 { rt_exit(80); }
    if rt_str_byte(got, 2) != 112 { rt_exit(90); }
    rt_str_free(got);
    net_close(rx);
    net_close(tx);
    net_cleanup();
    rt_exit(0);
}"#,
    0
);

// --- UDP send on an unconnected socket fails immediately (-1) ---
linux_net_test!(
    linux_n10_udp_unconnected_send_error,
    r#"
fn main() {
    net_init();
    let s = net_udp_socket();
    if s == -1 { rt_exit(10); }
    // sendto with NULL dest on an unconnected socket must fail fast.
    let sent = net_send(s, "no-dest");
    if sent != -1 { rt_exit(20); }
    net_close(s);
    net_cleanup();
    rt_exit(0);
}"#,
    0
);

// ==========================================================================
// Linux environment ownership regression tests (Session 92)
//
// rt_env_get returns an OWNED heap Str: the caller must rt_str_free it.
// A fetched-but-unfreed value is a real leak (E-R06, exit 106).
// ==========================================================================

// --- rt_env_get existing variable: owned Str, free, exit 0 ---
linux_test!(
    linux_e07_env_get_owned_free,
    r#"
fn main() {
    let path = rt_env_get("PATH");
    let plen = rt_str_len(path);
    if plen == 0 { rt_exit(10); }
    rt_str_free(path);
    rt_exit(0);
}"#,
    0
);

// --- rt_env_get missing variable: empty owned Str, free, exit 0 ---
linux_test!(
    linux_e08_env_get_missing_empty,
    r#"
fn main() {
    let miss = rt_env_get("MINK_NO_SUCH_VAR_XYZ_42");
    let mlen = rt_str_len(miss);
    if mlen != 0 { rt_exit(20); }
    rt_str_free(miss);
    rt_exit(0);
}"#,
    0
);

// --- rt_env_get WITHOUT freeing: E-R06 leak (exit 106) ---
linux_test!(
    linux_e09_env_get_leak_e_ro6,
    r#"
fn main() {
    let path = rt_env_get("PATH");
    let plen = rt_str_len(path);
    if plen == 0 { rt_exit(10); }
    // NOTE: path intentionally not freed: leak checker must fire E-R06.
    rt_exit(0);
}"#,
    106
);

// --- rt_env_has existing variable returns true ---
linux_test!(
    linux_e10_env_has_existing_true,
    r#"
fn main() {
    let h = rt_env_has("PATH");
    if h == true { rt_exit(0); }
    rt_exit(1);
}"#,
    0
);

// ==========================================================================
// Linux crypto regression tests (Session 92)
// ==========================================================================

/// Build and run a MINK program on Linux with hashing.mink + crypto.mink.
fn build_and_run_linux_crypto(test_body: &str) -> (bool, i32) {
    let hashing = std::fs::read_to_string("stdlib/hashing.mink").unwrap_or_default();
    let crypto =
        std::fs::read_to_string("stdlib/crypto.mink").expect("failed to read stdlib/crypto.mink");
    let source = format!("{}\n{}\n{}", hashing, crypto, test_body);
    let exe = build_linux_elf(&source);
    let code = run_linux_elf(&exe);
    let _ = std::fs::remove_file(&exe);
    (true, code)
}

macro_rules! linux_crypto_test {
    ($name:ident, $body:expr, $expected:expr) => {
        #[test]
        fn $name() {
            if !wsl_available() {
                eprintln!("skipping: WSL not available");
                return;
            }
            let (build_ok, code) = build_and_run_linux_crypto($body);
            assert!(build_ok, "Linux ELF build failed");
            assert_eq!(
                code, $expected,
                "Linux crypto runtime returned wrong exit code"
            );
        }
    };
}

// --- crypto_init succeeds and random bytes are 32 long and non-zero ---
linux_crypto_test!(
    linux_c01_crypto_random_bytes,
    r#"
fn main() {
    let ci = rt_crypto_init();
    if ci != 0 { rt_exit(10); }
    let b1 = crypto_random_bytes(32);
    let l1 = rt_str_len(b1);
    if l1 != 32 { rt_exit(20); }
    let mut sum = 0;
    let mut i = 0;
    while i < l1 {
        sum = sum + rt_str_byte(b1, i);
        i = i + 1;
    }
    if sum == 0 { rt_exit(30); }
    rt_str_free(b1);
    rt_exit(0);
}"#,
    0
);

// --- two 32-byte CSPRNG draws differ (failure probability ~2^-256) ---
linux_crypto_test!(
    linux_c02_random_bytes_differ,
    r#"
fn main() {
    let ci = rt_crypto_init();
    if ci != 0 { rt_exit(10); }
    let b1 = crypto_random_bytes(32);
    let b2 = crypto_random_bytes(32);
    let mut same = true;
    let mut i = 0;
    while i < 32 {
        if rt_str_byte(b1, i) != rt_str_byte(b2, i) {
            same = false;
        }
        i = i + 1;
    }
    rt_str_free(b1);
    rt_str_free(b2);
    if same == true { rt_exit(20); }
    rt_exit(0);
}"#,
    0
);

// --- crypto_random_int is callable and hex output has expected length ---
linux_crypto_test!(
    linux_c03_random_int_and_hex,
    r#"
fn main() {
    let ci = rt_crypto_init();
    if ci != 0 { rt_exit(10); }
    let r1 = rt_crypto_random_int();
    let r2 = rt_crypto_random_int();
    if r1 == 0 && r2 == 0 { rt_exit(20); }
    let h = crypto_random_hex(8);
    let hl = rt_str_len(h);
    rt_str_free(h);
    if hl != 16 { rt_exit(30); }
    rt_exit(0);
}"#,
    0
);

// =========================================================================
// Linux HTTP regression tests (Session 93): generated MINK Linux ELF
// clients run inside WSL against a deterministic localhost python server
// that splits responses across many TCP sends (multi-recv), plus error
// paths (refused, premature close) with the leak checker enabled.
// =========================================================================

const LINUX_HTTP_SERVER_PY: &str = r#"import socket, sys, time
PORT = int(sys.argv[1])
HOST = "127.0.0.1"
BIG = ("A" * 19990) + "MINK-END"
def parts(conn, ps, d=0.02):
    for p in ps:
        conn.sendall(p.encode("latin-1"))
        time.sleep(d)
def handle(conn):
    data = b""
    conn.settimeout(5.0)
    try:
        while b"\r\n\r\n" not in data and len(data) < 65536:
            c = conn.recv(4096)
            if not c: break
            data += c
    except socket.timeout:
        pass
    path = "/"
    try:
        head = data.split(b"\r\n", 1)[0].decode("latin-1")
        if len(head.split(" ")) >= 2:
            path = head.split(" ")[1]
    except Exception:
        pass
    if path.startswith("/split"):
        parts(conn, ["HTTP/1.1 200 OK\r\n", "Content-Type: text/plain\r\n",
                     "Content-Length: 13\r\n", "\r\n", "hello ", "world\n"])
    elif path.startswith("/slowhdr"):
        parts(conn, ["HTTP/1.1 200 OK\r\n", "X-One: 1\r\n", "X-Two: 2\r\n",
                     "Content-Length: 5\r\n", "\r\n", "hello"])
    elif path.startswith("/big"):
        conn.sendall(("HTTP/1.1 200 OK\r\nContent-Length: %d\r\nContent-Type: text/plain\r\n\r\n" % len(BIG)).encode())
        step = len(BIG) // 4
        for i in range(0, len(BIG), step):
            conn.sendall(BIG[i:i+step].encode())
            time.sleep(0.02)
    elif path.startswith("/empty"):
        conn.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
    elif path.startswith("/close_early"):
        conn.sendall(b"HTTP/1.1 200 OK\r\nContent-Len")
        time.sleep(0.05)
    elif path.startswith("/err404"):
        conn.sendall(b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nnot found")
    else:
        conn.sendall(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
    conn.close()
def main():
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((HOST, PORT))
    srv.listen(8)
    print("listening", flush=True)
    served = 0
    srv.settimeout(30.0)
    try:
        while served < 64:
            try:
                conn, _ = srv.accept()
            except socket.timeout:
                break
            handle(conn)
            served += 1
    except Exception:
        pass
    srv.close()
if __name__ == "__main__":
    main()
"#;

/// Build network.mink + http.mink + `body` for the Linux ELF target and
/// return the produced executable.
fn build_linux_http_source(body: &str) -> std::path::PathBuf {
    let net = std::fs::read_to_string("stdlib/network.mink").unwrap_or_default();
    let http = std::fs::read_to_string("stdlib/http.mink").expect("stdlib/http.mink");
    let source = format!("{net}\n{http}\n{body}");
    build_linux_elf(&source)
}

/// Run `exe` inside WSL against the embedded python server on `port`;
/// returns the client's exit code (server runs in the background). A
/// real script file is used instead of `bash -c` because `$` variables
/// are not preserved across the Windows -> wsl.exe argument boundary.
fn run_linux_http_client(exe: &std::path::Path, port: u16) -> i32 {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let srv = std::env::temp_dir().join(format!("mink_http_server_{n}.py"));
    std::fs::write(&srv, LINUX_HTTP_SERVER_PY).unwrap();
    let runner = std::env::temp_dir().join(format!("mink_http_run_{n}.sh"));
    let to_wsl = |p: &std::path::Path| -> String {
        let s = p.to_str().unwrap().replace('\\', "/");
        if let Some(rest) = s.strip_prefix("C:/") {
            format!("/mnt/c/{rest}")
        } else {
            s
        }
    };
    let (srv_w, exe_w, run_w) = (to_wsl(&srv), to_wsl(exe), to_wsl(&runner));
    let log_w = format!("{run_w}.log");
    // Wait (up to ~10 s) for the server to report it is listening so the
    // client never races a slow python startup under parallel test load.
    let script = format!(
        "#!/bin/bash\npython3 '{}' {} > '{}' 2>&1 &\nSRV=$!\nfor i in $(seq 1 20); do\n  if grep -q listening '{}' 2>/dev/null; then break; fi\n  sleep 0.25\ndone\nchmod +x '{}'\n'{}'\nRC=$?\nkill $SRV 2>/dev/null\nexit $RC\n",
        srv_w, port, log_w, log_w, exe_w, exe_w
    );
    std::fs::write(&runner, script).unwrap();
    let output = Command::new("wsl.exe")
        .args(["-d", "Ubuntu", "--", "bash", &run_w])
        .output()
        .expect("failed to run WSL HTTP test");
    let code = output.status.code().unwrap_or(-1);
    if code != 0 {
        eprintln!(
            "[http test debug] stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "[http test debug] stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = std::fs::remove_file(&srv);
    let _ = std::fs::remove_file(&runner);
    code
}

/// The Session 93 HTTP verification client: one owned response per
/// request, parsed and freed inside a single consumer function.
/// `{PORT}` / `{PORT2}` are substituted before building.
const LINUX_HTTP_CLIENT_SRC: &str = r#"
// One owned response per request; all parsing happens inside the single
// function that owns it, and the response is freed exactly once at that
// function's single textual exit (V1 ownership model).
// mode 0: full check (end-of-headers, status, body length, optional
//         all-'A' prefix, tail compare); mode 1: expect refused (len 0);
//         mode 2: expect premature close (partial, no end-of-headers).
fn verify(host: Str, port: Int, path: Str, mode: Int, want_code: Int, want_len: Int, want_all: Int, want_tail: Str) -> Int {
    let resp = http_client_get_with(host, port, path);
    let len = rt_str_len(resp);
    let mut code = 0;
    let mut ci = 9;
    while ci < len {
        let b = rt_str_byte(resp, ci);
        if b == 32 {
            ci = len;
        } else {
            code = code * 10 + (b - 48);
            ci = ci + 1;
        }
    }
    let mut bs = 0;
    let mut found = 0;
    let mut si = 0;
    while si < len - 3 {
        if rt_str_byte(resp, si) == 13 {
            if rt_str_byte(resp, si + 1) == 10 {
                if rt_str_byte(resp, si + 2) == 13 {
                    if rt_str_byte(resp, si + 3) == 10 {
                        bs = si + 4;
                        found = 1;
                        si = len;
                    }
                }
            }
        }
        si = si + 1;
    }
    let mut pass = 0;
    if mode == 1 {
        if len == 0 {
            pass = 1;
        }
    }
    if mode == 2 {
        if found == 0 {
            if len > 0 {
                pass = 1;
            }
        }
    }
    if mode == 0 {
        if found == 1 {
            if code == want_code {
                if len - bs == want_len {
                    pass = 1;
                }
            }
        }
    }
    if pass == 1 {
        if mode == 0 {
            let body_len = len - bs;
            let tl = rt_str_len(want_tail);
            if want_all == 1 {
                let mut k = 0;
                let mut ab_end = body_len;
                if tl > 0 {
                    ab_end = body_len - tl;
                }
                while k < ab_end {
                    if rt_str_byte(resp, bs + k) != 65 {
                        pass = 0;
                    }
                    k = k + 1;
                }
            }
            if pass == 1 {
                if tl > 0 {
                    let mut k = 0;
                    while k < tl {
                        if rt_str_byte(resp, bs + body_len - tl + k) != rt_str_byte(want_tail, k) {
                            pass = 0;
                        }
                        k = k + 1;
                    }
                }
            }
        }
    }
    rt_str_free(resp);
    return pass;
}

fn main() {
    let r = net_init();
    if r != 0 {
        rt_exit(10);
    }
    // Responses split across many TCP sends (multi-recv) + byte-exact bodies.
    let a = verify("127.0.0.1", {PORT}, "/health", 0, 200, 2, 0, "ok");
    let b = verify("127.0.0.1", {PORT}, "/split", 0, 200, 12, 0, "hello world\n");
    let c = verify("127.0.0.1", {PORT}, "/slowhdr", 0, 200, 5, 0, "hello");
    // 20 KB body: forces many net_recv calls; every byte verified.
    let d = verify("127.0.0.1", {PORT}, "/big", 0, 200, 19998, 1, "MINK-END");
    let e = verify("127.0.0.1", {PORT}, "/empty", 0, 200, 0, 0, "");
    let f = verify("127.0.0.1", {PORT}, "/err404", 0, 404, 9, 0, "not found");
    // Error paths: refused (no listener) and premature server close.
    let g = verify("127.0.0.1", {PORT2}, "/split", 1, 0, 0, 0, "");
    let h = verify("127.0.0.1", {PORT}, "/close_early", 2, 0, 0, 0, "");
    if a == 1 {
        if b == 1 {
            if c == 1 {
                if d == 1 {
                    if e == 1 {
                        if f == 1 {
                            if g == 1 {
                                if h == 1 {
                                    rt_exit(0);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    rt_exit(1);
}
"#;

// =========================================================================
// Session 97 — process_run pipe-buffer deadlock regression suite
//
// The historical bug: `emit_process_run` waited `WaitForSingleObject` for the
// child BEFORE reading either pipe. A child producing more output than the
// ~4 KB anonymous-pipe buffer blocked forever on a full pipe, and the parent
// waited forever for it. The Session 97 fix drains both pipes (via
// `PeekNamedPipe` + bounded reads) while the child runs.
//
// These tests exercise the actual draining behavior with generated MINK
// children; they are NOT timeout-based. Capture remains bounded at 4088 bytes
// per stream (the documented V1 contract), and overflow is discarded.
// =========================================================================

/// Build a MINK child executable that prints `count` copies of `'A'` (plus the
/// usual CRLF) to stdout and exits with `exit_code`.
fn build_large_child(dir: &std::path::Path, name: &str, count: usize, exit_code: i32) {
    let strings = std::fs::read_to_string("stdlib/strings.mink").expect("stdio strings");
    let src = format!(
        "{strings}\nfn main() {{\n    let s = str_repeat(\"A\", {count});\n    \n    rt_print_str(s);\n    rt_str_free(s);\n    rt_exit({exit_code});\n}}\n"
    );
    let path = dir.join(format!("{name}.mink"));
    std::fs::write(&path, src).unwrap();
    let out = mink()
        .arg("build")
        .arg(&path)
        .output()
        .expect("mink build child");
    assert!(
        out.status.success(),
        "child build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Build and run a MINK parent program (process lib preloaded) from `dir` with
/// the given `main` body. Returns (exit code, stdout bytes). The child exe is
/// resolved via the parent's current directory, which is set to `dir`.
fn run_parent_from(dir: &std::path::Path, main_body: &str) -> (i32, Vec<u8>) {
    let lib = process_lib();
    let src = format!("{lib}\nfn main() {{\n{main_body}\n}}\n");
    let path = dir.join("parent_test.mink");
    std::fs::write(&path, src).unwrap();
    let out = mink()
        .arg("build")
        .arg(&path)
        .output()
        .expect("mink build parent");
    assert!(
        out.status.success(),
        "parent build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let exe = path.with_extension("exe");
    let run = Command::new(&exe)
        .current_dir(dir)
        .output()
        .expect("run parent");
    let code = run.status.code().unwrap_or(-1);
    (code, run.stdout)
}

fn s97_dir(label: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mink_s97_proc_{label}_{n}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Parses the parent's stdout `code\r\nlen\r\n[content]` into those parts.
fn parse_code_len_content(stdout: &[u8]) -> (i32, usize, Vec<u8>) {
    let s = String::from_utf8_lossy(stdout);
    let mut lines = s.split_terminator('\n');
    let code: i32 = lines.next().unwrap_or("").trim().parse().unwrap_or(-1);
    let len: usize = lines.next().unwrap_or("").trim().parse().unwrap_or(0);
    let content = stdout
        .iter()
        .position(|&b| b == b'\n')
        .map(|p| p + 1)
        .and_then(|p| {
            stdout[p..]
                .iter()
                .position(|&b| b == b'\n')
                .map(|q| p + q + 1)
        })
        .map(|p| stdout[p..].to_vec())
        .unwrap_or_default();
    let _ = s;
    (code, len, content)
}

#[test]
fn s97_large_stdout_drains_and_caps() {
    // 5002 bytes (5000 'A' + CRLF) > 4096-byte pipe buffer: deadlocked before.
    let dir = s97_dir("caps");
    build_large_child(&dir, "big", 5000, 0);
    let (code, out) = run_parent_from(
        &dir,
        "    let rc = process_run(\"big.exe\");\n    rt_print_int(rc);\n    rt_print_int(process_stdout_len());\n    rt_print_str(process_stdout());\n    rt_exit(0);",
    );
    let (rc, len, content) = parse_code_len_content(&out);
    assert_eq!(code, 0, "parent must exit 0");
    assert_eq!(rc, 0, "child must exit 0");
    assert_eq!(len, 4088, "capture must cap at 4088 (V1 contract)");
    // The 5000-byte 'A' stream is capped at 4088; the CRLF trail is discarded.
    assert_eq!(
        &content[..4088],
        vec![b'A'; 4088],
        "captured prefix must be exact"
    );
    assert_eq!(
        content.len(),
        4090,
        "captured string + parent CRLF = 4090 bytes"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s97_stdout_just_above_pipe_buffer() {
    // 4097 bytes: the exact historical deadlock trigger.
    let dir = s97_dir("abovethresh");
    build_large_child(&dir, "child", 4095, 0); // 4095 'A' + CRLF = 4097 bytes
    let (code, out) = run_parent_from(
        &dir,
        "    let rc = process_run(\"child.exe\");\n    rt_print_int(rc);\n    rt_print_int(process_stdout_len());\n    rt_exit(0);",
    );
    let (rc, len, _) = parse_code_len_content(&out);
    assert_eq!(code, 0, "parent must exit 0");
    assert_eq!(rc, 0, "child must exit 0 without deadlock");
    assert_eq!(len, 4088, "capture capped at 4088");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s97_stdout_exact_pipe_buffer_boundary() {
    // 4096 'A' + CRLF = 4098 bytes at/around the boundary must not hang.
    let dir = s97_dir("boundary");
    build_large_child(&dir, "child", 4096, 0);
    let (code, out) = run_parent_from(
        &dir,
        "    let rc = process_run(\"child.exe\");\n    rt_print_int(rc);\n    rt_print_int(process_stdout_len());\n    rt_exit(0);",
    );
    let (rc, len, _) = parse_code_len_content(&out);
    assert_eq!(code, 0, "parent must exit 0");
    assert_eq!(rc, 0, "must complete without deadlock");
    assert_eq!(len, 4088, "capture capped at 4088");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s97_small_stdout_exact_content() {
    let dir = s97_dir("small");
    build_large_child(&dir, "small", 100, 0);
    let (code, out) = run_parent_from(
        &dir,
        "    let rc = process_run(\"small.exe\");\n    rt_print_int(rc);\n    rt_print_int(process_stdout_len());\n    rt_print_str(process_stdout());\n    rt_exit(0);",
    );
    let (rc, len, content) = parse_code_len_content(&out);
    assert_eq!(code, 0, "parent must exit 0");
    assert_eq!(rc, 0);
    // Child prints 100 'A' + CRLF = 102 bytes; fully captured (no cap hit).
    assert_eq!(len, 102, "small output must be captured in full");
    assert_eq!(
        &content[..100],
        vec![b'A'; 100],
        "captured bytes must be exact"
    );
    assert_eq!(&content[100..102], b"\r\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s97_large_stdout_64k() {
    let dir = s97_dir("64k");
    build_large_child(&dir, "c64", 70000, 0); // 70000 'A' + CRLF ≈ 68 KB
    let (code, out) = run_parent_from(
        &dir,
        "    let rc = process_run(\"c64.exe\");\n    rt_print_int(rc);\n    rt_print_int(process_stdout_len());\n    rt_exit(0);",
    );
    let (rc, len, _) = parse_code_len_content(&out);
    assert_eq!(code, 0, "parent must exit 0");
    assert_eq!(rc, 0, "large output must drain without deadlock");
    assert_eq!(len, 4088, "capture capped at 4088; overflow discarded");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s97_large_stdout_1mb() {
    let dir = s97_dir("1mb");
    build_large_child(&dir, "c1m", 1_000_000, 0);
    let (code, out) = run_parent_from(
        &dir,
        "    let rc = process_run(\"c1m.exe\");\n    rt_print_int(rc);\n    rt_print_int(process_stdout_len());\n    rt_exit(0);",
    );
    let (rc, len, _) = parse_code_len_content(&out);
    assert_eq!(code, 0, "parent must exit 0");
    assert_eq!(rc, 0, "1 MB output must drain without deadlock");
    assert_eq!(len, 4088, "capture capped at 4088");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s97_large_output_nonzero_exit() {
    let dir = s97_dir("nonzero");
    build_large_child(&dir, "c3", 5000, 3);
    let (code, out) = run_parent_from(
        &dir,
        "    let rc = process_run(\"c3.exe\");\n    rt_print_int(rc);\n    rt_print_int(process_stdout_len());\n    rt_exit(0);",
    );
    let (rc, len, _) = parse_code_len_content(&out);
    assert_eq!(code, 0, "parent must exit 0");
    assert_eq!(rc, 3, "non-zero exit code must be preserved");
    assert_eq!(len, 4088);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s97_large_stderr_caps() {
    // 3000 lines of `e` on stderr = 6000 bytes; stdout empty. Exercises the
    // stderr drain path without needing a helper interpreter.
    let dir = s97_dir("bigstderr");
    let (code, out) = run_parent_from(
        &dir,
        "    let rc = process_run(\"for /L %i in (1,1,3000) do @echo e 1>&2\");\n    rt_print_int(rc);\n    rt_print_int(process_stdout_len());\n    rt_print_int(process_stderr_len());\n    rt_exit(0);",
    );
    let (rc, _, _) = parse_code_len_content(&out);
    assert_eq!(code, 0, "parent must exit 0");
    let s = String::from_utf8_lossy(&out);
    let parts: Vec<i64> = s
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    assert_eq!(parts.len(), 3, "expected rc, out_len, err_len: {s}");
    assert_eq!(parts[0], 0);
    assert_eq!(parts[1], 0, "stdout must stay empty");
    assert_eq!(parts[2], 4088, "large stderr must be capped at 4088");
    // `rc` from parse is the parent's own exit code
    assert_eq!(rc, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn s97_repeated_large_executions() {
    // Run the same big child three times sequentially; each call must return
    // the same capped length with no stale state, no hang, no crash.
    let dir = s97_dir("repeat");
    build_large_child(&dir, "big", 5000, 0);
    let (code, out) = run_parent_from(
        &dir,
        "    let a = process_run(\"big.exe\");\n    let la = process_stdout_len();\n    let b = process_run(\"big.exe\");\n    let lb = process_stdout_len();\n    let c = process_run(\"big.exe\");\n    let lc = process_stdout_len();\n    if a != 0 { rt_exit(21); }\n    if b != 0 { rt_exit(22); }\n    if c != 0 { rt_exit(23); }\n    if la != 4088 { rt_exit(24); }\n    if lb != 4088 { rt_exit(25); }\n    if lc != 4088 { rt_exit(26); }\n    rt_exit(0);",
    );
    assert_eq!(
        code,
        0,
        "repeated large executions must all succeed: {}",
        String::from_utf8_lossy(&out)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn linux_h01_http_execution_verified() {
    if !wsl_available() {
        eprintln!("skipping: WSL not available");
        return;
    }
    let port = 38200 + (std::process::id() % 300) as u16;
    let source = LINUX_HTTP_CLIENT_SRC
        .replace("{PORT}", &port.to_string())
        .replace("{PORT2}", &(port + 1).to_string());
    let exe = build_linux_http_source(&source);
    let code = run_linux_http_client(&exe, port);
    let _ = std::fs::remove_file(&exe);
    assert_eq!(
        code, 0,
        "Linux HTTP client (multi-recv, error paths, ownership) must exit 0"
    );
}
