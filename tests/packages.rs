//! Directory-package tests — Session 109 (L68/P03).
//!
//! `mod name;` resolves a *directory package* to `name/mod.mink` (MINK's
//! equivalent of `name/__init__.py`) when `name.mink` does not exist, and a
//! package's own `mod` declarations resolve inside the package directory.
//! These tests exercise the real `mink build`/`mink check` CLI against
//! committed fixtures.

use std::path::PathBuf;
use std::process::Command;

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn fixture(parts: &[&str]) -> PathBuf {
    let mut path = PathBuf::from("tests/packages");
    for part in parts {
        path.push(part);
    }
    path
}

#[test]
fn l68_directory_package_builds_and_runs() {
    let source = fixture(&["main.mink"]);
    let output = mink().arg("build").arg(&source).output().unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = source.with_extension("exe");
    let run = Command::new(&exe).output().expect("run package fixture");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let _ = std::fs::remove_file(&exe);
    assert!(stderr.is_empty(), "unexpected stderr: {stderr:?}");
    assert_eq!(run.status.code(), Some(0), "stdout={stdout:?}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["7", "15"], "stdout={stdout:?}");
}

#[test]
fn l68_directory_package_checks_clean() {
    let source = fixture(&["main.mink"]);
    let output = mink().arg("check").arg(&source).output().unwrap();
    assert!(
        output.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn l68_flat_module_wins_over_directory_package() {
    // `dup.mink` and `dup/mod.mink` both exist. The fixture's main calls
    // `flat_marker`, which only `dup.mink` defines, so a successful build
    // proves the flat module was chosen.
    let source = fixture(&["precedence", "main.mink"]);
    let output = mink().arg("build").arg(&source).output().unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = source.with_extension("exe");
    let run = Command::new(&exe).output().expect("run precedence fixture");
    let _ = std::fs::remove_file(&exe);
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "11");
}

#[test]
fn l68_two_sibling_modules_resolve_distinct_symbols() {
    // Regression: declaration-name lookups used to be keyed by byte offset
    // alone, so two modules whose names shared an offset collapsed onto one
    // symbol and one module's items became unreachable. This fixture is the
    // minimal reproduction (alpha_one and beta_two both start at offset 7).
    let source = fixture(&["siblings", "main.mink"]);
    let output = mink().arg("build").arg(&source).output().unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let exe = source.with_extension("exe");
    let run = Command::new(&exe).output().expect("run siblings fixture");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let _ = std::fs::remove_file(&exe);
    assert_eq!(run.status.code(), Some(0), "stdout={stdout:?}");
    assert_eq!(stdout.trim(), "3", "stdout={stdout:?}");
}

#[test]
fn p03_missing_module_is_still_reported() {
    let source = fixture(&["missing", "main.mink"]);
    let output = mink().arg("check").arg(&source).output().unwrap();
    assert!(!output.status.success(), "expected a missing-module error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nonexistent"),
        "error should name the missing module: {stderr}"
    );
}
