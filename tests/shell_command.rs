//! `mink shell <run|check|build> <path>`: the command the Explorer right-click
//! entries invoke. It runs the named command and then waits for Enter so the
//! console window stays readable. These tests pipe standard input, so no
//! prompt appears and the command returns the plain exit code.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn scratch_file(label: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mink_shell_{label}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("prog.mink");
    std::fs::write(&path, source).unwrap();
    path
}

fn run_with_closed_stdin(args: &[&str]) -> (i32, String, String) {
    let mut child = mink()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdin.take().map(|mut stdin| stdin.flush()));
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn shell_run_prints_a_header_runs_the_program_and_returns_its_code() {
    let path = scratch_file("run", "fn main() {\n    rt_print_str(\"hello\\n\");\n}\n");
    let (code, stdout, _) = run_with_closed_stdin(&["shell", "run", path.to_str().unwrap()]);
    assert_eq!(code, 0, "stdout: {stdout}");
    assert!(stdout.starts_with("MINK run: "), "got: {stdout}");
    assert!(stdout.contains("hello"), "the program's output is shown: {stdout}");
}

#[test]
fn shell_check_reports_errors_and_fails() {
    let path = scratch_file("bad", "fn main() {\n    let x = ;\n}\n");
    let (code, stdout, stderr) = run_with_closed_stdin(&["shell", "check", path.to_str().unwrap()]);
    assert_ne!(code, 0);
    assert!(stdout.starts_with("MINK check: "), "got: {stdout}");
    assert!(stderr.contains("error["), "diagnostics reach the console: {stderr}");
}

#[test]
fn shell_build_writes_the_executable_beside_the_source() {
    let path = scratch_file("build", "fn main() {\n}\n");
    let (code, _, _) = run_with_closed_stdin(&["shell", "build", path.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(path.with_extension("exe").exists());
}

#[test]
fn shell_rejects_unknown_verbs_and_missing_arguments() {
    let (code, _, stderr) = run_with_closed_stdin(&["shell", "format", "x.mink"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("unknown shell command 'format'"), "got: {stderr}");

    let (code, _, stderr) = run_with_closed_stdin(&["shell", "run"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("missing path argument"), "got: {stderr}");

    let (code, _, stderr) = run_with_closed_stdin(&["shell"]);
    assert_ne!(code, 0);
    assert!(stderr.contains("missing command for 'shell'"), "got: {stderr}");
}
