//! Module-qualified paths (`module::member`) and the explanation for a module
//! that hides a bundled one.
//!
//! Background: the 1.0.4 clean-install report showed `mod json;` and
//! `mod math;` failing with a duplicate `main`. A `json.mink` or `math.mink`
//! next to the program was loaded in place of the bundled module, and nothing
//! said so. The lookup order is unchanged (a file beside the program still
//! wins, so programs shipping their own `math.mink` keep working); the
//! compiler now explains the shadowing when it breaks the build.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory under the system temp directory, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mink_modq_{label}_{}_{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, source).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs the checker and returns every diagnostic rendered as text.
fn diagnostics(path: &Path) -> Vec<String> {
    let mut sources = mink::source::SourceMap::new();
    match mink::driver::check(&mut sources, path) {
        Ok(report) => report.errors.iter().map(|e| e.to_string()).collect(),
        Err(mink::driver::BuildError::FrontEnd(report)) => {
            report.errors.iter().map(|e| e.to_string()).collect()
        }
        Err(other) => panic!("unexpected failure: {other}"),
    }
}

#[test]
fn qualified_call_to_a_bundled_module_checks_clean() {
    let dir = Scratch::new("one");
    let path = dir.write(
        "prog.mink",
        "mod math;\nfn main() {\n    let r = math::math_abs(-42);\n    rt_print_int(r);\n}\n",
    );
    assert_eq!(diagnostics(&path), Vec::<String>::new());
}

#[test]
fn qualified_call_takes_several_arguments() {
    let dir = Scratch::new("two");
    let path = dir.write(
        "prog.mink",
        "mod math;\nfn main() {\n    rt_print_int(math::math_max(3, 9));\n}\n",
    );
    assert_eq!(diagnostics(&path), Vec::<String>::new());
}

#[test]
fn qualified_path_above_the_mod_declaration_still_resolves() {
    let dir = Scratch::new("order");
    let path = dir.write(
        "prog.mink",
        "fn main() {\n    rt_print_int(math::math_abs(-1));\n}\nmod math;\n",
    );
    assert_eq!(diagnostics(&path), Vec::<String>::new());
}

#[test]
fn misspelled_member_is_one_module_error() {
    let dir = Scratch::new("typo");
    let path = dir.write(
        "prog.mink",
        "mod math;\nfn main() {\n    let r = math::math_absx(-42);\n    rt_print_int(r);\n}\n",
    );
    let errors = diagnostics(&path);
    assert_eq!(errors.len(), 1, "one mistake, one report: {errors:?}");
    assert!(
        errors[0].contains("cannot find `math_absx` in module `math`"),
        "got: {}",
        errors[0]
    );
}

#[test]
fn enum_variants_are_not_taken_for_module_paths() {
    let dir = Scratch::new("enum");
    let path = dir.write(
        "prog.mink",
        "mod math;\nenum Color { Red, Green }\nfn main() {\n    let c = Color::Red;\n    rt_print_int(math::math_abs(-1));\n}\n",
    );
    assert_eq!(diagnostics(&path), Vec::<String>::new());
}

#[test]
fn a_local_module_with_a_bundled_name_still_wins_and_builds() {
    // Programs that ship their own `math.mink` must keep working.
    let dir = Scratch::new("own");
    dir.write("math.mink", "fn double(n: Int) -> Int {\n    return n + n;\n}\n");
    let path = dir.write(
        "prog.mink",
        "mod math;\nfn main() {\n    rt_print_int(math::double(21));\n}\n",
    );
    assert_eq!(diagnostics(&path), Vec::<String>::new());
}

#[test]
fn a_stray_file_that_hides_a_bundled_module_is_explained() {
    // The 1.0.4 clean-install report: a `json.mink` probe beside the program
    // defines `main`, so the program has two.
    let dir = Scratch::new("shadow");
    dir.write("json.mink", "fn main() {\n    rt_print_str(\"x\\n\");\n}\n");
    let path = dir.write(
        "prog.mink",
        "mod json;\nfn main() {\n    rt_print_str(\"hi\\n\");\n}\n",
    );
    let errors = diagnostics(&path);
    assert!(
        errors.iter().any(|e| e.contains("duplicate definition of `main`")),
        "the underlying error is still reported: {errors:?}"
    );
    let explanation = errors
        .iter()
        .find(|e| e.contains("instead of the bundled `json` module"))
        .unwrap_or_else(|| panic!("no explanation of the shadowing in {errors:?}"));
    assert!(explanation.contains("`json.mink`"), "got: {explanation}");
}

#[test]
fn shadowing_alone_is_silent_when_the_build_is_fine() {
    let dir = Scratch::new("quiet");
    dir.write("math.mink", "fn only_here() -> Int {\n    return 7;\n}\n");
    let path = dir.write(
        "prog.mink",
        "mod math;\nfn main() {\n    rt_print_int(only_here());\n}\n",
    );
    assert_eq!(diagnostics(&path), Vec::<String>::new());
}

#[test]
fn qualified_call_runs_and_prints_the_result() {
    let dir = Scratch::new("run");
    let path = dir.write(
        "prog.mink",
        "mod math;\nfn main() {\n    rt_print_int(math::math_abs(-42));\n}\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("run mink");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).trim_start().starts_with("42"));
}
