//! Compiler robustness regressions (Session 277, M7 Phase A).
//!
//! Two defects found while auditing the compilation pipeline for M7.1:
//!
//! 1. **Deep input exhausted the compiler's stack.** The parser is recursive
//!    descent and the later passes walk the tree recursively, so a program with
//!    a few hundred nested parentheses — or a few hundred terms in one
//!    arithmetic chain — crashed the *release* compiler with
//!    `thread 'main' has overflowed its stack` and exit status 127 instead of
//!    reporting an error. `MAX_NESTING` / `MAX_EXPR_CHAIN` in `src/parser`
//!    now bound both and report `E-P33`, and the CLI runs on a 64 MiB worker
//!    stack.
//!
//! 2. **A literal wrapped in parentheses could not be compiled.** `mink check`
//!    passed `let x = (1);` and `mink build` failed with `E-B10: could not
//!    decode a literal`, because HIR lowering eliminated the syntax-only
//!    `Group` node but kept the parentheses' span, and the backend re-reads a
//!    literal's text from its span. The lowered node now keeps the inner span.
//!
//! Every test runs the real CLI binary as a subprocess, so it exercises the
//! shipped stack size, exit codes and diagnostics.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "mink_robust_{}_{}_{name}.mink",
        std::process::id(),
        n
    ));
    std::fs::write(&path, content).unwrap();
    path
}

/// Runs `mink check` and returns (exit code, stderr).
fn check(name: &str, source: &str) -> (i32, String) {
    let path = temp_source(name, source);
    let output = mink().arg("check").arg(&path).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = std::fs::remove_file(&path);
    (code, stderr)
}

/// Builds and runs a program, returning (exit code, stdout).
fn build_and_run(name: &str, source: &str) -> (i32, String) {
    let path = temp_source(name, source);
    let exe = path.with_extension("exe");
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(exe.exists(), "no executable produced");
    let run = Command::new(&exe).output().unwrap();
    let code = run.status.code().unwrap_or(-1);
    // Normalize CRLF so assertions compare program output, not the platform
    // line ending.
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&exe);
    (code, stdout)
}

// ===========================================================================
// Deep input: controlled diagnostics instead of a stack overflow
// ===========================================================================

#[test]
fn deeply_nested_parentheses_compile() {
    // 200 nested groups used to abort the release compiler (the crash
    // threshold was between 100 and 300) instead of compiling.
    let src = format!(
        "fn main() {{ let x = {}1{}; rt_print_int(x); return 0; }}",
        "(".repeat(200),
        ")".repeat(200)
    );
    let (code, stdout) = build_and_run("deep_paren_ok", &src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), "1");
}

#[test]
fn nesting_beyond_the_limit_is_one_diagnostic_not_a_crash() {
    let src = format!(
        "fn main() {{ let x = {}1{}; return 0; }}",
        "(".repeat(1_000),
        ")".repeat(1_000)
    );
    let (code, stderr) = check("deep_paren_bad", &src);
    assert_eq!(code, 1, "expected a diagnostic exit, got {code}: {stderr}");
    assert!(
        stderr.contains("E-P33"),
        "expected E-P33 for over-deep nesting, got:\n{stderr}"
    );
    assert!(
        !stderr.contains("overflowed its stack"),
        "the compiler crashed instead of diagnosing:\n{stderr}"
    );
    // One root cause, one diagnostic: the guard reports once per file.
    assert_eq!(
        stderr.matches("E-P33").count(),
        1,
        "expected a single nesting diagnostic, got:\n{stderr}"
    );
}

#[test]
fn deeply_nested_blocks_are_bounded() {
    // 1,000 nested blocks overflowed the stack; now they are diagnosed.
    let src = format!(
        "fn main() {{ {}{}{} return 0; }}",
        "{".repeat(1_000),
        "rt_print_int(1);",
        "}".repeat(1_000)
    );
    let (code, stderr) = check("deep_block", &src);
    assert_eq!(code, 1);
    assert!(stderr.contains("E-P33"), "got:\n{stderr}");
}

#[test]
fn deeply_nested_types_are_bounded() {
    let src = format!(
        "fn main() {{ let x: {}Int{} = null; return 0; }}",
        "Ptr<".repeat(1_000),
        ">".repeat(1_000)
    );
    let (code, stderr) = check("deep_type", &src);
    assert_eq!(code, 1);
    assert!(stderr.contains("E-P33"), "got:\n{stderr}");
}

#[test]
fn long_flat_expression_chain_compiles_and_runs() {
    // A 1,000-term chain used to overflow the stack during a later pass (the
    // parser builds it iteratively, but the tree is 1,000 nodes deep).
    let src = format!(
        "fn main() {{ let x = 1{}; rt_print_int(x); return 0; }}",
        " + 1".repeat(1_000)
    );
    let (code, stdout) = build_and_run("long_chain_ok", &src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), "1001");
}

#[test]
fn over_long_expression_chain_is_bound_and_emits_no_executable() {
    let src = format!(
        "fn main() {{ let x = 1{}; return 0; }}",
        " + 1".repeat(5_000)
    );
    let path = temp_source("long_chain_bad", &src);
    let exe = path.with_extension("exe");
    let _ = std::fs::remove_file(&exe);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert_eq!(output.status.code().unwrap_or(-1), 1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(stderr.contains("E-P33"), "got:\n{stderr}");
    assert!(
        !exe.exists(),
        "a failed compilation must not leave an executable behind"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn nesting_limit_does_not_reject_ordinary_deep_code() {
    // 32 nested groups is well inside the documented limit and must keep
    // working: the guard must not turn into a general expression-size cap.
    let src = format!(
        "fn main() {{ let x = {}7{}; rt_print_int(x); return 0; }}",
        "(".repeat(32),
        ")".repeat(32)
    );
    let (code, stdout) = build_and_run("moderate_paren", &src);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), "7");
}

// ===========================================================================
// Parenthesized literals: build must agree with check
// ===========================================================================

#[test]
fn grouped_integer_literal_compiles() {
    let (code, stdout) = build_and_run(
        "group_int",
        "fn main() { let x = (1); rt_print_int(x); return 0; }",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), "1");
}

#[test]
fn grouped_literal_as_call_argument_compiles() {
    let (code, stdout) = build_and_run("group_arg", "fn main() { rt_print_int((7)); return 0; }");
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), "7");
}

#[test]
fn grouped_string_literal_compiles() {
    let (code, stdout) = build_and_run(
        "group_str",
        r#"fn main() { rt_print_str(("hi")); return 0; }"#,
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), "hi");
}

#[test]
fn double_grouped_literal_compiles() {
    let (code, stdout) = build_and_run(
        "group_double",
        "fn main() { rt_print_int(((9))); return 0; }",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), "9");
}

#[test]
fn grouped_literals_keep_their_values_in_aggregates() {
    let (code, stdout) = build_and_run(
        "group_agg",
        "fn main() { let t = ((\"a\"), (5)); rt_print_str(t.0); rt_print_int(t.1); return 0; }",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout.trim_end(), "a\n5");
}
