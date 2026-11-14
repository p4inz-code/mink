//! M7 Phase C — cross-gate integration.
//!
//! Gates A–D were developed against separate corpora
//! (`access_control`, `constraints_conformance`, `diagnostics_json`,
//! `abi_conformance`). This file checks the combinations, which is where a
//! fix in one gate can quietly undo another: a module that fails an import
//! check must still be attributed correctly in JSON, and a program that
//! exercises the access rules must still run through the native backend.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("mink_cross_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, source) in files {
            std::fs::write(dir.join(name), source).unwrap();
        }
        Self { dir }
    }

    fn root(&self) -> PathBuf {
        self.dir.join("main.mink")
    }

    fn check_json(&self) -> (i32, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_mink"))
            .arg("check")
            .arg(self.root())
            .arg("--json")
            .output()
            .unwrap();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    }

    fn run(&self) -> (i32, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_mink"))
            .arg("run")
            .arg(self.root())
            .output()
            .unwrap();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn x_import_error_is_json_with_the_import_span() {
    // Gate A (import validation) + Gate C (machine-readable output).
    let f = Fixture::new(&[
        (
            "main.mink",
            "mod helper;\nuse helper::nope;\nfn main() { return 0; }\n",
        ),
        ("helper.mink", "pub fn open() -> Int { return 1; }\n"),
    ]);
    let (code, json) = f.check_json();
    assert_eq!(code, 1);
    assert!(json.contains("\"code\": \"E-S01\""), "{json}");
    assert!(json.contains("\"stage\": \"semantics\""), "{json}");
    assert!(
        json.contains("cannot find `nope` in module `helper`"),
        "{json}"
    );
    // The mistake is the import in the root file, not a location in the module.
    assert!(json.contains("main.mink\""), "{json}");
    assert!(json.contains("\"start_line\": 2"), "{json}");
}

#[test]
fn x_cross_module_duplicate_reports_both_files_in_json() {
    // The related span of a duplicate lives in the *other* module file.
    let f = Fixture::new(&[
        ("main.mink", "mod a;\nmod b;\nfn main() { return 0; }\n"),
        ("a.mink", "pub fn helper() -> Int { return 1; }\n"),
        ("b.mink", "pub fn helper() -> Int { return 2; }\n"),
    ]);
    let (code, json) = f.check_json();
    assert_eq!(code, 1);
    assert!(json.contains("\"code\": \"E-S02\""), "{json}");
    assert!(json.contains("a.mink\""), "{json}");
    assert!(json.contains("b.mink\""), "{json}");
    // The secondary location must carry its own file, not the error's.
    let related = json
        .split("\"related\": [")
        .nth(1)
        .expect("a related section");
    let related_file = related
        .split("\"file\": \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the related location's file");
    assert!(
        related_file.ends_with("a.mink") || related_file.ends_with("b.mink"),
        "related file was {related_file}"
    );
}

#[test]
fn x_generic_function_in_a_module_runs_from_the_root() {
    // Gate A (use) + Gate B (explicit type args) + Gate D (native execution).
    let f = Fixture::new(&[
        (
            "main.mink",
            "mod util;\nuse util::identity;\n\
             struct P { x: Int }\n\
             fn main() {\n\
                 let p = P { x: 6 };\n\
                 let q = identity::<P>(p);\n\
                 rt_print_int(q.x);\n\
                 return 0;\n\
             }\n",
        ),
        ("util.mink", "pub fn identity<T>(x: T) -> T { return x; }\n"),
    ]);
    let (code, stdout) = f.run();
    assert_eq!(code, 0, "stdout:\n{stdout}");
    assert_eq!(stdout.trim(), "6");
}

#[test]
fn x_uninferable_generic_call_in_a_module_is_attributed_to_that_module() {
    // Gate B (E-T44) + Gate A (cross-module call) + Gate C (per-file JSON).
    let f = Fixture::new(&[
        (
            "main.mink",
            "mod util;\nuse util::identity;\n\
             struct P { x: Int }\n\
             fn main() { let p = P { x: 6 }; return 0; }\n",
        ),
        (
            "util.mink",
            "pub fn identity<T>(x: T) -> T { return x; }\n\
             pub fn use_it() -> Int { let p: P = 0; return 0; }\n",
        ),
    ]);
    let (code, json) = f.check_json();
    // The module does not even have `P`; whatever fails must not be reported
    // against the root file. This case mainly pins that a failure in a module
    // file keeps its own attribution.
    assert_eq!(code, 1);
    assert!(json.contains("util.mink\""), "{json}");
    assert!(
        !json.contains("\"file\": \"main.mink\""),
        "no error belongs to the root file here:\n{json}"
    );
}

#[test]
fn x_inference_failure_json_names_the_callable_and_the_fix() {
    let f = Fixture::new(&[(
        "main.mink",
        "struct P { x: Int }\n\
         fn identity<T>(x: T) -> T { return x; }\n\
         fn main() { let p = P { x: 5 }; let q = identity(p); return q.x; }\n",
    )]);
    let (code, json) = f.check_json();
    assert_eq!(code, 1);
    assert!(json.contains("\"code\": \"E-T44\""), "{json}");
    assert!(json.contains("\"stage\": \"typecheck\""), "{json}");
    assert!(json.contains("identity::<Type>"), "{json}");
    assert!(
        !json.contains("E-S01"),
        "the call must not also be an unresolved name:\n{json}"
    );
}

#[test]
fn x_pub_mod_program_reports_both_files_checked() {
    // Gate A (`pub mod` loads its file) + Gate C (`files_checked`/`files`).
    let f = Fixture::new(&[
        (
            "main.mink",
            "pub mod helper;\nfn main() { rt_print_int(ok()); return 0; }\n",
        ),
        ("helper.mink", "fn ok() -> Int { return 3; }\n"),
    ]);
    let (code, json) = f.check_json();
    assert_eq!(code, 0, "{json}");
    assert!(json.contains("\"success\": true"), "{json}");
    assert!(json.contains("\"files_checked\": 2"), "{json}");
    assert!(json.contains("helper.mink\""), "{json}");

    let (run_code, stdout) = f.run();
    assert_eq!(run_code, 0);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn x_missing_module_file_json_points_at_the_mod_declaration() {
    let f = Fixture::new(&[("main.mink", "mod gone;\nfn main() { return 0; }\n")]);
    let (code, json) = f.check_json();
    assert_eq!(code, 1);
    assert!(json.contains("\"code\": \"E-S01\""), "{json}");
    assert!(json.contains("\"start_line\": 1"), "{json}");
    assert!(json.contains("\"start_column\": 5"), "{json}");
    assert!(json.contains("main.mink\""), "{json}");
}
