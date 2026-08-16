//! Integration tests for the Session 16 explicit references/borrowing
//! foundation: reference types (`&T` / `&mut T`), borrow expressions,
//! dereference, lexical borrow lifetimes, borrow conflicts (E-S12),
//! invalid borrows (E-S13), dangling references (E-S14), the type-level
//! rules (E-T19/E-T20/E-T21), pointer/reference distinction, and native
//! end-to-end execution of valid borrowing programs.
//!
//! The frozen rules are documented in
//! `docs/implementation/BORROWING_IMPLEMENTATION.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

use mink::ast::Ast;
use mink::ownership::{self, OwnershipResult};
use mink::parser::parse;
use mink::semantics::{SemanticErrorKind, SemanticResult};
use mink::source::{SourceMap, Span};
use mink::typecheck::{TypeErrorKind, TypeResult};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses, semantically analyzes, type-checks, and ownership-analyzes
/// `src`, asserting that it lexes/parses cleanly. Type errors are returned
/// so callers can assert on them; ownership errors likewise.
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
    let ownership = ownership::check(&ast, &semantic, &types);
    (ast, semantic, types, ownership)
}

/// The first type error of `kind`, if any.
fn first_type_error(
    types: &TypeResult,
    kind: TypeErrorKind,
) -> Option<&mink::typecheck::TypeError> {
    types.errors().iter().find(|error| error.kind() == kind)
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

/// Computes the 1-based line/column of a span start.
fn line_col(src: &str, span: Span) -> (usize, usize) {
    let start = span.start() as usize;
    let before = &src[..start];
    let line = before.matches('\n').count() + 1;
    let col = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, col)
}

/// Asserts `src` has exactly one ownership error of `kind` at `line:col`.
fn assert_ownership_error_at(src: &str, kind: SemanticErrorKind, line: usize, col: usize) {
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

/// Asserts `src` has exactly one type error of `kind` at `line:col`.
fn assert_type_error_at(src: &str, kind: TypeErrorKind, line: usize, col: usize) {
    let (_ast, _semantic, types, _ownership) = check_src(src);
    let error = first_type_error(&types, kind)
        .unwrap_or_else(|| panic!("expected {kind:?} in {src:?}, got {:?}", types.errors()));
    let (actual_line, actual_col) = line_col(src, error.span());
    assert_eq!(
        (actual_line, actual_col),
        (line, col),
        "span of {kind:?} in {src:?}"
    );
}

/// Asserts `src` produces no type or ownership errors.
fn assert_clean(src: &str) {
    let (_ast, _semantic, types, ownership) = check_src(src);
    assert!(
        types.errors().is_empty(),
        "type errors: {:?}",
        types.errors()
    );
    assert!(
        ownership.errors().is_empty(),
        "ownership errors: {:?}",
        ownership.errors()
    );
}

// ---------------------------------------------------------------------------
// Native end-to-end helpers
// ---------------------------------------------------------------------------

/// Returns a `Command` for the compiled `mink` binary.
fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

/// Writes `content` to a uniquely named temp file and returns its path.
fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "mink_references_test_{}_{name}",
        std::process::id()
    ));
    std::fs::write(&path, content).unwrap();
    path
}

/// Builds `source` with the compiler and returns the generated executable.
fn build(source: &str) -> PathBuf {
    let name = std::thread::current()
        .name()
        .unwrap_or("program")
        .replace("::", "_");
    let path = temp_source(&format!("{name}.mink"), source);
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert!(
        output.status.success(),
        "build failed: stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout).trim(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");
    exe
}

/// Runs `exe` and returns (exit code, stdout with `\r\n` normalized to
/// `\n` and trimmed).
fn run(exe: &PathBuf) -> (i32, String) {
    let output = Command::new(exe).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout)
        .replace("\r\n", "\n")
        .trim()
        .to_string();
    (output.status.code().unwrap_or(-1), stdout)
}

// ---------------------------------------------------------------------------
// Reference types and valid borrow programs
// ---------------------------------------------------------------------------

#[test]
fn shared_borrow_reads_through() {
    assert_clean("fn main() { let mut v = 41; let r = &v; rt_print_int(*r); return 0; }");
}

#[test]
fn multiple_shared_borrows_coexist() {
    assert_clean(
        "fn main() { let mut v = 41; let r1 = &v; let r2 = &v; let r3 = &v; \
         rt_print_int(*r1 + *r2 + *r3); return 0; }",
    );
}

#[test]
fn shared_references_copy_freely() {
    assert_clean(
        "fn main() { let v = 41; let r = &v; let x = r; let y = r; \
         rt_print_int(*x + *y); return 0; }",
    );
}

#[test]
fn mutable_borrow_writes_through() {
    assert_clean(
        "fn main() { let mut v = 41; let w = &mut v; *w = *w + 1; rt_print_int(*w); return 0; }",
    );
}

#[test]
fn borrow_parameter_read_and_write() {
    // Passing a reference to a function holds the borrow for the call;
    // the shared borrow ends with its declaring scope, after which an
    // exclusive borrow of the same source is allowed. (The exclusive
    // reference itself is moved by the call, so the source is read
    // directly after.)
    assert_clean(
        "fn read(r) { rt_print_int(*r); } \
         fn bump(r) { *r = *r + 1; } \
         fn main() { let mut v = 41; if true { let r = &v; read(r); } \
         let w = &mut v; bump(w); rt_print_int(v); return 0; }",
    );
}

#[test]
fn borrow_is_released_when_binding_reassigned() {
    // Reassigning the reference binding to a different source drops the
    // old borrow, so the old source can then be borrowed exclusively.
    assert_clean(
        "fn main() { let mut v = 41; let mut w = 0; let mut r = &v; rt_print_int(*r); \
         r = &w; rt_print_int(*r); let x = &mut v; *x = *x + 1; rt_print_int(*x); return 0; }",
    );
}

#[test]
fn borrow_does_not_outlive_declaring_block() {
    // `if` branches are scopes: a borrow declared inside is released when
    // the branch exits, so a later exclusive borrow is allowed.
    assert_clean(
        "fn main() { let mut v = 41; if true { let r = &v; rt_print_int(*r); } \
         let w = &mut v; *w = *w + 1; rt_print_int(*w); return 0; }",
    );
}

#[test]
fn borrow_does_not_outlive_loop_body() {
    assert_clean(
        "fn main() { let mut v = 0; let mut i = 0; \
         while i < 3 { let r = &v; rt_print_int(*r); i = i + 1; } \
         let w = &mut v; *w = 42; rt_print_int(*w); return 0; }",
    );
}

#[test]
fn reference_returned_from_parameter_propagates() {
    assert_clean(
        "fn identity(r) { return r; } \
         fn main() { let mut v = 41; let r = &v; let y = identity(r); rt_print_int(*y); return 0; }",
    );
}

#[test]
fn mutable_reference_returned_from_parameter_propagates() {
    assert_clean(
        "fn identity(r) { return r; } \
         fn main() { let mut v = 41; let w = &mut v; let y = identity(w); *y = *y + 1; \
         rt_print_int(*y); return 0; }",
    );
}

#[test]
fn reference_to_struct_field() {
    assert_clean(
        "struct P { x: Int, tag: Bool } \
         fn main() { let mut p = P { x: 41, tag: true }; let r = &p.x; \
         rt_print_int(*r); let r2 = &p.x; rt_print_int(*r2); return 0; }",
    );
}

#[test]
fn mutable_reference_to_struct_field_writes() {
    assert_clean(
        "struct P { x: Int, tag: Bool } \
         fn bump(r) { *r = *r + 1; } \
         fn main() { let mut p = P { x: 41, tag: true }; let w = &mut p.x; bump(w); \
         let r = &p.x; rt_print_int(*r); return 0; }",
    );
}

#[test]
fn struct_with_reference_field() {
    assert_clean(
        "struct S { r: &Int } \
         fn main() { let v = 41; let s = S { r: &v }; rt_print_int(*s.r); return 0; }",
    );
}

#[test]
fn array_of_references() {
    assert_clean(
        "fn main() { let mut v = 41; let a = [&v, &v]; rt_print_int(*a[0] + *a[1]); return 0; }",
    );
}

#[test]
fn reference_to_pointer_is_distinct() {
    // A reference to a pointer dereferences to the pointer, which is then
    // usable with the memory intrinsics.
    assert_clean(
        "fn main() { let mut p = rt_alloc(16); rt_mem_store(p, 7); let r = &p; \
         let q = *r; rt_print_int(rt_mem_load(q)); rt_free(p); return 0; }",
    );
}

#[test]
fn reference_to_string_reads_through() {
    assert_clean("fn main() { let s = \"hello\"; let r = &s; rt_print_str(*r); return 0; }");
}

#[test]
fn string_mutation_through_mut_reference() {
    assert_clean(
        "fn set_first(s) { rt_str_set_byte(s, 0, 66); } \
         fn main() { let mut s = rt_str_alloc(3); rt_str_set_byte(s, 0, 65); \
         let r = &mut s; set_first(*r); rt_print_str(s); rt_str_free(s); return 0; }",
    );
}

#[test]
fn recursion_with_references() {
    assert_clean(
        "fn sum_to(n, acc) { if n == 0 { return acc; } return sum_to(n - 1, acc + n); } \
         fn main() { let mut total = 0; let r = &total; rt_print_int(*r); \
         return sum_to(10, 0); }",
    );
}

#[test]
fn nested_scopes_chain_borrows() {
    assert_clean(
        "fn main() { let mut v = 41; \
         if true { let r1 = &v; rt_print_int(*r1); \
         if true { let r2 = &v; rt_print_int(*r2); } } \
         let w = &mut v; *w = *w + 1; rt_print_int(*w); return 0; }",
    );
}

#[test]
fn owned_string_and_reference_interact() {
    // An owned string can be borrowed read-only inside a scope and then
    // freed after the scope releases the borrow.
    assert_clean(
        "fn main() { let mut s = rt_str_alloc(3); rt_str_set_byte(s, 0, 65); \
         if true { let r = &s; rt_print_int(rt_str_byte(s, 0)); } rt_str_free(s); return 0; }",
    );
}

// ---------------------------------------------------------------------------
// Type-level errors: E-T19 / E-T20 / E-T21
// ---------------------------------------------------------------------------

#[test]
fn borrow_of_literal_is_e_t19() {
    assert_type_error_at(
        "fn main() { let x = &5; return 0; }",
        TypeErrorKind::InvalidBorrowTarget,
        1,
        21,
    );
}

#[test]
fn borrow_of_computed_value_is_e_t19() {
    assert_type_error_at(
        "fn main() { let x = 5; let r = &(x + 1); return 0; }",
        TypeErrorKind::InvalidBorrowTarget,
        1,
        32,
    );
}

#[test]
fn borrow_of_reference_is_e_t19() {
    assert_type_error_at(
        "fn main() { let v = 41; let r = &v; let s = &r; return 0; }",
        TypeErrorKind::InvalidBorrowTarget,
        1,
        45,
    );
}

#[test]
fn ref_to_ref_type_syntax_is_rejected() {
    // `&&Int` lexes as the logical-and token, so the type syntax does not
    // parse; the program is rejected before code generation (never a
    // panic).
    let src = "fn main() { let r: &Int = &1; let s: &&Int = r; return 0; }";
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).expect("the file just added");
    let parsed = parse(file);
    assert!(!parsed.is_valid(), "`&&Int` type syntax must be rejected");
}

#[test]
fn deref_of_non_reference_is_e_t20() {
    assert_type_error_at(
        "fn main() { let x = 5; let y = *x; return 0; }",
        TypeErrorKind::DerefNonReference,
        1,
        32,
    );
}

#[test]
fn deref_of_pointer_is_e_t20() {
    // `Ptr<T>` is not a reference: dereferencing it with `*` is a type
    // error (pointers are accessed via the memory intrinsics).
    assert_type_error_at(
        "fn main() { let p = rt_alloc(16); let q = *p; return 0; }",
        TypeErrorKind::DerefNonReference,
        1,
        43,
    );
}

#[test]
fn assign_through_immutable_reference_is_e_t21() {
    assert_type_error_at(
        "fn main() { let v = 41; let r = &v; *r = 42; return 0; }",
        TypeErrorKind::AssignThroughImmutableRef,
        1,
        37,
    );
}

#[test]
fn borrow_of_unit_enum_is_e_t19() {
    // Audit regression: enums are not reference element types (the Session
    // 16 model covers `Int`/`Bool`/`Str`/`Ptr`/structs/arrays). Previously
    // `&e` reached the backend and died with an internal E-B07 instead of
    // a front-end diagnostic.
    assert_type_error_at(
        "enum E { A, B } fn main() { let e = E::A; let r = &e; return 0; }",
        TypeErrorKind::InvalidBorrowTarget,
        1,
        51,
    );
}

#[test]
fn borrow_of_tagged_enum_is_e_t19() {
    assert_type_error_at(
        "enum E { A, B(Int) } fn main() { let e = E::B(1); let r = &e; return 0; }",
        TypeErrorKind::InvalidBorrowTarget,
        1,
        59,
    );
}

#[test]
fn borrow_of_enum_field_is_e_t19() {
    // Borrowing an enum-typed field is rejected the same way (its type is
    // the enum type).
    assert_type_error_at(
        "enum E { A, B } struct S { e: E } fn main() { let s = S { e: E::A }; let r = &s.e; return 0; }",
        TypeErrorKind::InvalidBorrowTarget,
        1,
        78,
    );
}

#[test]
fn deref_rooted_member_assignment_is_e_t33() {
    // Audit regression: `(*r).x = v` used to lower to a write into a
    // temporary copy of the dereferenced value, silently dropping the
    // assignment. It is now rejected (E-T33) because only whole-value
    // deref assignment (`*r = v`) is in the reference model.
    assert_type_error_at(
        "struct S { tag: Int } fn main() { let mut s = S { tag: 1 }; let r = &mut s; (*r).tag = 9; return 0; }",
        TypeErrorKind::DerefRootedAssignment,
        1,
        77,
    );
}

#[test]
fn deref_rooted_element_assignment_is_e_t33() {
    assert_type_error_at(
        "fn main() { let mut a = [1, 2]; let r = &mut a; (*r)[0] = 9; return 0; }",
        TypeErrorKind::DerefRootedAssignment,
        1,
        49,
    );
}

#[test]
fn deref_rooted_compound_assignment_is_e_t33() {
    assert_type_error_at(
        "struct S { tag: Int } fn main() { let mut s = S { tag: 1 }; let r = &mut s; (*r).tag += 1; return 0; }",
        TypeErrorKind::DerefRootedAssignment,
        1,
        77,
    );
}

// ---------------------------------------------------------------------------
// Borrow-check errors: E-S12 / E-S13 / E-S14
// ---------------------------------------------------------------------------

#[test]
fn conflicting_mutable_borrows_are_e_s12() {
    assert_ownership_error_at(
        "fn main() { let mut v = 41; let r = &mut v; let r2 = &mut v; return 0; }",
        SemanticErrorKind::BorrowConflict,
        1,
        59,
    );
}

#[test]
fn shared_borrow_after_mutable_borrow_is_e_s12() {
    assert_ownership_error_at(
        "fn main() { let mut v = 41; let r = &mut v; let r2 = &v; return 0; }",
        SemanticErrorKind::BorrowConflict,
        1,
        55,
    );
}

#[test]
fn mutable_borrow_while_shared_borrowed_is_e_s12() {
    assert_ownership_error_at(
        "fn main() { let mut v = 41; let r = &v; let w = &mut v; return 0; }",
        SemanticErrorKind::BorrowConflict,
        1,
        54,
    );
}

#[test]
fn assignment_while_borrowed_is_e_s12() {
    assert_ownership_error_at(
        "fn main() { let mut v = 41; let r = &v; v = 42; return 0; }",
        SemanticErrorKind::BorrowConflict,
        1,
        41,
    );
}

#[test]
fn move_while_borrowed_is_e_s12() {
    assert_ownership_error_at(
        "fn main() { let s = rt_str_alloc(4); let r = &s; rt_str_free(s); return 0; }",
        SemanticErrorKind::BorrowConflict,
        1,
        62,
    );
}

#[test]
fn mutation_while_borrowed_is_e_s12() {
    assert_ownership_error_at(
        "fn main() { let s = rt_str_alloc(4); let r = &s; rt_str_set_byte(s, 0, 65); return 0; }",
        SemanticErrorKind::BorrowConflict,
        1,
        66,
    );
}

#[test]
fn use_of_mutably_borrowed_binding_is_e_s12() {
    assert_ownership_error_at(
        "fn main() { let mut v = 41; let w = &mut v; rt_print_int(v); return 0; }",
        SemanticErrorKind::BorrowConflict,
        1,
        58,
    );
}

#[test]
fn root_level_conflict_through_field_is_e_s12() {
    // `&p.x` borrows the whole root `p`; a later `&mut p` conflicts.
    assert_ownership_error_at(
        "struct P { x: Int } fn main() { let mut p = P { x: 1 }; let r = &p.x; let w = &mut p; return 0; }",
        SemanticErrorKind::BorrowConflict,
        1,
        84,
    );
}

#[test]
fn mutable_borrow_of_immutable_binding_is_e_s13() {
    assert_ownership_error_at(
        "fn main() { let v = 41; let r = &mut v; return 0; }",
        SemanticErrorKind::InvalidBorrow,
        1,
        38,
    );
}

#[test]
fn borrow_of_constant_is_e_s13() {
    assert_ownership_error_at(
        "const GREET = \"hi\"; fn main() { let r = &GREET; return 0; }",
        SemanticErrorKind::InvalidBorrow,
        1,
        42,
    );
}

#[test]
fn returning_reference_to_local_is_e_s14() {
    assert_ownership_error_at(
        "fn main() { let v = 41; let r = &v; return r; }",
        SemanticErrorKind::DanglingReference,
        1,
        44,
    );
}

#[test]
fn returning_struct_with_local_reference_is_e_s14() {
    assert_ownership_error_at(
        "struct S { r: &Int } fn make() { let v = 41; let s = S { r: &v }; return s; } fn main() { return 0; }",
        SemanticErrorKind::DanglingReference,
        1,
        74,
    );
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn borrow_errors_are_deterministic_and_source_ordered() {
    // Multiple conflicts in one program are reported in source order with
    // stable codes, never in hash-map order.
    let src = "fn main() { \
               let mut v = 41; \
               let r1 = &mut v; \
               let r2 = &mut v; \
               let r3 = &mut v; \
               return 0; }";
    let (_, _, _, first) = check_src(src);
    let (_, _, _, second) = check_src(src);
    let first_codes: Vec<String> = first
        .errors()
        .iter()
        .map(|e| format!("{:?}:{}:{}", e.kind(), e.span().start(), e.span().end()))
        .collect();
    let second_codes: Vec<String> = second
        .errors()
        .iter()
        .map(|e| format!("{:?}:{}:{}", e.kind(), e.span().start(), e.span().end()))
        .collect();
    assert_eq!(first_codes, second_codes);
    assert_eq!(first_codes.len(), 2, "two conflicts: {:?}", first_codes);
    // The first conflict precedes the second in source order.
    let starts: Vec<u32> = first.errors().iter().map(|e| e.span().start()).collect();
    assert!(starts[0] < starts[1]);
}

#[test]
fn mixed_type_and_borrow_errors_are_deterministic() {
    let src = "fn main() { \
               let v = 41; \
               let r = &mut v; \
               let s = *v; \
               let t = &(v + 1); \
               return 0; }";
    let (_, _, types, ownership) = check_src(src);
    let mut kinds = Vec::new();
    for e in types.errors() {
        kinds.push(format!("T:{:?}:{}", e.kind(), e.span().start()));
    }
    for e in ownership.errors() {
        kinds.push(format!("S:{:?}:{}", e.kind(), e.span().start()));
    }
    let (_, _, types2, ownership2) = check_src(src);
    let mut kinds2 = Vec::new();
    for e in types2.errors() {
        kinds2.push(format!("T:{:?}:{}", e.kind(), e.span().start()));
    }
    for e in ownership2.errors() {
        kinds2.push(format!("S:{:?}:{}", e.kind(), e.span().start()));
    }
    assert_eq!(kinds, kinds2);
}

// ---------------------------------------------------------------------------
// Malformed / adversarial inputs
// ---------------------------------------------------------------------------

#[test]
fn borrow_syntax_is_rejected_cleanly() {
    // `&&` in binary position stays bitwise-and; `& &v` parses as a
    // nested unary borrow and is a type error, never a crash.
    let src = "fn main() { let v = 1; let r = & &v; return 0; }";
    let (_ast, _semantic, types, ownership) = check_src(src);
    assert!(!types.errors().is_empty() || !ownership.errors().is_empty());
}

#[test]
fn deref_of_unresolved_expression_is_clean() {
    // `*` on an expression that fails to resolve is rejected (the
    // semantic stage reports the unresolved name first), never a panic in
    // the checker or borrow analyzer.
    let src = "fn main() { let x = *missing; return 0; }";
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).expect("the file just added");
    let parsed = parse(file);
    assert!(parsed.is_valid());
    let (ast, _, _) = parsed.into_parts();
    let semantic = mink::semantics::analyze(&ast);
    assert!(
        !semantic.errors().is_empty(),
        "the unresolved name must be reported"
    );
}

#[test]
fn dangling_reference_in_loop_is_rejected() {
    // A borrow declared in a loop body cannot be returned (it dies when
    // the body exits), so the `return` is a dangling reference.
    let src = "fn f() { while true { let v = 1; let r = &v; return r; } return 0; } fn main() { return 0; }";
    let (_ast, _semantic, types, ownership) = check_src(src);
    let dangling = ownership_errors(&ownership, SemanticErrorKind::DanglingReference);
    assert!(
        !dangling.is_empty(),
        "ownership errors: {:?}",
        ownership.errors()
    );
    let _ = types;
}

#[test]
fn conflicting_borrow_in_else_branch_is_rejected() {
    let src = "fn main() { let mut v = 41; \
               if true { let r = &v; rt_print_int(*r); } \
               else { let w = &mut v; rt_print_int(*w); } \
               let w2 = &mut v; return 0; }";
    // The else branch's exclusive borrow is released at branch exit, so
    // the final `&mut v` is clean; the shared borrow in the then branch
    // never coexists with the else branch's exclusive borrow (linear
    // walk), matching the deterministic branch-merge model.
    assert_clean(src);
}

// ---------------------------------------------------------------------------
// Native end-to-end
// ---------------------------------------------------------------------------

#[test]
fn native_shared_and_mutable_borrows() {
    let exe = build(
        "fn read(r) { rt_print_int(*r); } \
         fn bump(r) { *r = *r + 1; } \
         fn main() { \
           let mut v = 41; \
           if true { let r1 = &v; let r2 = &v; read(r1); read(r2); } \
           let w = &mut v; \
           bump(w); \
           rt_print_int(v); \
           return 0; \
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(stdout, "41\n41\n42");
    assert_eq!(code, 0);
}

#[test]
fn native_reference_return_chain() {
    let exe = build(
        "fn identity(r) { return r; } \
         fn main() { \
           let mut v = 41; \
           let r = &v; \
           let y = identity(r); \
           rt_print_int(*y); \
           let r2 = &v; \
           let z = identity(r2); \
           rt_print_int(*z); \
           return 0; \
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(stdout, "41\n41");
    assert_eq!(code, 0);
}

#[test]
fn native_struct_field_borrow() {
    let exe = build(
        "struct P { x: Int, tag: Bool } \
         fn bump(r) { *r = *r + 1; } \
         fn main() { \
           let mut p = P { x: 41, tag: true }; \
           if true { let r = &p.x; rt_print_int(*r); } \
           let w = &mut p.x; \
           bump(w); \
           let r2 = &p.x; \
           rt_print_int(*r2); \
           return 0; \
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(stdout, "41\n42");
    assert_eq!(code, 0);
}

#[test]
fn native_deref_rooted_member_read() {
    // Audit regression: reading through a deref-rooted member (`(*r).x`)
    // is supported and copies the dereferenced value; only *assignment*
    // through such places is rejected (E-T33).
    let exe = build(
        "enum E { A, B(Int) } \
         struct S { e: E, tag: Int } \
         fn tag_of(r) { match (*r).e { E::B(x) => { return x; }, E::A => { return 0; } } } \
         fn main() { \
           let s = S { e: E::B(7), tag: 1 }; \
           let r = &s; \
           rt_print_int(tag_of(r)); \
           return 0; \
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(stdout, "7");
    assert_eq!(code, 0);
}

#[test]
fn native_array_of_references() {
    let exe = build(
        "fn main() { \
           let mut v = 41; \
           let a = [&v, &v]; \
           rt_print_int(*a[0] + *a[1]); \
           return 0; \
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(stdout, "82");
    assert_eq!(code, 0);
}

#[test]
fn native_reference_to_string() {
    let exe = build(
        "fn main() { \
           let s = \"hello\"; \
           let r = &s; \
           rt_print_str(*r); \
           return 0; \
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(stdout, "hello");
    assert_eq!(code, 0);
}

#[test]
fn native_invalid_borrow_program_fails_before_codegen() {
    // A program with a borrow conflict must be rejected before code
    // generation: the build fails and no executable is produced.
    let path = temp_source(
        "conflict.mink",
        "fn main() { let mut v = 41; let r = &mut v; let r2 = &mut v; return 0; }",
    );
    let output = mink().arg("build").arg(&path).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("E-S12") || stderr.contains("E-T19"),
        "stderr was: {stderr}"
    );
    let exe = path.with_extension("exe");
    assert!(!exe.exists(), "no executable should be produced");
}
