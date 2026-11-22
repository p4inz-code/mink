//! `mink fmt` on the command line: writing, `--check`, `--diff`, `--stdin`, folders and failures.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("mink_fmtcli_{label}_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn mink(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mink")).arg("fmt").args(args).output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

const MESSY: &str = "fn main(){let x=1;\nif x==1 {\nrt_print_int(x);\n}}\n";
const TIDY: &str = "fn main() {\n    let x = 1;\n    if x == 1 {\n        rt_print_int(x);\n    }\n}\n";

#[test]
fn formatting_rewrites_the_file_and_reports_it() {
    let dir = Scratch::new("write");
    let file = dir.write("a.mink", MESSY);
    let out = mink(&[file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), TIDY);
    assert!(text(&out.stdout).contains("formatted"), "{}", text(&out.stdout));
    assert!(text(&out.stdout).contains("1 file(s) formatted"));
    assert!(!dir.0.join("a.mink.mink-fmt.tmp").exists(), "no temporary file is left");

    let again = mink(&[file.to_str().unwrap()]);
    assert!(text(&again.stdout).contains("0 file(s) formatted, 1 already formatted"));
}

#[test]
fn check_lists_files_and_fails_without_writing() {
    let dir = Scratch::new("check");
    let messy = dir.write("messy.mink", MESSY);
    let tidy = dir.write("tidy.mink", TIDY);
    let out = mink(&["--check", dir.0.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = text(&out.stdout);
    assert!(stdout.contains("would reformat") && stdout.contains("messy.mink"), "{stdout}");
    assert!(!stdout.contains("would reformat") || !stdout.contains("tidy.mink\n"), "{stdout}");
    assert_eq!(std::fs::read_to_string(&messy).unwrap(), MESSY, "nothing was written");
    assert_eq!(std::fs::read_to_string(&tidy).unwrap(), TIDY);

    let clean = mink(&["--check", tidy.to_str().unwrap()]);
    assert_eq!(clean.status.code(), Some(0));
}

#[test]
fn diff_shows_the_change_and_writes_nothing() {
    let dir = Scratch::new("diff");
    let file = dir.write("a.mink", MESSY);
    let out = mink(&["--diff", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = text(&out.stdout);
    assert!(stdout.contains("--- ") && stdout.contains("(formatted)"), "{stdout}");
    assert!(stdout.contains("+    let x = 1;"), "{stdout}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), MESSY);
}

#[test]
fn stdin_goes_to_stdout_for_editors() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mink"))
        .args(["fmt", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(MESSY.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), TIDY);

    let mut bad = Command::new(env!("CARGO_BIN_EXE_mink"))
        .args(["fmt", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    bad.stdin.take().unwrap().write_all(b"fn f() { \"open }\n").unwrap();
    let out = bad.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out.stderr).contains("<stdin>"), "{}", text(&out.stderr));
    assert!(out.stdout.is_empty(), "nothing is printed for a file that cannot be formatted");
}

#[test]
fn stdout_prints_one_files_result_without_touching_it() {
    let dir = Scratch::new("stdout");
    let file = dir.write("a.mink", MESSY);
    let out = mink(&["--stdout", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), TIDY);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), MESSY);

    dir.write("b.mink", TIDY);
    let two = mink(&["--stdout", dir.0.to_str().unwrap()]);
    assert_eq!(two.status.code(), Some(2));
}

#[test]
fn folders_are_searched_and_hidden_and_build_folders_are_skipped() {
    let dir = Scratch::new("tree");
    dir.write("src/a.mink", MESSY);
    dir.write("src/deep/b.mink", MESSY);
    dir.write("target/c.mink", MESSY);
    dir.write(".git/d.mink", MESSY);
    dir.write(".mink/packages/e.mink", MESSY);
    dir.write("notes.txt", "not source");
    let out = mink(&[dir.0.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(std::fs::read_to_string(dir.0.join("src/a.mink")).unwrap(), TIDY);
    assert_eq!(std::fs::read_to_string(dir.0.join("src/deep/b.mink")).unwrap(), TIDY);
    for skipped in ["target/c.mink", ".git/d.mink", ".mink/packages/e.mink"] {
        assert_eq!(std::fs::read_to_string(dir.0.join(skipped)).unwrap(), MESSY, "{skipped} must be skipped");
    }
    assert_eq!(std::fs::read_to_string(dir.0.join("notes.txt")).unwrap(), "not source");
}

#[test]
fn a_file_that_cannot_be_formatted_is_reported_and_left_alone() {
    let dir = Scratch::new("bad");
    let bad = dir.write("bad.mink", "fn f() {\n    let s = \"never closed;\n}\n");
    let good = dir.write("good.mink", MESSY);
    let out = mink(&[dir.0.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "a failure is a failing exit code");
    let stderr = text(&out.stderr);
    assert!(stderr.contains("bad.mink") && stderr.contains("line 2"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&bad).unwrap(), "fn f() {\n    let s = \"never closed;\n}\n");
    assert_eq!(std::fs::read_to_string(&good).unwrap(), TIDY, "the other file is still formatted");
    assert!(text(&out.stdout).contains("1 failed"));
}

#[test]
fn argument_errors_are_usage_errors() {
    let out = mink(&["--bogus"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("unknown option '--bogus'"));
    assert_eq!(mink(&["--stdin", "a.mink"]).status.code(), Some(2));
    assert_eq!(mink(&["--stdout", "--check", "a.mink"]).status.code(), Some(2));
    let missing = mink(&["does-not-exist.mink"]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(text(&missing.stderr).contains("does-not-exist.mink"));
    let help = mink(&["--help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(text(&help.stdout).contains("never changes what a program says"));
}

#[test]
fn naming_nothing_is_a_usage_error_and_touches_no_file() {
    // Running `mink fmt` with no path must never format whatever folder it happens to run in.
    let dir = Scratch::new("nopath");
    let file = dir.write("a.mink", MESSY);
    let out = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("fmt")
        .current_dir(&dir.0)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("mink fmt ."), "{}", text(&out.stderr));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), MESSY);
}

#[test]
fn an_empty_folder_says_so() {
    let dir = Scratch::new("empty");
    let out = mink(&[dir.0.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).contains("No .mink files found"));
}

#[test]
fn a_formatted_program_still_builds_and_runs_the_same() {
    let dir = Scratch::new("runs");
    let file = dir.write("p.mink", "fn main(){let a=2;let b=3;rt_print_int(a*b+1);}\n");
    assert_eq!(mink(&[file.to_str().unwrap()]).status.code(), Some(0));
    let formatted = std::fs::read_to_string(&file).unwrap();
    assert!(formatted.contains("let a = 2;"), "{formatted}");
    let run = Command::new(env!("CARGO_BIN_EXE_mink")).arg("run").arg(&file).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", text(&run.stderr));
    assert!(text(&run.stdout).trim_start().starts_with('7'), "{}", text(&run.stdout));
    let _ = Path::new(".");
}
