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
        stdout.contains("passed parsing, semantic analysis, and type checking (6 tokens)"),
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
    // must parse cleanly — and, since `mink check` now runs semantic
    // analysis and type checking, its identifiers must resolve and its
    // operators must be well-typed (each level uses type-valid operands).
    let path = temp_source(
        "precedence.mink",
        concat!(
            "fn foo(v) { v; }\n",
            "fn f() {\n",
            "    let x = 1; let y = 2; let z = 3;\n",
            "    let mut a = 1 + 2 * 3;\n",
            "    let mut b = 1 << 2 + 3;\n",
            "    let c = x == y;\n",
            "    let d = true && false || true;\n",
            "    let e = 1 | 2 ^ 3 & 4;\n",
            "    let f = a + b == 5 && d;\n",
            "    let mut g = 0;\n",
            "    g = a = b = 5;\n",
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

// ---------------------------------------------------------------------------
// Semantic analysis diagnostics
// ---------------------------------------------------------------------------

#[test]
fn check_with_valid_semantic_program_passes() {
    // A program whose names all resolve, with valid mutable assignment and
    // control-flow context, passes with exit 0.
    let path = temp_source(
        "sem_valid.mink",
        "let base = 1;\nfn f() {\n    let mut x = base;\n    x = 2;\n    loop { break; }\n    return;\n}\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("passed parsing, semantic analysis, and type checking"),
        "stdout was: {stdout}"
    );
}

#[test]
fn check_with_unresolved_identifier_fails() {
    let path = temp_source("sem_unresolved.mink", "fn f() { missing; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-S01"), "stderr was: {stderr}");
    assert!(
        stderr.contains("cannot find name `missing` in this scope"),
        "stderr was: {stderr}"
    );
    assert!(stderr.contains("-->"), "stderr was: {stderr}");
}

#[test]
fn check_with_duplicate_declaration_reports_original() {
    let path = temp_source("sem_duplicate.mink", "let x = 1;\nlet x = 2;\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-S02"), "stderr was: {stderr}");
    assert!(
        stderr.contains("duplicate definition of `x`"),
        "stderr was: {stderr}"
    );
    // The original declaration location is reported as a note.
    assert!(
        stderr.contains("note: previous declaration is here"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_immutable_assignment_fails() {
    let path = temp_source("sem_immutable.mink", "fn f() { let x = 1; x = 2; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-S03"), "stderr was: {stderr}");
    assert!(
        stderr.contains("cannot assign to `x`: it is not mutable"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_const_assignment_fails() {
    let path = temp_source("sem_const_assign.mink", "const x = 1;\nfn f() { x = 2; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-S04"), "stderr was: {stderr}");
    assert!(
        stderr.contains("cannot assign to `x`: it is a constant"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_break_outside_loop_fails() {
    let path = temp_source("sem_break.mink", "fn f() { break; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-S05"), "stderr was: {stderr}");
    assert!(
        stderr.contains("`break` outside of a loop"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_continue_outside_loop_fails() {
    let path = temp_source("sem_continue.mink", "fn f() { continue; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-S06"), "stderr was: {stderr}");
    assert!(
        stderr.contains("`continue` outside of a loop"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_multiple_semantic_errors_reports_all() {
    let path = temp_source("sem_many.mink", "fn f() { alpha; beta; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("E-S01").count(), 2, "stderr was: {stderr}");
}

#[test]
fn check_with_return_at_module_scope_fails_as_syntax() {
    // The frozen grammar only allows declarations at module scope, so a
    // module-level `return;` is rejected by the parser (exit 1) — the
    // `return`-outside-function rule is enforced by the grammar itself.
    let path = temp_source("sem_return_module.mink", "return;\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-P01"), "stderr was: {stderr}");
}

#[test]
fn check_skips_semantics_when_parse_fails() {
    // A syntax error suppresses semantic analysis: the `break;` in the same
    // source must not produce a cascading E-S05.
    let path = temp_source("sem_parse_skip.mink", "fn f() { break; let x = ; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-P03"), "stderr was: {stderr}");
    assert!(!stderr.contains("E-S05"), "stderr was: {stderr}");
}

#[test]
fn check_with_lexical_and_semantic_sources_distinguish_stages() {
    // A lexical error skips semantics entirely; a semantically invalid but
    // lexically valid source reports a semantic code.
    let lex_path = temp_source("sem_lex_skip.mink", "@ let x = 1;\n");
    let lex_output = mink().arg("check").arg(&lex_path).output().unwrap();
    let _ = std::fs::remove_file(&lex_path);
    assert_eq!(lex_output.status.code(), Some(1));
    let lex_stderr = String::from_utf8_lossy(&lex_output.stderr);
    assert!(!lex_stderr.contains("E-S"), "stderr was: {lex_stderr}");

    let sem_path = temp_source("sem_lex_valid.mink", "fn f() { unknown; }\n");
    let sem_output = mink().arg("check").arg(&sem_path).output().unwrap();
    let _ = std::fs::remove_file(&sem_path);
    assert_eq!(sem_output.status.code(), Some(1));
    let sem_stderr = String::from_utf8_lossy(&sem_output.stderr);
    assert!(sem_stderr.contains("E-S01"), "stderr was: {sem_stderr}");
}

// ---------------------------------------------------------------------------
// Type-analysis diagnostics
// ---------------------------------------------------------------------------

#[test]
fn check_with_typed_valid_program_passes() {
    // A well-typed program exercising literals, operators, assignment,
    // ranges, calls, and typed returns passes with exit 0.
    let path = temp_source(
        "type_valid.mink",
        concat!(
            "fn add(a, b) { return a + b; }\n",
            "fn main() {\n",
            "    let x = 1 + 2 * 3;\n",
            "    let mut y = x;\n",
            "    y = add(x, 1);\n",
            "    if y > 0 { return; }\n",
            "    for i in 0..10 { let z = i; z; }\n",
            "    let b = true && false;\n",
            "    let r = 0 .. 10;\n",
            "    r;\n",
            "}\n",
        ),
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn check_with_invalid_operator_types_fails() {
    let path = temp_source(
        "type_operator.mink",
        "fn f() { let x = 1; let y = x + true; }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T02"), "stderr was: {stderr}");
    assert!(
        stderr.contains("cannot apply operator `+` to types `Int` and `Bool`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_incompatible_assignment_fails() {
    let path = temp_source("type_assign.mink", "fn f() { let mut x = 1; x = true; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T01"), "stderr was: {stderr}");
    assert!(
        stderr.contains("expected `Int`, found `Bool`"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("related location is here"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_invalid_call_fails() {
    let path = temp_source("type_call.mink", "fn f() { let x = 1; x(2); }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T04"), "stderr was: {stderr}");
    assert!(
        stderr.contains("cannot call a value of type `Int`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_incorrect_arity_fails() {
    let path = temp_source("type_arity.mink", "fn f(p) {} fn g() { f(1, 2); }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T05"), "stderr was: {stderr}");
    assert!(
        stderr.contains("expected `1` arguments, found `2`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_invalid_comparison_fails() {
    let path = temp_source(
        "type_compare.mink",
        "fn f() { let x = 1; let y = x < true; }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T02"), "stderr was: {stderr}");
    assert!(
        stderr.contains("cannot apply operator `<` to types `Int` and `Bool`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_invalid_logical_operation_fails() {
    let path = temp_source(
        "type_logical.mink",
        "fn f() { let x = 1; let y = x && true; }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T02"), "stderr was: {stderr}");
}

#[test]
fn check_with_invalid_condition_fails() {
    let path = temp_source("type_condition.mink", "fn f() { if 1 { } }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T01"), "stderr was: {stderr}");
    assert!(
        stderr.contains("expected `Bool`, found `Int`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_invalid_range_fails() {
    let path = temp_source("type_range.mink", "fn f() { let r = 0 .. \"a\"; }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T03"), "stderr was: {stderr}");
    assert!(
        stderr.contains("cannot construct a range with operands of types `Int` and `Str`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_non_range_iterable_fails() {
    let path = temp_source("type_iter.mink", "fn f() { for i in 1 { } }\n");
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T06"), "stderr was: {stderr}");
    assert!(
        stderr.contains("cannot iterate over a value of type `Int`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_multiple_type_errors_reports_all() {
    let path = temp_source(
        "type_many.mink",
        "fn f() { let mut a = 1; a = true; let mut b = 2; b = \"s\"; }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.matches("E-T01").count(), 2, "stderr was: {stderr}");
}

#[test]
fn check_with_inference_program_passes() {
    // Recursion, argument-driven inference, and return inference all
    // resolve; the program is well-typed and passes.
    let path = temp_source(
        "infer_ok.mink",
        "fn f(n) { if n > 0 { return f(n - 1); } return 0; }\n\
         fn g() { let x = f(3); x + 1; }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "stderr was: {stderr}");
}

#[test]
fn check_with_incompatible_call_constraints_fails() {
    // The body pins the parameter to `Int`; the second call site passes a
    // `Float`, which conflicts with the inferred signature.
    let path = temp_source(
        "infer_call.mink",
        "fn f(p) { p + 1; } fn g() { f(1); f(1.5); }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T01"), "stderr was: {stderr}");
    assert!(
        stderr.contains("expected `Int`, found `Float`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_conflicting_returns_fails() {
    let path = temp_source(
        "infer_return.mink",
        "fn f(c) { if c { return 1; } return 1.5; }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T01"), "stderr was: {stderr}");
    assert!(
        stderr.contains("expected `Int`, found `Float`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_with_pinned_condition_conflict_fails() {
    // The condition pins the function result to `Bool`; using it as a
    // number afterwards is a genuine operator error.
    let path = temp_source(
        "infer_cond.mink",
        "fn f() { return; } fn g() { if f() { } f() + 1; }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-T02"), "stderr was: {stderr}");
    assert!(
        stderr.contains("cannot apply operator `+` to types `Bool` and `Int`"),
        "stderr was: {stderr}"
    );
}

#[test]
fn check_reports_semantic_and_type_errors_together() {
    // An unresolved name (semantic) and an incompatible assignment (type)
    // in the same source are both reported; the unresolved name does not
    // cascade into type noise.
    let path = temp_source(
        "type_sem_mixed.mink",
        "fn f() { missing; let mut x = 1; x = true; }\n",
    );
    let output = mink().arg("check").arg(&path).output().unwrap();
    let _ = std::fs::remove_file(&path);

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-S01"), "stderr was: {stderr}");
    assert!(stderr.contains("E-T01"), "stderr was: {stderr}");
    // Exactly one of each: no cascade from the unresolved name.
    assert_eq!(stderr.matches("E-S01").count(), 1, "stderr was: {stderr}");
    assert_eq!(stderr.matches("E-T01").count(), 1, "stderr was: {stderr}");
}
