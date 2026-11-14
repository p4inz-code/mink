//! M7 Phase C — Gate B: generics and constraints conformance corpus.
//!
//! Gate B is a design gate: MINK has no trait or constraint system, and this
//! milestone deliberately does not add one (`docs/language/TYPE_SYSTEM.md`
//! §12–§13 remain a design brief). What exists is monomorphization: generic
//! declarations are copied per concrete type argument, and the copy is then
//! checked like ordinary code. That has two consequences this file pins:
//!
//! * Soundness does not depend on constraints, because no generic body is
//!   checked with an abstract `T` — `addg(true, false)` is rejected as
//!   `Bool + Bool`, at the instantiation, like hand-written code.
//! * A call whose type arguments cannot be inferred from its arguments cannot
//!   be instantiated at all. Before Phase C that surfaced as "cannot find name
//!   `identity`", naming a function that plainly exists; it is now `E-T44` at
//!   the call, naming the fix.
//!
//! The `B-UNSUPPORTED` cases pin what the language does *not* have. They are
//! expected to change when traits land, which makes that change visible.

use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn check_source(source: &str) -> mink::driver::CheckReport {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut sources = mink::source::SourceMap::new();
    let path =
        std::env::temp_dir().join(format!("mink_constraints_{}_{n}.mink", std::process::id()));
    std::fs::write(&path, source).expect("write temp file");
    let report = mink::driver::check(&mut sources, &path).expect("check should not panic");
    let _ = std::fs::remove_file(&path);
    report
}

fn codes(report: &mink::driver::CheckReport) -> Vec<&'static str> {
    report.errors.iter().map(|e| e.code()).collect()
}

fn messages(report: &mink::driver::CheckReport) -> String {
    report
        .errors
        .iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_accepted(report: &mink::driver::CheckReport) {
    assert!(
        report.errors.is_empty(),
        "expected acceptance, got:\n{}",
        messages(report)
    );
}

fn assert_rejected(report: &mink::driver::CheckReport, code: &str) {
    assert!(
        codes(report).contains(&code),
        "expected {code}, got {:?}:\n{}",
        codes(report),
        messages(report)
    );
}

/// Runs a single-file program through the real CLI, for the cases whose
/// answer is a value rather than a diagnostic.
fn run_source(source: &str) -> (i32, String, String) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mink_constraints_run_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("main.mink");
    std::fs::write(&path, source).expect("write temp program");
    let output = Command::new(env!("CARGO_BIN_EXE_mink"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("run the mink CLI");
    let _ = std::fs::remove_dir_all(&dir);
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

// ======================================================================
// B-INFER: a call that cannot choose its type arguments
// ======================================================================

#[test]
fn b_infer_struct_typed_argument_reports_the_call_not_a_missing_function() {
    // The limitation is documented (only literals, typed locals, and calls to
    // non-generic functions are read), but the old diagnostic claimed the
    // function did not exist.
    let report = check_source(
        "struct P { x: Int }\n\
         fn identity<T>(x: T) -> T { return x; }\n\
         fn main() { let p = P { x: 5 }; let q = identity(p); return q.x; }\n",
    );
    assert_rejected(&report, "E-T44");
    assert!(
        !codes(&report).contains(&"E-S01"),
        "the call must not also be reported as an unknown name: {:?}\n{}",
        codes(&report),
        messages(&report)
    );
    let message = messages(&report);
    assert!(
        message.contains("cannot infer the type arguments of `identity`"),
        "{message}"
    );
    assert!(
        message.contains("identity::<Type>"),
        "the diagnostic should name the fix: {message}"
    );
}

#[test]
fn b_infer_typed_local_is_inferred() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }\n\
         fn main() { let n: Int = 1; let m = identity(n); return m; }\n",
    );
    assert_accepted(&report);
}

#[test]
fn b_infer_literal_is_inferred() {
    let report = check_source(
        "fn identity<T>(x: T) -> T { return x; }\n\
         fn main() { let a = identity(42); let b = identity(true); return 0; }\n",
    );
    assert_accepted(&report);
}

#[test]
fn b_infer_explicit_type_arguments_always_work() {
    let (code, stdout, stderr) = run_source(
        "struct P { x: Int }\n\
         fn identity<T>(x: T) -> T { return x; }\n\
         fn main() { let p = P { x: 5 }; let q = identity::<P>(p); rt_print_int(q.x); return 0; }\n",
    );
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert_eq!(stdout.trim(), "5");
}

// ======================================================================
// B-CHECK: generic bodies are checked after substitution
// ======================================================================

#[test]
fn b_check_generic_body_is_checked_at_the_instantiation() {
    // No constraint is needed to reject this: the copy for `Bool` fails the
    // same operator rule hand-written code does.
    let report = check_source(
        "fn addg<T>(a: T, b: T) -> T { return a + b; }\n\
         fn main() { let c = addg(true, false); return 0; }\n",
    );
    assert_rejected(&report, "E-T02");
    assert!(
        messages(&report).contains("Bool"),
        "the rejected instantiation should name the type: {}",
        messages(&report)
    );
}

#[test]
fn b_check_generic_member_access_is_checked_at_the_instantiation() {
    // `p.x` is not valid for an abstract `T`; after `T = P` it is ordinary
    // field access and must be accepted.
    let report = check_source(
        "struct P { x: Int }\n\
         fn getx<T>(p: T) -> Int { return p.x; }\n\
         fn main() { let p = P { x: 7 }; return getx::<P>(p); }\n",
    );
    assert_accepted(&report);
}

#[test]
fn b_check_mismatched_instantiations_stay_separate() {
    // The `Int` copy is fine; the `Bool` copy is not, and a mistake in one
    // must not be hidden by the other.
    let report = check_source(
        "fn addg<T>(a: T, b: T) -> T { return a + b; }\n\
         fn main() {\n\
             let n = addg(1, 2);\n\
             let b = addg(true, false);\n\
             return n;\n\
         }\n",
    );
    assert_rejected(&report, "E-T02");
}

// ======================================================================
// B-INST: generic aggregates
// ======================================================================

#[test]
fn b_inst_generic_struct_literal_infers_its_type_arguments() {
    let (code, stdout, stderr) = run_source(
        "struct Box<T> { v: T }\n\
         fn main() { let b = Box { v: 3 }; rt_print_int(b.v); return 0; }\n",
    );
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn b_inst_two_parameter_generic_struct() {
    let (code, stdout, stderr) = run_source(
        "struct Pair<T, U> { a: T, b: U }\n\
         fn main() { let p = Pair { a: 4, b: 5 }; rt_print_int(p.a + p.b); return 0; }\n",
    );
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert_eq!(stdout.trim(), "9");
}

#[test]
fn b_inst_generic_enum_variant_payload() {
    let (code, stdout, stderr) = run_source(
        "enum Maybe<T> { Just(T), Nothing }\n\
         fn main() {\n\
             let m = Maybe::Just(5);\n\
             match m {\n\
                 Maybe::Just(v) => { rt_print_int(v); },\n\
                 Maybe::Nothing => { },\n\
             }\n\
             return 0;\n\
         }\n",
    );
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert_eq!(stdout.trim(), "5");
}

// ======================================================================
// B-GAP: known holes that constraints would close
// ======================================================================

#[test]
fn b_gap_an_uninstantiated_generic_body_is_not_type_checked() {
    // Documented limitation, pinned here: `x.y` on an abstract `T` is
    // meaningless, but no copy exists, so nothing is checked. It is not
    // unsound (there is no way to call the function), and it is the concrete
    // argument for checking generic bodies once, which is what a constraint
    // system would enable.
    let report = check_source(
        "fn never_called<T>(x: T) -> T { return x.y; }\n\
         fn main() { return 0; }\n",
    );
    assert_accepted(&report);
}

// ======================================================================
// B-UNSUPPORTED: syntax the language reserves but does not implement
// ======================================================================

#[test]
fn b_unsupported_trait_declarations_are_rejected() {
    let report = check_source(
        "trait Show { fn show(self) -> Int; }\n\
         fn main() { return 0; }\n",
    );
    assert_rejected(&report, "E-P01");
}

#[test]
fn b_unsupported_impl_blocks_are_rejected() {
    let report = check_source(
        "struct P { x: Int }\n\
         impl Show for P { }\n\
         fn main() { return 0; }\n",
    );
    assert_rejected(&report, "E-P01");
}

#[test]
fn b_unsupported_trait_bounds_are_rejected() {
    // `T: Bound` is not silently ignored: the parser rejects the bound rather
    // than producing a generic function whose constraint does nothing.
    let report = check_source(
        "fn f<T: Show>(x: T) -> T { return x; }\n\
         fn main() { return 0; }\n",
    );
    let errors = codes(&report);
    assert!(
        !errors.is_empty(),
        "a trait bound must not be silently accepted"
    );
    assert!(
        errors.iter().all(|c| c.starts_with("E-P")),
        "bound syntax should fail at the parser, got {errors:?}:\n{}",
        messages(&report)
    );
}

#[test]
fn b_unsupported_polymorphic_dispatch_does_not_exist() {
    // There is no syntax for a value of unknown concrete type, so a trait
    // object cannot even be written.
    let report = check_source(
        "fn take(x: dyn Show) -> Int { return 0; }\n\
         fn main() { return 0; }\n",
    );
    assert!(!report.errors.is_empty(), "`dyn` must not be accepted");
}
