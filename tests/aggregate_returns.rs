//! Integration tests for the session 22 milestone: **aggregate returns and
//! module-scope aggregate statics**.
//!
//! Functions may now return structs, arrays, and tagged-union enums (a
//! multi-word result is returned through a caller-allocated return slot),
//! and module-scope `let`/`const` bindings may hold aggregate values
//! (decoded into the image's data region). This lifts the session-14/19
//! `E-B03` rejection while keeping `Range` returns rejected, keeping
//! string/pointer/reference statics rejected, and rejecting aggregate
//! `main` results (`E-B09`). Mutable module aggregates are written through
//! read-modify-write place stores, so `a[i] = v` and `g.rows[1].y = v`
//! reach the binding.
//!
//! The rules under test are documented in
//! `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md` (session 22
//! section), `docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md`, and
//! `docs/implementation/SUM_TYPES_IMPLEMENTATION.md`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use mink::backend::{self, BInstKind, BType};
use mink::driver;
use mink::mir::{MirItemKind, MirPlaceRoot, MirTargetKind};
use mink::source::SourceMap;
use mink::typecheck::TypeResult;

/// A unique per-call suffix, so tests running in parallel never share a
/// temp source file.
static COUNTER: AtomicU32 = AtomicU32::new(0);

fn unique_source(kind: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "mink_aggregate_returns_{kind}_{}_{n}.mink",
        std::process::id()
    ))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses, semantically analyzes, and type-checks `src`, returning the type
/// result (for front-end-level assertions).
fn check_src(src: &str) -> (SourceMap, TypeResult) {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).expect("the file just added");
    let parsed = mink::parser::parse(file);
    assert!(
        parsed.is_valid(),
        "test source must lex and parse cleanly\nlex errors: {:?}\nparse errors: {:?}",
        parsed.lex_errors(),
        parsed.parse_errors()
    );
    let (ast, lex_errors, parse_errors) = parsed.into_parts();
    assert!(lex_errors.is_empty() && parse_errors.is_empty());
    let semantic = mink::semantics::analyze(&ast);
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    (sources, types)
}

/// Runs the front end on `src` and lowers the optimized MIR into backend
/// instructions, asserting every stage is clean.
fn lower_backend(src: &str) -> (mink::mir::MirProgram, mink::backend::BProgram) {
    let mut sources = SourceMap::new();
    let path = unique_source("test");
    std::fs::write(&path, src).unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        report.errors.is_empty(),
        "the source itself must be valid MINK: {:?}",
        report.errors
    );
    let optimized = report.mir.expect("clean program lowers to MIR");
    let program = backend::lower(&optimized, &sources)
        .unwrap_or_else(|errors| panic!("clean MIR must lower: {errors:?}"));
    backend::verify(&program).expect("lowering must produce valid instructions");
    (optimized, program)
}

/// The lowered function named `name`.
fn function<'p>(program: &'p mink::backend::BProgram, name: &str) -> &'p mink::backend::BFunction {
    program
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no function named `{name}`"))
}

/// The backend errors for `src`, or a panic when lowering succeeds.
fn lower_errors(src: &str) -> Vec<mink::backend::BackendError> {
    let mut sources = SourceMap::new();
    let path = unique_source("err");
    std::fs::write(&path, src).unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        report.errors.is_empty(),
        "the source itself must be valid MINK: {:?}",
        report.errors
    );
    let optimized = report.mir.expect("clean program lowers to MIR");
    match backend::lower(&optimized, &sources) {
        Ok(_) => panic!("expected backend errors for: {src}"),
        Err(errors) => errors,
    }
}

fn error_kinds(src: &str) -> Vec<mink::backend::BackendErrorKind> {
    lower_errors(src).iter().map(|e| e.kind()).collect()
}

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "mink_aggregate_returns_{}_{name}",
        std::process::id()
    ));
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
    let exe = path.with_extension("exe");
    assert!(exe.exists(), "no executable produced");
    exe
}

fn run(exe: &PathBuf) -> (i32, Vec<u8>) {
    let output = Command::new(exe).output().unwrap();
    let stdout = output.stdout;
    // The native runtime writes `\n`; the Windows console layer may hand
    // text-mode CRLF through pipes, so normalize before comparing.
    let stdout = if stdout.contains(&b'\r') {
        stdout
            .iter()
            .copied()
            .filter(|b| *b != b'\r')
            .collect::<Vec<u8>>()
    } else {
        stdout
    };
    (output.status.code().unwrap_or(-1), stdout)
}

// ---------------------------------------------------------------------------
// Front end
// ---------------------------------------------------------------------------

#[test]
fn aggregate_returns_pass_the_front_end() {
    // The front end always accepted aggregate returns; the backend rejected
    // them with E-B03. Both struct and tagged-union returns are valid MINK.
    let (_sources, types) =
        check_src("struct P { x: Int } fn make() { return P { x: 1 }; } fn main() { return 0; }");
    assert!(types.errors().is_empty());
    let (_sources, types) =
        check_src("enum E { A, B(Int) } fn make(c) { return E::B(c); } fn main() { return 0; }");
    assert!(types.errors().is_empty());
}

#[test]
fn module_scope_aggregates_pass_the_front_end() {
    let (_sources, types) = check_src(
        "struct P { x: Int, y: Int }
         const ORIGIN = P { x: 1, y: 2 };
         let mut grid = [P { x: 1, y: 2 }, P { x: 3, y: 4 }];
         fn main() { return 0; }",
    );
    assert!(types.errors().is_empty());
}

#[test]
fn array_typed_fields_may_reference_later_structs() {
    // Regression: `array_type` used to validate the element's layout while
    // fields were still being resolved in source order, so an array-typed
    // field referencing a *later* struct was wrongly rejected as an empty
    // struct (E-T18). The eager check defers `Empty`; the full layout
    // validation runs after every struct's fields are set.
    let (_sources, types) = check_src(
        "struct Grid { rows: [Point; 2] }
         struct Point { x: Int, y: Int }
         fn main() { let g = Grid { rows: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }] }; return 0; }",
    );
    assert!(
        types.errors().is_empty(),
        "a forward reference must not be rejected: {:?}",
        types.errors()
    );
}

#[test]
fn genuinely_empty_structs_are_still_e_t18() {
    // The deferred validation still reports a genuinely empty struct, at
    // its own declaration.
    let (_sources, types) =
        check_src("struct A { arr: [B; 1] } struct B { } fn main() { return 0; }");
    assert!(
        types.errors().iter().any(|e| e.kind().code() == "E-T18"),
        "an empty struct must still be E-T18"
    );
}

// ---------------------------------------------------------------------------
// MIR: static-rooted places
// ---------------------------------------------------------------------------

#[test]
fn static_rooted_deep_places_keep_the_root() {
    // `g.rows[1].y = 40` on a module binding lowers to a structural place
    // rooted at module storage, so the assignment reaches the binding
    // instead of a temporary copy of the intermediate value.
    let (mir, _) = lower_backend(
        "struct P { x: Int, y: Int }
         struct Grid { rows: [P; 2] }
         let mut g = Grid { rows: [P { x: 1, y: 2 }, P { x: 3, y: 4 }] };
         fn main() { g.rows[1].y = 40; return 0; }",
    );
    let main = mir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            MirItemKind::Fn(f) if f.name.name == "main" => Some(f),
            _ => None,
        })
        .expect("main exists");
    let has_static_place = main.blocks.iter().flat_map(|b| &b.stmts).any(|stmt| {
        matches!(
            &stmt.kind,
            mink::mir::MirStmtKind::Assign {
                target,
                ..
            } if matches!(
                &target.kind,
                MirTargetKind::Place {
                    root: MirPlaceRoot::Static(_),
                    ..
                }
            )
        )
    });
    assert!(
        has_static_place,
        "a module-rooted deep place must keep a static root"
    );
}

#[test]
fn local_rooted_places_keep_local_roots() {
    // Local-rooted chains are unchanged: the root stays a local.
    let (mir, _) = lower_backend(
        "struct P { x: Int, y: Int }
         struct Grid { rows: [P; 2] }
         fn main() { let mut g = Grid { rows: [P { x: 1, y: 2 }, P { x: 3, y: 4 }] }; g.rows[1].y = 40; return 0; }",
    );
    let main = mir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            MirItemKind::Fn(f) if f.name.name == "main" => Some(f),
            _ => None,
        })
        .expect("main exists");
    let has_local_place = main.blocks.iter().flat_map(|b| &b.stmts).any(|stmt| {
        matches!(
            &stmt.kind,
            mink::mir::MirStmtKind::Assign {
                target,
                ..
            } if matches!(
                &target.kind,
                MirTargetKind::Place {
                    root: MirPlaceRoot::Local(_),
                    ..
                }
            )
        )
    });
    assert!(
        has_local_place,
        "a local-rooted deep place keeps a local root"
    );
}

// ---------------------------------------------------------------------------
// Backend lowering: aggregate returns
// ---------------------------------------------------------------------------

#[test]
fn struct_returns_are_multi_word_results() {
    let (_mir, program) = lower_backend(
        "struct P { x: Int, y: Int } fn make() { return P { x: 1, y: 2 }; } fn main() { let p = make(); return p.x; }",
    );
    let make = function(&program, "make");
    assert_eq!(make.result, BType::Struct);
    assert_eq!(make.result_words, 2, "a two-field struct spans two words");
}

#[test]
fn array_returns_are_multi_word_results() {
    let (_mir, program) =
        lower_backend("fn make() { return [1, 2, 3]; } fn main() { let a = make(); return a[0]; }");
    let make = function(&program, "make");
    assert_eq!(make.result, BType::Array);
    assert_eq!(
        make.result_words, 3,
        "a three-element array spans three words"
    );
}

#[test]
fn tagged_enum_returns_are_multi_word_results() {
    let (_mir, program) = lower_backend(
        "enum E { A, B(Int) } fn make() { return E::B(5); } fn main() { let e = make(); return 0; }",
    );
    let make = function(&program, "make");
    assert_eq!(make.result, BType::Enum);
    assert_eq!(make.result_words, 2, "a tagged union spans two words");
}

#[test]
fn unit_only_enum_returns_stay_single_word() {
    let (_mir, program) = lower_backend(
        "enum D { A, B } fn pick(c) { if c > 0 { return D::B; } return D::A; } fn main() { let d = pick(1); return 0; }",
    );
    let pick = function(&program, "pick");
    assert_eq!(pick.result, BType::Enum);
    assert_eq!(pick.result_words, 1, "a unit-only enum stays one word");
}

#[test]
fn range_returns_are_still_rejected() {
    // Ranges are iteration values, not data values; returning one stays
    // E-B03.
    let kinds = error_kinds("fn f() { return 0 .. 10; } fn main() { return 0; }");
    assert_eq!(kinds, [mink::backend::BackendErrorKind::UnsupportedType]);
}

#[test]
fn aggregate_main_result_is_rejected() {
    // `main`'s result becomes the process exit code; an aggregate result
    // cannot be returned through the entry stub's single word (E-B09).
    let mut sources = SourceMap::new();
    let path = unique_source("main");
    std::fs::write(
        &path,
        "struct P { x: Int } fn main() { return P { x: 1 }; }",
    )
    .unwrap();
    let report = driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(report.errors.is_empty(), "the front end must accept it");
    let optimized = report.mir.expect("clean program lowers to MIR");
    let errors =
        backend::compile(&optimized, &sources, backend::Target::X86_64WindowsPe).unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].kind(),
        mink::backend::BackendErrorKind::InvalidEntryPoint
    );
    assert_eq!(errors[0].code(), "E-B09");
}

// ---------------------------------------------------------------------------
// Backend lowering: module-scope aggregate statics
// ---------------------------------------------------------------------------

#[test]
fn struct_statics_carry_decoded_images() {
    let (_mir, program) = lower_backend(
        "struct P { x: Int, y: Int } const ORIGIN = P { x: 1, y: 2 }; fn main() { return ORIGIN.x; }",
    );
    let static_binding = &program.statics[0];
    assert_eq!(static_binding.ty, BType::Struct);
    // The value image is the two little-endian words in normal byte order.
    assert_eq!(
        static_binding.bytes,
        vec![1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn array_statics_carry_decoded_images() {
    let (_mir, program) = lower_backend("const A = [10, 20, 30]; fn main() { return A[1]; }");
    let static_binding = &program.statics[0];
    assert_eq!(static_binding.ty, BType::Array);
    assert_eq!(
        static_binding.bytes,
        vec![
            10, 0, 0, 0, 0, 0, 0, 0, //
            20, 0, 0, 0, 0, 0, 0, 0, //
            30, 0, 0, 0, 0, 0, 0, 0,
        ]
    );
}

#[test]
fn bool_arrays_are_packed_in_the_image() {
    let (_mir, program) = lower_backend("const A = [true, false, true]; fn main() { return 0; }");
    // `[Bool; 3]` is 3 bytes; the region is rounded up to one word.
    let static_binding = &program.statics[0];
    assert_eq!(static_binding.bytes.len(), 8);
    assert_eq!(static_binding.bytes[..3], [1, 0, 1]);
}

#[test]
fn tagged_enum_statics_carry_images() {
    let (_mir, program) =
        lower_backend("enum E { A, B(Int) } const X = E::B(99); fn main() { return 0; }");
    let static_binding = &program.statics[0];
    assert_eq!(static_binding.ty, BType::Enum);
    assert_eq!(
        static_binding.bytes.len(),
        16,
        "a tagged union spans two words"
    );
    // The tag word is the discriminant of `B` (1), the payload word is 99.
    assert_eq!(static_binding.bytes[0], 1);
    assert_eq!(static_binding.bytes[8], 99);
}

#[test]
fn unit_enum_statics_are_one_word() {
    let (_mir, program) = lower_backend("enum D { A, B } const X = D::B; fn main() { return 0; }");
    let static_binding = &program.statics[0];
    assert_eq!(static_binding.ty, BType::Enum);
    assert_eq!(static_binding.bytes, vec![1, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn aggregate_static_copies_reference_earlier_bindings() {
    let (_mir, program) = lower_backend(
        "struct P { x: Int, y: Int } const A = P { x: 1, y: 2 }; const B = A; fn main() { return B.x; }",
    );
    assert_eq!(program.statics.len(), 2);
    assert_eq!(program.statics[0].bytes, program.statics[1].bytes);
}

#[test]
fn one_word_struct_statics_use_the_image_path() {
    // A struct with a single field occupies exactly one word but its value
    // is a materialized literal; it must not be confused with a word
    // binding.
    let (_mir, program) =
        lower_backend("struct P { x: Int } const P0 = P { x: 5 }; fn main() { return P0.x; }");
    let static_binding = &program.statics[0];
    assert_eq!(static_binding.ty, BType::Struct);
    assert_eq!(static_binding.bytes, vec![5, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn static_rooted_place_stores_are_read_modify_write() {
    // `a[0] = 9` on a mutable module array lowers to a load of the whole
    // binding, a place store, and a store of the whole binding back.
    let (_mir, program) =
        lower_backend("let mut a = [10, 20, 30]; fn main() { a[0] = 9; return a[0]; }");
    let main = function(&program, "main");
    let insts: Vec<_> = main.blocks.iter().flat_map(|b| &b.insts).collect();
    assert!(
        insts
            .iter()
            .any(|i| matches!(i.kind, BInstKind::LoadStatic { .. })),
        "the binding must be loaded before the place store"
    );
    assert!(
        insts
            .iter()
            .any(|i| matches!(i.kind, BInstKind::PlaceStore { .. })),
        "the element store must be a place store"
    );
    assert!(
        insts
            .iter()
            .any(|i| matches!(i.kind, BInstKind::StoreStatic { .. })),
        "the binding must be stored back after the place store"
    );
}

#[test]
fn word_statics_keep_decoded_values() {
    // Existing word-binding behavior is unchanged.
    let (_mir, program) = lower_backend("const a = 42; const b = true; fn main() { return a; }");
    assert_eq!(program.statics[0].value, 42);
    assert_eq!(program.statics[0].bytes, 42i64.to_le_bytes().to_vec());
    assert_eq!(program.statics[1].value, 1);
    assert_eq!(program.statics[1].bytes, 1i64.to_le_bytes().to_vec());
}

#[test]
fn string_containing_aggregate_statics_are_rejected() {
    // A string value needs a patched image address; string-valued module
    // bindings stay rejected (E-B05), including inside aggregates.
    let kinds =
        error_kinds("struct S { t: Str } const X = S { t: \"hi\" }; fn main() { return 0; }");
    assert_eq!(kinds, [mink::backend::BackendErrorKind::UnsupportedStatic]);
}

#[test]
fn non_literal_aggregate_statics_are_rejected() {
    // A module binding whose initializer calls a function needs runtime
    // initialization; it stays E-B05.
    let kinds = error_kinds(
        "struct S { p: Ptr<Int> } const X = S { p: rt_alloc(8) }; fn main() { return 0; }",
    );
    assert_eq!(kinds, [mink::backend::BackendErrorKind::UnsupportedStatic]);
}

#[test]
fn aggregate_static_regions_accumulate_in_source_order() {
    // A word binding followed by an aggregate binding keeps the word at
    // offset 0 and the aggregate after it; the emitter's bases accumulate
    // byte offsets (the emitted image is verified by the native tests).
    let (_mir, program) = lower_backend(
        "const n = 7; struct P { x: Int, y: Int } const ORIGIN = P { x: 1, y: 2 }; fn main() { return n + ORIGIN.x; }",
    );
    assert_eq!(program.statics.len(), 2);
    assert_eq!(program.statics[0].bytes.len(), 8);
    assert_eq!(program.statics[1].bytes.len(), 16);
}

// ---------------------------------------------------------------------------
// Native execution: aggregate returns
// ---------------------------------------------------------------------------

#[test]
fn native_struct_return_flows_to_fields() {
    let exe = build(
        "struct Point { x: Int, y: Int }
         fn make() { return Point { x: 3, y: 4 }; }
         fn main() {
             let p = make();
             rt_print_int(p.x * 10 + p.y);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "34");
}

#[test]
fn native_array_return_indexing() {
    let exe = build(
        "fn make() { return [10, 20, 30]; }
         fn main() {
             let a = make();
             rt_print_int(a[0] + a[2]);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "40");
}

#[test]
fn native_tagged_enum_return_dispatches() {
    let exe = build(
        "enum Shape { Circle(Int), Nothing }
         fn make(c) { if c > 0 { return Shape::Circle(c * 2); } return Shape::Nothing; }
         fn main() {
             let s = make(5);
             match s {
                 Shape::Circle(r) => { rt_print_int(r); },
                 Shape::Nothing => { rt_print_int(0); }
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "10");
}

#[test]
fn native_nested_aggregate_return() {
    let exe = build(
        "struct Inner { v: Int }
         struct Outer { a: [Inner; 2], tag: Int }
         fn make() { return Outer { a: [Inner { v: 3 }, Inner { v: 4 }], tag: 9 }; }
         fn main() {
             let o = make();
             rt_print_int(o.a[0].v * 100 + o.a[1].v * 10 + o.tag);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "349");
}

#[test]
fn native_chained_aggregate_calls() {
    let exe = build(
        "struct P { x: Int, y: Int }
         fn make(a) { let p = P { x: a, y: a * 2 }; return p; }
         fn bump(p) { let q = P { x: p.x + 1, y: p.y + 1 }; return q; }
         fn main() {
             let r = bump(make(5));
             rt_print_int(r.x * 100 + r.y);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "611");
}

#[test]
fn native_bool_field_struct_return() {
    let exe = build(
        "struct Inner { flag: Bool, v: Int }
         struct Outer { inner: Inner, tag: Bool }
         fn make() { return Outer { inner: Inner { flag: true, v: 7 }, tag: false }; }
         fn main() {
             let o = make();
             let mut s = 0;
             if o.inner.flag { s = s + 1; }
             if o.tag { s = s + 10; }
             rt_print_int(s * 100 + o.inner.v);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "107");
}

#[test]
fn native_aggregate_return_identity() {
    // An identity function returning its aggregate parameter.
    let exe = build(
        "enum E { A, B(Int) }
         fn id(x) { return x; }
         fn main() {
             let e = id(E::B(42));
             match e {
                 E::B(v) => { rt_print_int(v); },
                 E::A => { rt_print_int(0); }
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "42");
}

#[test]
fn native_aggregate_return_of_forward_referenced_array_field() {
    // The front-end fix (array-typed fields may reference later structs)
    // end-to-end: a struct containing an array of a later-declared struct
    // is built, returned, and read.
    let exe = build(
        "struct Grid { rows: [Point; 2] }
         struct Point { x: Int, y: Int }
         fn make() { return Grid { rows: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }] }; }
         fn main() {
             let g = make();
             rt_print_int(g.rows[1].x * 10 + g.rows[0].y);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "32");
}

// ---------------------------------------------------------------------------
// Native execution: module-scope aggregate statics
// ---------------------------------------------------------------------------

#[test]
fn native_module_struct_static_reads() {
    let exe = build(
        "struct Point { x: Int, y: Int }
         const ORIGIN = Point { x: 1, y: 2 };
         fn main() {
             let p = ORIGIN;
             rt_print_int(p.x * 10 + p.y);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "12");
}

#[test]
fn native_module_array_static_indexing() {
    let exe = build(
        "const A = [10, 20, 30];
         fn main() { rt_print_int(A[1]); return; }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "20");
}

#[test]
fn native_module_aggregate_mutation_single_step() {
    let exe = build(
        "let mut a = [10, 20, 30];
         fn main() { a[0] = 9; rt_print_int(a[0] + a[1]); return; }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "29");
}

#[test]
fn native_module_deep_place_mutation() {
    let exe = build(
        "struct Point { x: Int, y: Int }
         struct Grid { rows: [Point; 2] }
         let mut g = Grid { rows: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }] };
         fn main() {
             g.rows[1].y = 40;
             g.rows[0].x += 10;
             rt_print_int(g.rows[1].y * 100 + g.rows[0].x);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "4011");
}

#[test]
fn native_module_aggregate_whole_assignment() {
    let exe = build(
        "struct Point { x: Int, y: Int }
         let mut p = Point { x: 1, y: 2 };
         fn main() {
             p = Point { x: 9, y: 8 };
             rt_print_int(p.x * 10 + p.y);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "98");
}

#[test]
fn native_module_tagged_enum_static_matches() {
    let exe = build(
        "enum Shape { Circle(Int), Nothing }
         const DEFAULT = Shape::Circle(99);
         fn main() {
             let s = DEFAULT;
             match s {
                 Shape::Circle(r) => { rt_print_int(r); },
                 Shape::Nothing => { rt_print_int(0); }
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "99");
}

#[test]
fn native_module_unit_enum_static_matches() {
    let exe = build(
        "enum D { A, B }
         const X = D::B;
         fn main() {
             let d = X;
             match d {
                 D::A => { rt_print_int(1); },
                 D::B => { rt_print_int(2); }
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "2");
}

#[test]
fn native_module_array_of_struct_static() {
    let exe = build(
        "struct P { x: Int, y: Int }
         const G = [P { x: 1, y: 2 }, P { x: 3, y: 4 }];
         fn main() {
             let g = G;
             rt_print_int(g[1].x * 10 + g[0].y);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "32");
}

#[test]
fn native_module_aggregate_static_copy() {
    let exe = build(
        "struct Point { x: Int, y: Int }
         const A = Point { x: 1, y: 2 };
         const B = A;
         fn main() {
             rt_print_int(B.x * 10 + B.y);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "12");
}

#[test]
fn native_module_bool_array_static() {
    let exe = build(
        "const FLAGS = [true, false, true];
         fn main() {
             let mut s = 0;
             if FLAGS[0] { s = s + 1; }
             if FLAGS[1] { s = s + 10; }
             if FLAGS[2] { s = s + 100; }
             rt_print_int(s);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "101");
}

#[test]
fn native_scalar_module_bindings_still_work() {
    // Regression: word bindings, scalar statics, and mixed programs are
    // unchanged.
    let exe = build(
        "const n = 5;
         let mut m = 1;
         fn main() { m = m + n; rt_print_int(m); return; }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "6");
}

#[test]
fn native_aggregate_returns_and_statics_are_deterministic() {
    let src = "struct P { x: Int, y: Int }
               enum E { A, B(Int) }
               const ORIGIN = P { x: 1, y: 2 };
               const TAG = E::B(7);
               fn make() { return P { x: ORIGIN.x + 1, y: 2 }; }
               fn main() {
                   let p = make();
                   rt_print_int(p.x * 100 + p.y);
                   match TAG {
                       E::B(v) => { rt_print_int(v); },
                       E::A => { rt_print_int(0); }
                   }
                   return;
               }";
    let exe1 = build(src);
    let exe2 = build(src);
    assert_eq!(
        std::fs::read(&exe1).unwrap(),
        std::fs::read(&exe2).unwrap(),
        "identical sources produce identical images"
    );
    let (code1, out1) = run(&exe1);
    let (code2, out2) = run(&exe2);
    assert_eq!(code1, code2);
    assert_eq!(out1, out2);
    assert_eq!(code1, 0);
    assert_eq!(String::from_utf8_lossy(&out1).trim(), "202\n7");
}

#[test]
fn native_aggregate_return_deterministic_images() {
    let src = "struct P { x: Int, y: Int }
               fn make() { return P { x: 1, y: 2 }; }
               fn main() { let p = make(); rt_print_int(p.x * 10 + p.y); return; }";
    let exe1 = build(src);
    let exe2 = build(src);
    assert_eq!(
        std::fs::read(&exe1).unwrap(),
        std::fs::read(&exe2).unwrap(),
        "aggregate-return programs must emit byte-identical images"
    );
    let (code, stdout) = run(&exe1);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "12");
}

#[test]
fn native_module_binding_bounds_check_still_traps() {
    // A runtime bounds check on a module array still traps with E-R10
    // (a *constant* out-of-range index is caught by the checker as E-T11,
    // so the trap path needs a non-constant index).
    let exe = build(
        "const A = [10, 20];
         fn main() { let i = 5; rt_print_int(A[i]); return; }",
    );
    let (code, _) = run(&exe);
    assert_eq!(
        code, 110,
        "an out-of-range module array index must trap E-R10"
    );
}

// ---------------------------------------------------------------------------
// Sub-word aggregate values (regression: slot guard words)
// ---------------------------------------------------------------------------
// A value whose bytes include a sub-word tail (booleans, sub-word element
// arrays) runs *downward* from its slot's first word; without a guard word
// the next slot's first qword silently overwrote those tail bytes, so
// bool reads at byte offset >= 1 returned 0. Every path is covered here:
// construction, element/field access, whole-value copies, module statics,
// parameters, and returns.

#[test]
fn native_bool_array_elements_read_at_every_index() {
    let exe = build(
        "fn main() {
             let a = [true, false, true, true, false, true, false, true];
             let mut n = 0;
             if a[0] { n = n + 1; }
             if a[1] { n = n + 2; }
             if a[2] { n = n + 4; }
             if a[3] { n = n + 8; }
             if a[4] { n = n + 16; }
             if a[5] { n = n + 32; }
             if a[6] { n = n + 64; }
             if a[7] { n = n + 128; }
             rt_print_int(n);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "173");
}

#[test]
fn native_bool_struct_fields_read_at_nonzero_offsets() {
    // Bools at offsets 0 and 16 (word-aligned, as the layout packs them
    // next to the integer fields), read back correctly; a *packed* run of
    // booleans at offsets 1-7 followed by an integer field remains a
    // known limitation (the integer chunk's qword covers the downward
    // tail bytes of the packed booleans).
    let exe = build(
        "struct F { a: Bool, b: Int, c: Bool, d: Int }
         fn main() {
             let f = F { a: true, b: 7, c: true, d: 7 };
             let mut n = 0;
             if f.a { n = n + 1; }
             if f.c { n = n + 4; }
             rt_print_int(n * 10 + f.b * 10 + f.d);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    // n = 5, so 5 * 10 + 7 * 10 + 7
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "127");
}

#[test]
fn native_bool_static_array_elements_read_at_every_index() {
    let exe = build(
        "const B = [true, false, true, false, true];
         fn main() {
             let mut n = 0;
             if B[0] { n = n + 1; }
             if B[1] { n = n + 2; }
             if B[2] { n = n + 4; }
             if B[3] { n = n + 8; }
             if B[4] { n = n + 16; }
             rt_print_int(n);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "21");
}

#[test]
fn native_bool_aggregate_copies_preserve_tail_bytes() {
    // Whole-value copies move the value as full words; the guard word
    // carries the sub-word tail bytes at its top, so a copy keeps them.
    let exe = build(
        "struct F { a: Bool, b: Bool, c: Bool }
         fn main() {
             let f = F { a: true, b: false, c: true };
             let g = f;
             let a = [true, false, true, true];
             let b = a;
             let mut n = 0;
             if g.a { n = n + 1; }
             if g.b { n = n + 2; }
             if g.c { n = n + 4; }
             if b[0] { n = n + 8; }
             if b[1] { n = n + 16; }
             if b[2] { n = n + 32; }
             if b[3] { n = n + 64; }
             rt_print_int(n);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "109");
}

#[test]
fn native_bool_aggregates_pass_and_return_through_slots() {
    let exe = build(
        "fn pick(a) {
             let mut n = 0;
             if a[0] { n = n + 1; }
             if a[1] { n = n + 2; }
             if a[2] { n = n + 4; }
             return n;
         }
         fn make() { return [true, false, true]; }
         fn main() {
             let a = [true, false, true];
             rt_print_int(pick(a) * 10 + pick(make()));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "55");
}

#[test]
fn native_bool_element_mutation_then_read() {
    let exe = build(
        "fn main() {
             let mut a = [false, false, false];
             let mut n = 0;
             a[1] = true;
             a[2] = true;
             if a[0] { n = n + 1; }
             if a[1] { n = n + 2; }
             if a[2] { n = n + 4; }
             rt_print_int(n);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "6");
}
