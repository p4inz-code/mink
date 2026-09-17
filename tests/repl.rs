//! Interactive REPL integration tests — Windows Python parity R01/T04.
//!
//! Each test spawns the real `mink repl` CLI, feeds a script on stdin, and
//! asserts on the compile-eval session transcript. The REPL builds and runs
//! genuine Windows PE executables through `driver::build`, so these tests
//! exercise the real user-facing path (compiler + runtime + CLI), never a
//! test-only shortcut.

use std::io::Write;
use std::process::{Command, Stdio};

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

/// Runs `mink repl` with `script` piped to stdin. Returns (exit code, stdout,
/// stderr).
fn repl(script: &str) -> (i32, String, String) {
    let mut child = mink()
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn mink repl");
    {
        let mut stdin = child.stdin.take().expect("repl stdin");
        stdin
            .write_all(script.as_bytes())
            .expect("write repl script");
        // Dropping `stdin` closes the pipe, which delivers EOF to the REPL.
    }
    let output = child.wait_with_output().expect("repl output");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn repl_with_file(path: &std::path::Path, script: &str) -> (i32, String, String) {
    let mut child = mink()
        .arg("repl")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn mink repl");
    {
        let mut stdin = child.stdin.take().expect("repl stdin");
        stdin
            .write_all(script.as_bytes())
            .expect("write repl script");
    }
    let output = child.wait_with_output().expect("repl output");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn r01_repl_starts_and_eof_exits_cleanly() {
    let (code, stdout, stderr) = repl("");
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("interactive"), "missing banner: {stdout:?}");
    assert!(stderr.is_empty(), "unexpected stderr: {stderr:?}");
}

#[test]
fn r02_repl_bare_int_expression_prints_value() {
    let (code, stdout, stderr) = repl("40 + 2\n");
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains("42"), "stdout={stdout:?}");
}

#[test]
fn r03_repl_declaration_then_call() {
    let script = "fn double(x: Int) -> Int {\n    return x * 2;\n}\ndouble(21)\n";
    let (code, stdout, stderr) = repl(script);
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains("42"), "stdout={stdout:?}");
}

#[test]
fn r04_repl_string_expression_prints_text() {
    let (code, stdout, stderr) = repl("\"hello repl\"\n");
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains("hello repl"), "stdout={stdout:?}");
}

#[test]
fn r05_repl_multiline_declaration() {
    let script =
        "fn add(a: Int, b: Int) -> Int {\n    let c = a + b;\n    return c;\n}\nadd(20, 22)\n";
    let (code, stdout, stderr) = repl(script);
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains("42"), "stdout={stdout:?}");
    assert!(
        stdout.contains("..."),
        "expected continuation prompt: {stdout:?}"
    );
}

#[test]
fn r06_repl_compile_error_continues_session() {
    let script = "fn f() -> Int { return 4242; }\nthis is not valid mink\nf()\n";
    let (code, stdout, stderr) = repl(script);
    assert_eq!(code, 0, "a compile error must not kill the session");
    assert!(
        stderr.contains("mink: error["),
        "expected compile error on stderr: {stderr:?}"
    );
    assert!(
        stdout.contains("4242"),
        "session did not continue: {stdout:?}"
    );
}

#[test]
fn r07_repl_help_lists_commands() {
    let (code, stdout, stderr) = repl(":help\n");
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains(":quit"), "stdout={stdout:?}");
    assert!(stdout.contains(":clear"), "stdout={stdout:?}");
}

#[test]
fn r08_repl_clear_resets_session() {
    let script = "fn f() -> Int { return 1; }\n:clear\nf()\n";
    let (code, stdout, stderr) = repl(script);
    assert_eq!(code, 0);
    assert!(
        stderr.contains("mink: error["),
        "call after :clear should fail to compile: {stderr:?}"
    );
    let _ = stdout;
}

#[test]
fn r09_repl_show_reports_empty_session() {
    let (code, stdout, stderr) = repl(":show\n");
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(
        stdout.contains("(no session declarations)"),
        "stdout={stdout:?}"
    );
}

#[test]
fn r10_repl_quit_exits_zero() {
    let (code, stdout, stderr) = repl(":quit\n");
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
}

#[test]
fn r11_repl_statement_with_semicolon() {
    let (code, stdout, stderr) = repl("rt_print_int(7);\n");
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains('7'), "stdout={stdout:?}");
}

#[test]
fn r12_repl_unknown_command_is_reported_but_survives() {
    let (code, stdout, stderr) = repl(":bogus\n40 + 2\n");
    assert_eq!(code, 0);
    assert!(stderr.contains("unknown command"), "stderr={stderr:?}");
    assert!(stdout.contains("42"), "stdout={stdout:?}");
}

#[test]
fn r13_repl_loads_initial_file() {
    let path = std::env::temp_dir().join(format!("mink_repl_initial_{}.mink", std::process::id()));
    std::fs::write(
        &path,
        "fn triple(x: Int) -> Int {\n    return x * 3;\n}\nfn main() {\n    return 0;\n}\n",
    )
    .unwrap();
    let (code, stdout, stderr) = repl_with_file(&path, "triple(14)\n");
    let _ = std::fs::remove_file(&path);
    assert_eq!(code, 0, "stderr={stderr}");
    assert!(stdout.contains("42"), "stdout={stdout:?}");
}

#[test]
fn r14_repl_repeated_evaluation_is_stable() {
    let mut script = String::from("fn sq(x: Int) -> Int { return x * x; }\n");
    for _ in 0..10 {
        script.push_str("sq(6)\n");
    }
    let (code, stdout, stderr) = repl(&script);
    assert_eq!(code, 0, "stderr={stderr}");
    let hits = stdout.matches("36").count();
    assert_eq!(hits, 10, "expected 10 evaluations, got {hits}: {stdout:?}");
}
