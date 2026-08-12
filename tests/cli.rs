//! Integration tests for the `mink` compiler executable: process entry,
//! command dispatch, and exit codes.

use std::path::PathBuf;
use std::process::Command;

/// Returns a `Command` for the compiled `mink` binary.
fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

/// Writes `content` to a uniquely named temp file and returns its path.
fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mink_cli_test_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn version_flag_prints_version_and_succeeds() {
    let output = mink().arg("--version").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout.trim(), format!("mink {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn version_command_prints_version_and_succeeds() {
    let output = mink().arg("version").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert_eq!(stdout.trim(), format!("mink {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_flag_lists_commands_and_succeeds() {
    let output = mink().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert_eq!(output.status.code(), Some(0));
    for command in ["build", "check", "run", "test", "fmt", "version"] {
        assert!(stdout.contains(command), "help should mention '{command}'");
    }
}

#[test]
fn no_arguments_prints_help_and_succeeds() {
    let output = mink().output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage"));
}

#[test]
fn build_with_missing_file_fails_with_io_error() {
    let missing = std::env::temp_dir().join(format!(
        "mink_cli_test_{}_does_not_exist.mink",
        std::process::id()
    ));
    // Guard against a stale file from a previous interrupted run.
    let _ = std::fs::remove_file(&missing);
    let output = mink().arg("build").arg(&missing).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to read"), "stderr was: {stderr}");
}

#[test]
fn build_with_valid_source_reports_not_implemented() {
    let path = temp_source("valid.mink", "fn main() {}\n");
    let output = mink().arg("build").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    // The file loads, so the failure is the unimplemented pipeline, not an
    // I/O error.
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not yet implemented"),
        "stderr was: {stderr}"
    );
}

#[test]
fn recognized_commands_report_not_implemented() {
    for command in ["run", "test", "fmt"] {
        let output = mink().arg(command).output().unwrap();
        assert_eq!(output.status.code(), Some(2), "for command '{command}'");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("not yet implemented"),
            "for command '{command}': {stderr}"
        );
    }
}

#[test]
fn unknown_command_fails_cleanly() {
    let output = mink().arg("frobnicate").output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown command"), "stderr was: {stderr}");
}

#[test]
fn build_without_path_reports_usage_error() {
    let output = mink().arg("build").output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing path"), "stderr was: {stderr}");
}

#[test]
fn check_with_valid_source_passes() {
    // Note: the filename is unique to this test; the shared helper writes to
    // a per-process temp dir and parallel tests must not reuse names.
    let path = temp_source("check_valid.mink", "fn main() {}\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("passed parsing (6 tokens)"),
        "stdout was: {stdout}"
    );
}

#[test]
fn check_with_empty_source_passes() {
    let path = temp_source("empty.mink", "");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn check_with_invalid_lexical_source_fails() {
    let path = temp_source("bad.mink", "let x = \"unterminated\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unterminated string literal"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("-->"),
        "stderr should include a location: {stderr}"
    );
}

#[test]
fn check_with_invalid_syntax_fails_with_parser_error() {
    let path = temp_source("bad_syntax.mink", "fn main {}\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E-P08"),
        "stderr should include the parser error code: {stderr}"
    );
    assert!(stderr.contains("expected '('"), "stderr was: {stderr}");
    assert!(
        stderr.contains("-->"),
        "stderr should include a location: {stderr}"
    );
}

#[test]
fn check_with_multiple_syntax_errors_reports_all() {
    let path = temp_source("many_syntax_errors.mink", "let x = ; let y = ;\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr.matches("E-P03").count(),
        2,
        "both independent errors should be reported: {stderr}"
    );
}

#[test]
fn check_reports_lexical_and_syntax_errors_together() {
    // `@` is a lexical error (no token); the unterminated `let` declaration
    // is a syntax error. Both must be reported in one run.
    let path = temp_source("mixed_errors.mink", "@ let x = 1");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E-L01"),
        "stderr should include the lexical error code: {stderr}"
    );
    assert!(
        stderr.contains("E-P06"),
        "stderr should include the syntax error code: {stderr}"
    );
}

#[test]
fn check_with_representative_program_passes() {
    let path = temp_source(
        "representative.mink",
        "fn main() {\n    let x = 1 + 2 * 3;\n    if x > 0 {\n        return x;\n    } else {\n        return 0;\n    }\n}\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("passed parsing"), "stdout was: {stdout}");
}

#[test]
fn check_with_missing_file_fails() {
    let missing = std::env::temp_dir().join(format!(
        "mink_cli_test_{}_missing_check.mink",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&missing);
    let output = mink().arg("check").arg(&missing).output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to read"), "stderr was: {stderr}");
}

#[test]
fn check_without_path_reports_usage_error() {
    let output = mink().arg("check").output().unwrap();

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing path"), "stderr was: {stderr}");
}

#[test]
fn check_with_excluded_declaration_fails() {
    // `struct` is a reserved keyword but deliberately excluded from the
    // frozen grammar; the parser must reject it, not silently accept it.
    let path = temp_source("excluded_decl.mink", "struct Point {}\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-P01"), "stderr was: {stderr}");
    assert!(
        stderr.contains("expected a top-level declaration"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_excluded_construct_inside_function_fails() {
    // A closure is excluded from the frozen grammar; it must be rejected
    // inside a function body too.
    let path = temp_source("excluded_stmt.mink", "fn f() { let g = |x| x; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-P03"), "stderr was: {stderr}");
}

#[test]
fn check_recovery_does_not_cascade() {
    // One malformed for-loop header must produce exactly one diagnostic;
    // recovery must not emit cascades from the same root cause.
    let path = temp_source("no_cascade.mink", "fn f() { for x 0..10 { } }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("E-P12").count(), 1, "stderr was: {stderr}");
}

#[test]
fn check_with_unicode_source_passes() {
    // Unicode inside string literals and comments must parse with correct
    // byte spans and a successful exit.
    let path = temp_source(
        "unicode_ok.mink",
        "fn main() { /* 世界 */ let s = \"héllo 世界\"; }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn check_with_precedence_matrix_passes() {
    // Every precedence level and associativity form in the frozen grammar
    // must parse cleanly.
    let path = temp_source(
        "precedence.mink",
        concat!(
            "fn f() {\n",
            "    let a = 1 + 2 * 3;\n",
            "    let b = 1 << 2 + 3;\n",
            "    let c = x == y < z;\n",
            "    let d = p && q || r;\n",
            "    let e = m | n ^ o & p;\n",
            "    let f = a + b == c && d;\n",
            "    let g = a = b = c;\n",
            "    let h = 0 .. 10;\n",
            "    let i = foo(1).member[0](x);\n",
            "}\n",
        ),
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("passed parsing"), "stdout was: {stdout}");
}

#[test]
fn check_with_nested_constructs_passes() {
    let path = temp_source(
        "nested.mink",
        concat!(
            "fn main() {\n",
            "    for i in 0..10 {\n",
            "        while i > 0 {\n",
            "            loop {\n",
            "                if i == 3 { break; }\n",
            "                continue;\n",
            "            }\n",
            "        }\n",
            "    }\n",
            "    return;\n",
            "}\n",
        ),
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn check_with_missing_closer_at_eof_fails() {
    let path = temp_source("unclosed.mink", "fn f() { g(1,\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-P13"), "stderr was: {stderr}");
}
