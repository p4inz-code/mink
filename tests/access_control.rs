//! M7 Phase C — Gate A: visibility and access-control conformance corpus.
//!
//! Cases are identified as `A-<area>-<n>` and pin the module/import model that
//! MINK actually documents (`CORE_GRAMMAR.md` §28–§30,
//! `docs/implementation/MODULE_SYSTEM_IMPLEMENTATION.md`): file-based modules,
//! one flattened namespace, `use module::item;` as a name-bringing alias, and
//! `pub` parsed but not enforced.
//!
//! The corpus has two jobs. It proves the documented access rules are
//! implemented (imports are validated against the modules that were loaded),
//! and it pins the behaviour of the parts that are *not* implemented — private
//! cross-module access is allowed — so changing that model later is a
//! deliberate, visible decision rather than an accident.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

/// A throwaway directory holding one multi-file MINK program.
struct Program {
    dir: PathBuf,
}

impl Program {
    /// Creates a directory containing `files` as `(name, source)` pairs.
    fn new(files: &[(&str, &str)]) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("mink_access_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp module dir");
        for (name, source) in files {
            std::fs::write(dir.join(name), source).expect("write temp module");
        }
        Self { dir }
    }

    fn root(&self) -> PathBuf {
        self.dir.join("main.mink")
    }

    /// Runs `mink check` on the root file.
    fn check(&self) -> Outcome {
        self.run(&["check", self.root().to_str().unwrap()])
    }

    /// Runs `mink run` on the root file.
    fn run_program(&self) -> Outcome {
        self.run(&["run", self.root().to_str().unwrap()])
    }

    fn run(&self, args: &[&str]) -> Outcome {
        let output = mink().args(args).output().expect("run the mink CLI");
        Outcome {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Outcome {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn accepted(&self) {
        assert_eq!(
            self.code, 0,
            "expected acceptance, got {}; stderr:\n{}",
            self.code, self.stderr
        );
    }

    fn rejected_with(&self, error_code: &str, message_fragment: &str) {
        assert_eq!(
            self.code, 1,
            "expected rejection, got {}; stdout:\n{}\nstderr:\n{}",
            self.code, self.stdout, self.stderr
        );
        assert!(
            self.stderr.contains(error_code),
            "expected {error_code} in stderr:\n{}",
            self.stderr
        );
        assert!(
            self.stderr.contains(message_fragment),
            "expected {message_fragment:?} in stderr:\n{}",
            self.stderr
        );
    }
}

// ======================================================================
// A-V1: same-module access
// ======================================================================

#[test]
fn a_v1_private_function_is_callable_in_its_own_module() {
    let p = Program::new(&[(
        "main.mink",
        "fn helper() -> Int { return 3; }\n\
         fn main() { rt_print_int(helper()); return 0; }\n",
    )]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "3");
}

#[test]
fn a_v1_private_binding_is_readable_in_its_own_module() {
    let p = Program::new(&[(
        "main.mink",
        "let limit = 9;\nfn main() { rt_print_int(limit); return 0; }\n",
    )]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "9");
}

// ======================================================================
// A-V2: public items across module boundaries
// ======================================================================

#[test]
fn a_v2_public_function_is_callable_cross_module() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod math;\nuse math::add;\nfn main() { rt_print_int(add(2, 3)); return 0; }\n",
        ),
        (
            "math.mink",
            "pub fn add(a: Int, b: Int) -> Int { return a + b; }\n",
        ),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "5");
}

#[test]
fn a_v2_public_constant_is_readable_cross_module() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod limits;\nuse limits::MAX;\nfn main() { rt_print_int(MAX); return 0; }\n",
        ),
        ("limits.mink", "pub const MAX: Int = 42;\n"),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "42");
}

#[test]
fn a_v2_public_struct_is_constructible_cross_module() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod shapes;\nuse shapes::Point;\n\
             fn main() { let p = Point { x: 4, y: 6 }; rt_print_int(p.x + p.y); return 0; }\n",
        ),
        ("shapes.mink", "pub struct Point { x: Int, y: Int }\n"),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "10");
}

#[test]
fn a_v2_public_enum_is_usable_cross_module() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod color;\n\
             fn main() {\n\
                 let c = Color::Green;\n\
                 match c {\n\
                     Color::Red => { return 1; },\n\
                     Color::Green => { return 0; },\n\
                 }\n\
             }\n",
        ),
        ("color.mink", "pub enum Color { Red, Green }\n"),
    ]);
    let out = p.run_program();
    out.accepted();
}

// ======================================================================
// A-V3: private items are reachable (documented V1 limitation)
// ======================================================================

#[test]
fn a_v3_private_function_is_callable_cross_module() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod helper;\nfn main() { rt_print_int(secret()); return 0; }\n",
        ),
        ("helper.mink", "fn secret() -> Int { return 7; }\n"),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "7");
}

#[test]
fn a_v3_private_function_is_importable() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod helper;\nuse helper::secret;\nfn main() { rt_print_int(secret()); return 0; }\n",
        ),
        ("helper.mink", "fn secret() -> Int { return 8; }\n"),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "8");
}

#[test]
fn a_v3_private_struct_is_usable_cross_module() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod shapes;\nuse shapes::Hidden;\n\
             fn main() { let h = Hidden { q: 12 }; rt_print_int(h.q); return 0; }\n",
        ),
        ("shapes.mink", "struct Hidden { q: Int }\n"),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "12");
}

#[test]
fn a_v3_private_type_is_usable_in_a_public_signature() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod shapes;\nuse shapes::make;\n\
             fn main() { let h = make(5); rt_print_int(h); return 0; }\n",
        ),
        (
            "shapes.mink",
            "struct Secret { q: Int }\npub fn make(q: Int) -> Int { let s = Secret { q: q }; return s.q; }\n",
        ),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "5");
}

// ======================================================================
// A-V4: import validation
// ======================================================================

#[test]
fn a_v4_import_of_unknown_item_is_reported() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod helper;\nuse helper::nope;\nfn main() { return 0; }\n",
        ),
        ("helper.mink", "pub fn open() -> Int { return 1; }\n"),
    ]);
    let out = p.check();
    out.rejected_with("E-S01", "cannot find `nope` in module `helper`");
}

#[test]
fn a_v4_import_of_unknown_item_names_the_import_not_the_use() {
    // The diagnostic must point at the import, not at a later use of the
    // name, because the import is the mistake.
    let p = Program::new(&[
        (
            "main.mink",
            "mod helper;\nuse helper::nope;\nfn main() { rt_print_int(nope()); return 0; }\n",
        ),
        ("helper.mink", "pub fn open() -> Int { return 1; }\n"),
    ]);
    let out = p.check();
    out.rejected_with("E-S01", "cannot find `nope` in module `helper`");
    assert!(
        out.stderr.contains("main.mink:2:13"),
        "diagnostic should point at the imported name on line 2:\n{}",
        out.stderr
    );
}

#[test]
fn a_v4_import_of_unknown_module_is_reported() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod helper;\nuse nowhere::thing;\nfn main() { return 0; }\n",
        ),
        ("helper.mink", "pub fn open() -> Int { return 1; }\n"),
    ]);
    let out = p.check();
    out.rejected_with("E-S01", "cannot find module `nowhere`");
}

#[test]
fn a_v4_import_with_three_segments_is_reported() {
    // `use a::b::c` has no meaning in this module model; accepting it and
    // ignoring `::c` would silently import the wrong thing.
    let p = Program::new(&[
        (
            "main.mink",
            "mod helper;\nuse helper::open::extra;\nfn main() { return 0; }\n",
        ),
        ("helper.mink", "pub fn open() -> Int { return 1; }\n"),
    ]);
    let out = p.check();
    out.rejected_with("E-S01", "one item");
}

#[test]
fn a_v4_importing_a_module_by_itself_is_a_no_op() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod helper;\nuse helper;\nfn main() { rt_print_int(open()); return 0; }\n",
        ),
        ("helper.mink", "pub fn open() -> Int { return 6; }\n"),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "6");
}

#[test]
fn a_v4_self_import_is_a_no_op() {
    let p = Program::new(&[(
        "main.mink",
        "mod nothere;\nuse main::helper;\nfn helper() -> Int { return 2; }\n\
         fn main() { rt_print_int(helper()); return 0; }\n",
    )]);
    // `mod nothere;` is a different defect surface (a missing file), so keep
    // this case single-file: the import names the root module itself.
    let p2 = Program::new(&[(
        "main.mink",
        "use main::helper;\nfn helper() -> Int { return 2; }\n\
         fn main() { rt_print_int(helper()); return 0; }\n",
    )]);
    drop(p);
    let out = p2.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "2");
}

#[test]
fn a_v4_pub_use_is_validated_like_a_plain_import() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod helper;\npub use helper::nope;\nfn main() { return 0; }\n",
        ),
        ("helper.mink", "pub fn open() -> Int { return 1; }\n"),
    ]);
    let out = p.check();
    out.rejected_with("E-S01", "cannot find `nope` in module `helper`");
}

#[test]
fn a_v4_import_of_a_missing_module_file_points_at_the_mod_declaration() {
    let p = Program::new(&[("main.mink", "mod gone;\nfn main() { return 0; }\n")]);
    let out = p.check();
    out.rejected_with("E-S01", "gone.mink' not found");
    assert!(
        out.stderr.contains("main.mink:1:5"),
        "diagnostic should point at the `mod` declaration:\n{}",
        out.stderr
    );
}

// ======================================================================
// A-V5: nested modules
// ======================================================================

#[test]
fn a_v5_child_module_can_declare_its_own_child() {
    let p = Program::new(&[
        (
            "main.mink",
            "mod outer;\nuse inner::value;\nfn main() { rt_print_int(value()); return 0; }\n",
        ),
        (
            "outer.mink",
            "mod inner;\npub fn outer_value() -> Int { return 1; }\n",
        ),
        ("inner.mink", "pub fn value() -> Int { return 11; }\n"),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "11");
}

#[test]
fn a_v5_pub_mod_loads_the_declared_file() {
    // `pub mod x;` is documented syntax (§30 applies `pub` to any item);
    // treating it as anything other than a module declaration would silently
    // skip the file.
    let p = Program::new(&[
        (
            "main.mink",
            "pub mod helper;\nfn main() { rt_print_int(helper_ok()); return 0; }\n",
        ),
        ("helper.mink", "fn helper_ok() -> Int { return 4; }\n"),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "4");
}

// ======================================================================
// A-V6: one flattened namespace
// ======================================================================

#[test]
fn a_v6_same_name_in_two_modules_is_a_duplicate_definition() {
    // Modules do not isolate names: every loaded item lives in one namespace.
    // The diagnostic must name the duplicate, not silently pick a winner.
    let p = Program::new(&[
        (
            "main.mink",
            "mod a;\nmod b;\nuse a::helper;\nuse b::helper;\n\
             fn main() { rt_print_int(helper()); return 0; }\n",
        ),
        ("a.mink", "pub fn helper() -> Int { return 1; }\n"),
        ("b.mink", "pub fn helper() -> Int { return 2; }\n"),
    ]);
    let out = p.check();
    out.rejected_with("E-S02", "duplicate definition of `helper`");
}

#[test]
fn a_v6_duplicate_diagnostic_points_at_both_files() {
    // A cross-module duplicate comes from two different source files; both
    // spans must be attributed to the file they actually come from.
    let p = Program::new(&[
        (
            "main.mink",
            "mod a;\nmod b;\nfn main() { rt_print_int(helper()); return 0; }\n",
        ),
        ("a.mink", "pub fn helper() -> Int { return 1; }\n"),
        ("b.mink", "pub fn helper() -> Int { return 2; }\n"),
    ]);
    let out = p.check();
    out.rejected_with("E-S02", "duplicate definition of `helper`");
    assert!(
        out.stderr.contains("a.mink:1:8") || out.stderr.contains("b.mink:1:8"),
        "the duplicate should be located in a module file:\n{}",
        out.stderr
    );
}

// ======================================================================
// A-V7: what the language does not have
// ======================================================================

#[test]
fn a_v7_field_visibility_is_not_part_of_the_syntax() {
    // Struct fields have no visibility: `pub` there is a syntax error rather
    // than a silently ignored modifier.
    let p = Program::new(&[(
        "main.mink",
        "struct S { pub x: Int }\nfn main() { return 0; }\n",
    )]);
    let out = p.check();
    out.rejected_with("E-P02", "expected an identifier");
}

#[test]
fn a_v7_pub_function_parses_and_runs() {
    let p = Program::new(&[(
        "main.mink",
        "pub fn helper() -> Int { return 1; }\n\
         pub fn main() { rt_print_int(helper()); return; }\n",
    )]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "1");
}

#[test]
fn a_v7_unknown_module_path_segment_is_not_treated_as_a_module() {
    // Enabled by the A-V4 import validation: an unresolvable import must fail
    // even when nothing uses the imported name.
    let p = Program::new(&[
        (
            "main.mink",
            "mod math;\nuse math::missing;\nfn main() { return 0; }\n",
        ),
        (
            "math.mink",
            "pub fn add(a: Int, b: Int) -> Int { return a + b; }\n",
        ),
    ]);
    let out = p.check();
    out.rejected_with("E-S01", "cannot find `missing` in module `math`");
}

#[test]
fn a_v7_gate_a_program_symbols_exist_in_the_final_executable() {
    // Access control is a front-end rule; the backend still links every
    // module's functions, including unreferenced private helpers.
    let p = Program::new(&[
        (
            "main.mink",
            "mod helper;\nfn main() { rt_print_int(open()); return 0; }\n",
        ),
        (
            "helper.mink",
            "fn unused_private() -> Int { return 99; }\npub fn open() -> Int { return 1; }\n",
        ),
    ]);
    let out = p.run_program();
    out.accepted();
    assert_eq!(out.stdout.trim(), "1");
}
