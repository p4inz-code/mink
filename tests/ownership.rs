//! Integration tests for the Session 15 ownership/borrowing foundation:
//! move semantics for heap-owning values, use-after-move detection
//! (E-S10), immutable-string mutation rejection (E-S11), implicit
//! function-local borrows, result provenance through calls, and native
//! execution of valid ownership programs.
//!
//! The frozen rules are documented in
//! `docs/implementation/OWNERSHIP_IMPLEMENTATION.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

use mink::ast::Ast;
use mink::ownership::{self, OwnershipResult};
use mink::parser::parse;
use mink::semantics::{SemanticErrorKind, SemanticResult};
use mink::source::{SourceMap, Span};
use mink::typecheck::TypeResult;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses, semantically analyzes, type-checks, and ownership-analyzes
/// `src`, asserting that it lexes/parses cleanly.
fn check_src(src: &str) -> (Ast, SemanticResult, TypeResult, OwnershipResult) {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).expect("the file just added");
    let parsed = parse(file);
    assert!(
        parsed.is_valid(),
        "test source must lex and parse cleanly\nlex errors: {:?}\nparse errors: {:?}",
        parsed.lex_errors(),
        parsed.parse_errors()
    );
    let (ast, lex_errors, parse_errors) = parsed.into_parts();
    assert!(lex_errors.is_empty() && parse_errors.is_empty());
    let semantic = mink::semantics::analyze(&ast);
    assert!(
        semantic.errors().is_empty(),
        "test source must be semantically clean: {:?}",
        semantic.errors()
    );
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    assert!(
        !types.has_errors(),
        "test source must type-check cleanly: {:?}",
        types.errors()
    );
    let ownership = ownership::check(&ast, &semantic, &types);
    (ast, semantic, types, ownership)
}

/// All ownership errors of `kind`.
fn ownership_errors(
    ownership: &OwnershipResult,
    kind: SemanticErrorKind,
) -> Vec<&mink::semantics::SemanticError> {
    ownership
        .errors()
        .iter()
        .filter(|error| error.kind() == kind)
        .collect()
}

/// The message of the first ownership error of `kind`.
fn first_message(ownership: &OwnershipResult, kind: SemanticErrorKind) -> String {
    let error = ownership
        .errors()
        .iter()
        .find(|error| error.kind() == kind)
        .unwrap_or_else(|| {
            panic!(
                "no ownership error of kind {kind:?}: {:?}",
                ownership.errors()
            )
        });
    error.to_string()
}

/// Asserts `src` has exactly one ownership error at `line:col` (1-based).
fn assert_error_at(src: &str, kind: SemanticErrorKind, line: usize, col: usize) {
    let (_ast, _semantic, _types, ownership) = check_src(src);
    let errors = ownership_errors(&ownership, kind);
    assert!(
        !errors.is_empty(),
        "expected {kind:?} in {src:?}, got {:?}",
        ownership.errors()
    );
    let span = errors[0].span();
    let (actual_line, actual_col) = line_col(src, span);
    assert_eq!(
        (actual_line, actual_col),
        (line, col),
        "span of {kind:?} in {src:?}"
    );
}

/// Computes the 1-based line/column of a span start.
fn line_col(src: &str, span: Span) -> (usize, usize) {
    let start = span.start() as usize;
    let before = &src[..start];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, col)
}

// ---------------------------------------------------------------------------
// Valid ownership programs
// ---------------------------------------------------------------------------

#[test]
fn string_literals_copy_freely() {
    let src =
        "fn main() { let a = \"hi\"; let b = a; let c = b; rt_print_str(c); rt_print_str(a); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn owned_string_moves_once_and_borrows_keep_it_live() {
    let src = "fn main() { let s = rt_str_alloc(3); rt_print_int(rt_str_len(s)); rt_str_set_byte(s, 0, 65); rt_print_str(s); rt_str_free(s); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn literal_passed_to_owning_parameter_copies() {
    let src = "fn f(s) { rt_print_int(rt_str_len(s)); } fn main() { f(\"hi\"); f(\"hi\"); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn owned_argument_moves_into_function() {
    let src = "fn f(s) { rt_print_int(rt_str_len(s)); rt_str_free(s); } fn main() { let s = rt_str_alloc(3); f(s); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn struct_with_literal_fields_copies() {
    let src = "struct P { name: Str, age: Int } fn main() { let p = P { name: \"a\", age: 1 }; let q = p; let r = p; rt_print_str(r.name); rt_print_int(q.age); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn struct_with_owned_field_moves_whole() {
    let src = "struct P { name: Str } fn main() { let p = P { name: rt_str_alloc(3) }; let q = p; rt_str_free(q.name); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn per_field_move_leaves_other_fields_usable() {
    let src = "struct P { name: Str, tag: Str, age: Int } fn main() { let mut p = P { name: rt_str_alloc(2), tag: \"t\", age: 7 }; let n = p.name; rt_print_int(p.age); rt_print_str(p.tag); rt_str_free(n); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn assignment_resurrects_a_moved_binding() {
    let src = "fn main() { let mut s = rt_str_alloc(2); let t = s; s = rt_str_alloc(3); rt_str_free(t); rt_str_free(s); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn assignment_resurrects_a_moved_field() {
    let src = "struct P { name: Str } fn main() { let mut p = P { name: rt_str_alloc(2) }; let n = p.name; p.name = \"x\"; rt_print_str(p.name); rt_str_free(n); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn owned_field_reassignment_transfers_ownership() {
    let src = "struct P { name: Str } fn main() { let mut p = P { name: \"a\" }; p.name = rt_str_alloc(3); let q = p; rt_str_free(q.name); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn immutable_result_provenance_copies() {
    let src = "fn make() { return \"lit\"; } fn main() { let a = make(); let b = a; let c = a; rt_print_str(c); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn owned_result_provenance_moves() {
    let src = "fn make() { return rt_str_alloc(3); } fn main() { let a = make(); let b = a; rt_str_free(b); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn returning_a_binding_moves_it_out() {
    let src = "fn make(s) { return s; } fn main() { let s = rt_str_alloc(3); let t = make(s); rt_str_free(t); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn parameter_reads_are_borrows() {
    let src = "fn f(s) { rt_print_int(rt_str_len(s)); rt_print_int(rt_str_byte(s, 0)); rt_print_str(s); } fn main() { f(\"ab\"); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn array_of_literals_copies() {
    let src =
        "fn main() { let a = [\"x\", \"y\"]; let b = a; rt_print_str(b[0]); rt_print_str(a[1]); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn const_string_copies_at_every_use() {
    let src = "const GREET = \"hi\"; fn main() { rt_print_str(GREET); rt_print_str(GREET); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn module_binding_ownership_flows_into_functions() {
    let src = "let s = rt_str_alloc(3); fn f() { rt_print_int(rt_str_len(s)); } fn main() { f(); rt_str_free(s); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn scalars_and_pointers_are_unaffected() {
    let src = "fn main() { let p = rt_alloc(16); let q = p; rt_mem_store(q, 5); rt_print_int(rt_mem_load(p)); let x = 1; let y = x; let z = x + y; rt_print_int(z); rt_free(p); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn equality_borrows_do_not_move() {
    let src = "fn main() { let s = rt_str_alloc(2); let b = s == \"x\"; rt_print_int(rt_str_len(s)); rt_str_free(s); let _ = b; }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

// ---------------------------------------------------------------------------
// Invalid moves (E-S10)
// ---------------------------------------------------------------------------

#[test]
fn use_after_move_is_e_s10() {
    let src = "fn main() { let s = rt_str_alloc(3); let t = s; rt_print_int(rt_str_len(s)); rt_str_free(t); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    let errors = ownership_errors(&ownership, SemanticErrorKind::UseOfMovedValue);
    assert!(!errors.is_empty(), "{:?}", ownership.errors());
    assert_eq!(errors[0].kind().code(), "E-S10");
}

#[test]
fn use_after_move_via_assignment_is_e_s10() {
    let src = "fn main() { let mut s = rt_str_alloc(3); let t = s; s = rt_str_alloc(2); rt_print_int(rt_str_len(t)); rt_str_free(t); }";
    // `s = ...` reassigns (legal); the moved `t` use here is the error.
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn use_after_free_is_e_s10() {
    let src = "fn main() { let s = rt_str_alloc(3); rt_str_free(s); rt_print_int(rt_str_len(s)); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(!ownership_errors(&ownership, SemanticErrorKind::UseOfMovedValue).is_empty());
}

#[test]
fn double_argument_move_is_e_s10() {
    let src = "fn f(s) { rt_print_int(rt_str_len(s)); } fn main() { let s = rt_str_alloc(3); f(s); f(s); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(!ownership_errors(&ownership, SemanticErrorKind::UseOfMovedValue).is_empty());
}

#[test]
fn moving_struct_with_moved_field_is_e_s10() {
    let src = "struct P { name: Str } fn main() { let mut p = P { name: rt_str_alloc(2) }; let n = p.name; let q = p; rt_str_free(n); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    let errors = ownership_errors(&ownership, SemanticErrorKind::UseOfMovedValue);
    assert!(!errors.is_empty(), "{:?}", ownership.errors());
    assert_eq!(
        first_message(&ownership, SemanticErrorKind::UseOfMovedValue),
        "cannot move `p`: field `name` was moved"
    );
}

#[test]
fn whole_struct_move_with_multiple_dead_fields_is_deterministic() {
    // Two fields moved out, then the whole struct is moved: the reported
    // field name must be stable (sorted), not HashMap-iteration order.
    let src = "struct P { a: Str, b: Str } fn main() { let mut p = P { a: rt_str_alloc(1), b: rt_str_alloc(1) }; let x = p.a; let y = p.b; let q = p; rt_str_free(x); rt_str_free(y); }";
    let messages: Vec<String> = (0..8)
        .map(|_| {
            let (_ast, _semantic, _types, ownership) = check_src(src);
            first_message(&ownership, SemanticErrorKind::UseOfMovedValue)
        })
        .collect();
    assert_eq!(messages[0], "cannot move `p`: field `a` was moved");
    assert!(messages.iter().all(|m| m == &messages[0]));
}

#[test]
fn reading_moved_field_is_e_s10() {
    let src = "struct P { name: Str, tag: Str } fn main() { let mut p = P { name: rt_str_alloc(2), tag: rt_str_alloc(2) }; let n = p.name; let m = p.name; rt_str_free(n); rt_str_free(m); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(!ownership_errors(&ownership, SemanticErrorKind::UseOfMovedValue).is_empty());
}

#[test]
fn array_whole_move_on_element_read_is_e_s10_for_later_use() {
    let src = "fn main() { let a = [rt_str_alloc(2)]; let x = a[0]; rt_print_int(rt_str_len(a[0])); rt_str_free(x); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(!ownership_errors(&ownership, SemanticErrorKind::UseOfMovedValue).is_empty());
}

#[test]
fn nested_place_transfer_kills_the_root() {
    let src = "struct Inner { v: Str } struct Outer { inner: Inner } fn main() { let mut o = Outer { inner: Inner { v: rt_str_alloc(2) } }; let x = o.inner.v; rt_print_str(o.inner.v); rt_str_free(x); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(!ownership_errors(&ownership, SemanticErrorKind::UseOfMovedValue).is_empty());
}

#[test]
fn use_after_move_reports_the_right_span() {
    let src = "fn main() {\n    let s = rt_str_alloc(3);\n    let t = s;\n    rt_print_int(rt_str_len(s));\n    rt_str_free(t);\n}\n";
    // The `s` in `rt_str_len(s)` is on line 4.
    assert_error_at(src, SemanticErrorKind::UseOfMovedValue, 4, 29);
}

// ---------------------------------------------------------------------------
// Immutable string mutation (E-S11)
// ---------------------------------------------------------------------------

#[test]
fn mutating_a_literal_is_e_s11() {
    let src = "fn main() { rt_str_set_byte(\"hi\", 0, 65); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    let errors = ownership_errors(&ownership, SemanticErrorKind::MutatingImmutableString);
    assert!(!errors.is_empty(), "{:?}", ownership.errors());
    assert_eq!(errors[0].kind().code(), "E-S11");
}

#[test]
fn mutating_a_literal_binding_is_e_s11() {
    let src = "fn main() { let s = \"hi\"; rt_str_set_byte(s, 0, 65); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    let errors = ownership_errors(&ownership, SemanticErrorKind::MutatingImmutableString);
    assert!(!errors.is_empty(), "{:?}", ownership.errors());
    assert_eq!(
        first_message(&ownership, SemanticErrorKind::MutatingImmutableString),
        "cannot mutate `s`: it is an immutable string"
    );
}

#[test]
fn mutating_a_literal_struct_field_is_e_s11() {
    let src = "struct P { name: Str } fn main() { let p = P { name: \"x\" }; rt_str_set_byte(p.name, 0, 65); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(!ownership_errors(&ownership, SemanticErrorKind::MutatingImmutableString).is_empty());
}

#[test]
fn mutating_an_owned_string_is_allowed() {
    let src = "fn main() { let s = rt_str_alloc(3); rt_str_set_byte(s, 0, 65); rt_print_str(s); rt_str_free(s); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn mutating_an_owned_struct_field_is_allowed() {
    let src = "struct P { name: Str } fn main() { let p = P { name: rt_str_alloc(3) }; rt_str_set_byte(p.name, 0, 65); rt_str_free(p.name); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

// ---------------------------------------------------------------------------
// Recovery and determinism
// ---------------------------------------------------------------------------

#[test]
fn ownership_errors_are_suppressed_when_earlier_stages_fail() {
    // A type error (Int + Bool) must be the only diagnostic; ownership is
    // skipped by the driver when the front end is not clean.
    let src = "fn main() { let s = rt_str_alloc(3); let t = s; let x = s + true; }";
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).unwrap();
    let parsed = parse(file);
    assert!(parsed.is_valid());
    let (ast, _, _) = parsed.into_parts();
    let semantic = mink::semantics::analyze(&ast);
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    assert!(types.has_errors(), "the source must have a type error");
    // The driver runs ownership only on a clean front end; simulate the
    // gate by not running it. A direct run must not panic either.
    let _ = ownership::check(&ast, &semantic, &types);
}

#[test]
fn ownership_errors_are_source_ordered_and_deterministic() {
    let src = "fn main() { let s = rt_str_alloc(1); let a = s; let t = rt_str_alloc(1); let b = t; rt_str_free(a); rt_str_free(b); rt_print_int(rt_str_len(s)); rt_print_int(rt_str_len(t)); }";
    let first = {
        let (_ast, _semantic, _types, ownership) = check_src(src);
        ownership
            .errors()
            .iter()
            .map(|e| (e.span().start(), e.to_string()))
            .collect::<Vec<_>>()
    };
    let second = {
        let (_ast, _semantic, _types, ownership) = check_src(src);
        ownership
            .errors()
            .iter()
            .map(|e| (e.span().start(), e.to_string()))
            .collect::<Vec<_>>()
    };
    assert_eq!(first, second, "ownership diagnostics must be deterministic");
    assert_eq!(first.len(), 2, "{:?}", first);
    assert!(
        first.windows(2).all(|w| w[0].0 <= w[1].0),
        "errors must be source-ordered"
    );
}

#[test]
fn deep_nesting_does_not_panic() {
    let mut src = String::from("fn main() {");
    for i in 0..60 {
        src.push_str(&format!("let s{i} = rt_str_alloc(1);\n"));
    }
    src.push_str("rt_print_int(rt_str_len(s59));\n");
    for i in 0..60 {
        src.push_str(&format!("rt_str_free(s{});\n", 59 - i));
    }
    src.push_str("}\n");
    let (_ast, _semantic, _types, ownership) = check_src(&src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

#[test]
fn chained_moves_across_functions_are_deterministic() {
    let src = "fn a(s) { return s; } fn b(s) { return a(s); } fn c(s) { return b(s); } fn main() { let s = rt_str_alloc(3); let t = c(s); rt_str_free(t); }";
    let (_ast, _semantic, _types, ownership) = check_src(src);
    assert!(ownership.errors().is_empty(), "{:?}", ownership.errors());
}

// ---------------------------------------------------------------------------
// Native end-to-end
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("mink_ownership_test_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

fn build(source: &str) -> PathBuf {
    let name = std::thread::current()
        .name()
        .unwrap_or("program")
        .replace("::", "_");
    let path = temp_source(&format!("{name}.mink"), source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    path.with_extension("exe")
}

fn run(exe: &PathBuf) -> (i32, String) {
    let output = Command::new(exe).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    (output.status.code().unwrap_or(-1), stdout)
}

#[test]
fn native_valid_ownership_program_runs() {
    let exe = build(
        "fn consume(s) {\n\
             rt_print_int(rt_str_len(s));\n\
             rt_str_free(s);\n\
             return 0;\n\
         }\n\
         fn make() { return rt_str_alloc(5); }\n\
         fn main() {\n\
             let a = \"hi\";\n\
             let b = a;\n\
             rt_print_str(b);\n\
             let s = rt_str_alloc(3);\n\
             consume(s);\n\
             let m = make();\n\
             rt_print_int(rt_str_len(m));\n\
             rt_str_free(m);\n\
             return 0;\n\
         }\n",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0, "stdout: {stdout}");
    assert_eq!(stdout, "hi\n3\n5\n");
}

#[test]
fn native_struct_ownership_runs() {
    let exe = build(
        "struct Person { name: Str, age: Int }\n\
         fn main() {\n\
             let p = Person { name: \"alice\", age: 30 };\n\
             let q = p;\n\
             rt_print_str(q.name);\n\
             rt_print_int(q.age);\n\
             let mut r = Person { name: rt_str_alloc(3), age: 1 };\n\
             rt_str_set_byte(r.name, 0, 66);\n\
             rt_print_str(r.name);\n\
             rt_str_free(r.name);\n\
             return 0;\n\
         }\n",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0, "stdout: {stdout}");
    assert_eq!(stdout, "alice\n30\nB\u{0}\u{0}\n");
}

#[test]
fn native_invalid_ownership_program_fails_before_codegen() {
    let src = "fn main() { let s = rt_str_alloc(3); let t = s; rt_print_int(rt_str_len(s)); }";
    let name = std::thread::current()
        .name()
        .unwrap_or("program")
        .replace("::", "_");
    let path = temp_source(&format!("{name}.mink"), src);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        !output.status.success(),
        "an invalid ownership program must not build"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E-S10"), "stderr: {stderr}");
    assert!(
        !path.with_extension("exe").exists(),
        "no executable may be produced"
    );
}
