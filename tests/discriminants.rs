//! Integration tests for the explicit enum discriminants milestone
//! (session 20): `enum E { A = 5, B }` declarations on unit and
//! data-carrying variants, implicit continuation (previous value plus
//! one), the wrapping 64-bit literal model, duplicate-discriminant
//! rejection (`E-T31`), implicit-continuation overflow rejection
//! (`E-T32`), the parse diagnostics (`E-P19`), the effective tag values
//! flowing through construction, pattern matching, equality, layout,
//! MIR/backend lowering, ownership, and native execution.
//!
//! The rules under test are documented in
//! `docs/implementation/DISCRIMINANTS_IMPLEMENTATION.md` and specified in
//! `docs/language/CORE_LANGUAGE.md` and `docs/language/TYPE_SYSTEM.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

use mink::ast::{Ast, ExprKind, ItemKind};
use mink::backend::{self, BType};
use mink::mir::{
    self, MirConstant, MirConstantKind, MirFn, MirItemKind, MirOperandKind, MirProgram,
    MirRvalueKind, MirStmtKind,
};
use mink::parser::{ParseErrorKind, ParseOutput, parse};
use mink::runtime::layout::{enum_layout, scalar_size_align};
use mink::semantics::SemanticResult;
use mink::source::SourceMap;
use mink::typecheck::{TypeErrorKind, TypeId, TypeKind, TypeResult};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_src(src: &str) -> ParseOutput {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file is present");
    parse(file)
}

fn parse_errors(src: &str) -> Vec<ParseErrorKind> {
    parse_src(src)
        .parse_errors()
        .iter()
        .map(|e| e.kind())
        .collect()
}

/// Parses, semantically analyzes, and type-checks `src`, asserting that it
/// lexes and parses cleanly (type tests start from valid syntax).
fn check_src(src: &str) -> (SourceMap, Ast, SemanticResult, TypeResult) {
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
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    (sources, ast, semantic, types)
}

/// All type errors of `kind`.
fn type_errors(types: &TypeResult, kind: TypeErrorKind) -> Vec<&mink::typecheck::TypeError> {
    types
        .errors()
        .iter()
        .filter(|error| error.kind() == kind)
        .collect()
}

/// The first enum item in `ast`, panicking if there is none.
fn first_enum(ast: &Ast) -> &mink::ast::EnumItem {
    ast.items()
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Enum(e) => Some(e),
            _ => None,
        })
        .expect("program declares an enum")
}

/// The inferred type of the first symbol named `name`.
fn symbol_type(types: &TypeResult, semantic: &SemanticResult, name: &str) -> TypeId {
    let symbol = semantic
        .symbols()
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named `{name}`"));
    types
        .symbol_type(symbol.id)
        .unwrap_or_else(|| panic!("symbol `{name}` has no type"))
}

/// The declared variants of the enum named `name`, with their effective
/// discriminants, in declaration order. Enum type names are not symbols, so
/// the enum is located through the type table by name.
fn variant_discriminants(types: &TypeResult, name: &str) -> Vec<i64> {
    let info = types
        .types()
        .enums()
        .iter()
        .find(|info| info.name == name)
        .unwrap_or_else(|| panic!("no enum named `{name}`"));
    info.variants
        .iter()
        .map(|variant| variant.discriminant)
        .collect()
}

/// The enum's first declaration parsed from `src`, for inspecting whether
/// each variant carries an explicit discriminant expression.
fn explicit_discriminants(ast: &Ast) -> Vec<Option<i64>> {
    first_enum(ast)
        .variants
        .iter()
        .map(|variant| {
            variant
                .discriminant
                .as_ref()
                .map(|literal| match &literal.kind {
                    ExprKind::Int => {
                        // The value is decoded by the type checker; here we
                        // only confirm the shape is an integer literal (the
                        // decoded values are covered by the type tests).
                        0
                    }
                    ExprKind::Unary {
                        op: mink::ast::UnaryOp::Neg,
                        ..
                    } => 0,
                    _ => panic!("an explicit discriminant must be a literal"),
                })
        })
        .collect()
}

/// Runs the front end and lowers through HIR and MIR, asserting every stage
/// is clean.
fn lower_mir(src: &str) -> (mink::hir::HirProgram, MirProgram) {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).unwrap();
    let parsed = parse(file);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let semantic = mink::semantics::analyze(&ast);
    assert!(semantic.errors().is_empty(), "{:?}", semantic.errors());
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    let hir = mink::hir::lower(&ast, &semantic, &types)
        .unwrap_or_else(|errors| panic!("clean front end must lower: {errors:?}"));
    let mir = mir::lower(&hir).unwrap_or_else(|errors| panic!("clean HIR must lower: {errors:?}"));
    if let Err(errors) = mir::validate(&mir) {
        panic!("lowering a clean program must produce valid MIR: {errors:?}");
    }
    (hir, mir)
}

fn mir_fn<'p>(mir: &'p MirProgram, name: &str) -> &'p MirFn {
    mir.items
        .iter()
        .find_map(|item| match &item.kind {
            MirItemKind::Fn(f) if f.name.name == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no MIR function named `{name}`"))
}

/// Whether `f` contains an equality comparison whose right side is an enum
/// constant with discriminant `discriminant` (a variant tag test).
fn compares_against_enum_discriminant(f: &MirFn, discriminant: i64) -> bool {
    f.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            let MirRvalueKind::Binary { op, rhs, .. } = &rvalue.kind else {
                return false;
            };
            *op == mink::ast::BinaryOp::Eq
                && matches!(
                    &rhs.kind,
                    MirOperandKind::Constant(MirConstant {
                        kind: MirConstantKind::Enum { variant },
                        ..
                    }) if *variant == discriminant
                )
        })
    })
}

/// Whether `f` loads an enum constant with discriminant `discriminant`
/// (a unit-variant construction).
fn loads_enum_discriminant(f: &MirFn, discriminant: i64) -> bool {
    f.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            let MirRvalueKind::Use(operand) = &rvalue.kind else {
                return false;
            };
            matches!(
                &operand.kind,
                MirOperandKind::Constant(MirConstant {
                    kind: MirConstantKind::Enum { variant },
                    ..
                }) if *variant == discriminant
            )
        })
    })
}

/// Whether `f` constructs a data-carrying variant with tag `discriminant`.
fn constructs_enum_with_tag(f: &MirFn, discriminant: i64) -> bool {
    f.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            matches!(
                &rvalue.kind,
                MirRvalueKind::EnumInit {
                    discriminant: tag,
                    ..
                } if *tag == discriminant
            )
        })
    })
}

/// Runs the ownership analyzer on `src`, returning its errors.
fn check_ownership(src: &str) -> Vec<mink::semantics::SemanticError> {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).unwrap();
    let parsed = parse(file);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let semantic = mink::semantics::analyze(&ast);
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    let ownership = mink::ownership::check(&ast, &semantic, &types);
    ownership.errors().to_vec()
}

/// Runs the front end on `src` and lowers the optimized MIR into backend
/// instructions, asserting every stage is clean.
fn lower_backend(src: &str) -> (MirProgram, mink::backend::BProgram) {
    let mut sources = SourceMap::new();
    let name = std::thread::current()
        .name()
        .unwrap_or("backend")
        .replace("::", "_");
    let path = std::env::temp_dir().join(format!(
        "mink_discriminants_test_{}_{name}.mink",
        std::process::id()
    ));
    std::fs::write(&path, src).unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let mir = report.mir.as_ref().expect("clean program lowers to MIR");
    let program = backend::lower(mir, &sources)
        .unwrap_or_else(|errors| panic!("clean MIR must lower: {errors:?}"));
    if let Err(errors) = backend::verify(&program) {
        panic!("lowering must produce valid instructions: {errors:?}");
    }
    (mir.clone(), program)
}

// ---------------------------------------------------------------------------
// Parser: `Variant = IntLit`
// ---------------------------------------------------------------------------

#[test]
fn explicit_discriminants_parse_on_unit_variants() {
    let (_s, ast, _semantic, types) = check_src("enum E { A = 5, B, C = 100, D } fn main() {}");
    let e = first_enum(&ast);
    assert_eq!(e.variants.len(), 4);
    let explicit = explicit_discriminants(&ast);
    assert!(explicit[0].is_some(), "A declares `= 5`");
    assert!(explicit[1].is_none(), "B continues implicitly");
    assert!(explicit[2].is_some(), "C declares `= 100`");
    assert!(explicit[3].is_none(), "D continues implicitly");
    // The explicit values decode into the effective discriminants.
    assert_eq!(variant_discriminants(&types, "E"), vec![5, 6, 100, 101]);
}

#[test]
fn explicit_discriminants_parse_on_data_carrying_variants() {
    let (_s, ast, _semantic, types) =
        check_src("enum E { A(Int) = 10, B, C(Str) = 200, D } fn main() {}");
    let e = first_enum(&ast);
    assert_eq!(e.variants.len(), 4);
    assert!(e.variants[0].payload.is_some(), "A carries a payload");
    assert!(explicit_discriminants(&ast)[0].is_some());
    assert!(explicit_discriminants(&ast)[1].is_none());
    assert_eq!(variant_discriminants(&types, "E"), vec![10, 11, 200, 201]);
}

#[test]
fn negated_radix_and_separated_literals_parse() {
    let (_s, ast, _semantic, types) =
        check_src("enum E { A = -5, B = 0x10, C = 1_000, D = 0o17, F = 0b101 } fn main() {}");
    let e = first_enum(&ast);
    assert_eq!(e.variants.len(), 5);
    // The negated form is a unary-minus literal; the radix and separated
    // forms are plain integer literals.
    assert!(matches!(
        &e.variants[0]
            .discriminant
            .as_ref()
            .expect("A is explicit")
            .kind,
        ExprKind::Unary { .. }
    ));
    for variant in &e.variants[1..] {
        assert!(
            matches!(
                variant.discriminant.as_ref().expect("explicit").kind,
                ExprKind::Int
            ),
            "radix and separated literals are integer literals"
        );
    }
    assert_eq!(
        variant_discriminants(&types, "E"),
        vec![-5, 16, 1000, 15, 5]
    );
}

#[test]
fn discriminant_requires_an_integer_literal() {
    for src in [
        "enum E { A = true } fn main() {}",
        "enum E { A = 1.5 } fn main() {}",
        "enum E { A = x } fn main() {}",
        "enum E { A = } fn main() {}",
        "enum E { A = - } fn main() {}",
        "enum E { A = -x } fn main() {}",
    ] {
        assert_eq!(
            parse_errors(src),
            vec![ParseErrorKind::ExpectedIntegerLiteral],
            "`{src}` must be E-P19"
        );
    }
}

#[test]
fn bad_discriminant_recovers_to_the_next_variant() {
    // A bad discriminant is one E-P19; the following variant still parses.
    let parsed = parse_src("enum E { A = true, B } fn main() {}");
    let (ast, _, errors) = parsed.into_parts();
    assert_eq!(errors.len(), 1, "{:?}", errors);
    assert_eq!(errors[0].kind(), ParseErrorKind::ExpectedIntegerLiteral);
    let e = first_enum(&ast);
    assert_eq!(e.variants.len(), 2, "B must still be parsed");
    assert_eq!(e.variants[1].name.name, "B");
}

// ---------------------------------------------------------------------------
// Type system: effective discriminants, duplicates, overflow
// ---------------------------------------------------------------------------

#[test]
fn implicit_discriminants_still_start_at_zero() {
    // Session 17/19 behavior is unchanged when no explicit values are given.
    let (_s, _a, _semantic, types) = check_src("enum E { A, B, C } fn main() {}");
    assert_eq!(variant_discriminants(&types, "E"), vec![0, 1, 2]);
}

#[test]
fn mixed_implicit_and_explicit_values_continue() {
    let (_s, _a, _semantic, types) = check_src("enum E { A, B = 10, C } fn main() {}");
    assert_eq!(variant_discriminants(&types, "E"), vec![0, 10, 11]);
}

#[test]
fn negative_discriminants_continue_implicitly() {
    let (_s, _a, _semantic, types) = check_src("enum E { A = -1, B } fn main() {}");
    assert_eq!(variant_discriminants(&types, "E"), vec![-1, 0]);
}

#[test]
fn wrapping_literals_keep_the_64_bit_model() {
    // The language's literal model is wrapping 64-bit two's complement:
    // 2^64 - 1 is the same bit pattern as -1.
    let (_s, _a, _semantic, types) =
        check_src("enum E { A = 18446744073709551615, B } fn main() {}");
    assert_eq!(variant_discriminants(&types, "E"), vec![-1, 0]);
}

#[test]
fn duplicate_explicit_discriminants_are_rejected() {
    let (_s, _a, _semantic, types) = check_src("enum E { A = 5, B = 5 } fn main() {}");
    let errors = type_errors(&types, TypeErrorKind::DuplicateDiscriminant);
    assert_eq!(errors.len(), 1, "{:?}", types.errors());
    assert!(
        errors[0].actual().unwrap().contains("`B`"),
        "the later variant is named: {}",
        errors[0]
    );
    let related = errors[0]
        .related()
        .expect("the earlier variant is the related location");
    let a_span = first_enum(&_a).variants[0].span;
    assert_eq!(
        related, a_span,
        "the related span points at the first variant"
    );
}

#[test]
fn implicit_explicit_collisions_are_rejected() {
    // `A` implicitly gets 0; `B = 0` collides with it.
    let (_s, _a, _semantic, types) = check_src("enum E { A, B = 0 } fn main() {}");
    let errors = type_errors(&types, TypeErrorKind::DuplicateDiscriminant);
    assert_eq!(errors.len(), 1, "{:?}", types.errors());
}

#[test]
fn only_the_later_duplicate_is_reported() {
    let (_s, _a, _semantic, types) = check_src("enum E { A = 5, B = 5, C = 6 } fn main() {}");
    let errors = type_errors(&types, TypeErrorKind::DuplicateDiscriminant);
    assert_eq!(errors.len(), 1, "one root error only: {:?}", types.errors());
    assert!(errors[0].actual().unwrap().contains("`B`"));
}

#[test]
fn duplicates_on_tagged_unions_are_rejected() {
    let (_s, _a, _semantic, types) =
        check_src("enum E { A(Int) = 5, B = 5 } fn main() { let x = E::A(1); }");
    let errors = type_errors(&types, TypeErrorKind::DuplicateDiscriminant);
    assert_eq!(errors.len(), 1, "{:?}", types.errors());
}

#[test]
fn implicit_continuation_overflow_is_rejected_once() {
    let (_s, _a, _semantic, types) =
        check_src("enum E { A = 9223372036854775807, B, C = 3 } fn main() {}");
    let errors = type_errors(&types, TypeErrorKind::DiscriminantOverflow);
    assert_eq!(errors.len(), 1, "reported once: {:?}", types.errors());
    assert!(errors[0].actual().unwrap().contains("`B`"));
    // An explicit variant after the overflow resolves normally.
    assert_eq!(variant_discriminants(&types, "E"), vec![i64::MAX, 0, 3]);
}

#[test]
fn tagged_union_equality_stays_rejected_with_explicit_tags() {
    let (_s, _a, _semantic, types) = check_src(
        "enum E { A(Int) = 5, B = 6 } fn main() { let a = E::A(1); let b = E::B; if a == b { return; } return; }",
    );
    let errors = type_errors(&types, TypeErrorKind::EnumEquality);
    assert_eq!(errors.len(), 1, "{:?}", types.errors());
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

#[test]
fn unit_only_enums_stay_scalar_with_explicit_tags() {
    // A unit-only enum is a single word regardless of its tag values.
    let (_s, _a, semantic, types) =
        check_src("enum E { A = 5, B = 6 } fn main() { let x = E::A; }");
    let ty = symbol_type(&types, &semantic, "x");
    assert_eq!(
        scalar_size_align(types.types(), ty),
        Some((8, 8)),
        "unit-only enums remain single-word discriminants"
    );
}

#[test]
fn tagged_unions_keep_their_geometry_with_explicit_tags() {
    let (_s, _a, semantic, types) =
        check_src("enum E { A = 100, B(Int) = 200 } fn main() { let x = E::B(5); }");
    let ty = symbol_type(&types, &semantic, "x");
    let TypeKind::Enum(id) = types.types().kind(ty).unwrap() else {
        panic!("x is an enum");
    };
    let layout = enum_layout(*id, types.types()).unwrap();
    assert!(
        layout.tagged,
        "an enum with a payload variant is a tagged union"
    );
    assert_eq!(layout.size, 16, "tag word plus an 8-byte payload");
    assert_eq!(layout.align, 8);
    assert_eq!(layout.tag_offset, 0);
    assert_eq!(layout.payload_offset, 8);
    // The variant payload layouts record the explicit tag values.
    assert_eq!(layout.variants[0].discriminant, 100);
    assert_eq!(layout.variants[1].discriminant, 200);
    assert_eq!(layout.variants[0].size, 0, "unit variant has no payload");
    assert_eq!(layout.variants[1].size, 8, "Int payload is one word");
}

// ---------------------------------------------------------------------------
// MIR lowering
// ---------------------------------------------------------------------------

#[test]
fn unit_construction_carries_the_explicit_tag() {
    let src = "enum E { A = 5, B } fn main() { let a = E::A; return; }";
    let (_hir, mir) = lower_mir(src);
    let main = mir_fn(&mir, "main");
    assert!(
        loads_enum_discriminant(main, 5),
        "construction of `E::A` must load tag 5"
    );
    assert!(
        !loads_enum_discriminant(main, 6),
        "`E::B` must not be constructed"
    );
}

#[test]
fn data_carrying_construction_carries_the_explicit_tag() {
    let src = "enum E { A, B(Int) = 42 } fn main() { let e = E::B(7); return; }";
    let (_hir, mir) = lower_mir(src);
    let main = mir_fn(&mir, "main");
    assert!(
        constructs_enum_with_tag(main, 42),
        "construction of `E::B` must carry tag 42"
    );
}

#[test]
fn match_tests_use_explicit_tags() {
    let src = "enum E { A = 5, B } fn main() { let e = E::A; match e { E::A => { }, E::B => { } } return; }";
    let (_hir, mir) = lower_mir(src);
    let main = mir_fn(&mir, "main");
    assert!(
        compares_against_enum_discriminant(main, 5),
        "the `E::A` arm must test tag 5"
    );
    assert!(
        compares_against_enum_discriminant(main, 6),
        "the `E::B` arm must test tag 6"
    );
}

#[test]
fn lowering_is_deterministic_with_explicit_tags() {
    let src = "enum E { A = 5, B(Int) = 42 } fn main() { let e = E::B(7); match e { E::B(x) => { rt_print_int(x); }, E::A => { } } return; }";
    let (first_hir, first_mir) = lower_mir(src);
    let (second_hir, second_mir) = lower_mir(src);
    assert_eq!(first_mir, second_mir);
    assert_eq!(first_hir, second_hir);
}

// ---------------------------------------------------------------------------
// Backend lowering and verification
// ---------------------------------------------------------------------------

#[test]
fn tagged_enum_locals_are_multi_word_with_explicit_tags() {
    let src = "enum E { A = 5, B(Int) = 42 } fn main() { let x = E::B(5); }";
    let (_mir, program) = lower_backend(src);
    let main = program
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("main lowered");
    assert!(
        main.locals
            .iter()
            .any(|l| l.ty == BType::Enum && l.words == 2),
        "a tagged union spans two words in the backend"
    );
}

#[test]
fn tagged_enum_results_are_supported_with_explicit_tags() {
    // Session 22: a tagged-union (multi-word) result is returned through
    // a caller-allocated return slot; explicit discriminants flow through
    // the returned value unchanged.
    let src =
        "enum E { A = 5, B(Int) = 42 } fn id(x) { return x; } fn main() { let x = id(E::B(5)); }";
    let mut sources = SourceMap::new();
    let name = std::thread::current()
        .name()
        .unwrap_or("result")
        .replace("::", "_");
    let path = std::env::temp_dir().join(format!(
        "mink_discriminants_result_{}_{name}.mink",
        std::process::id()
    ));
    std::fs::write(&path, src).unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(
        report.errors.is_empty(),
        "the front end must accept the program: {:?}",
        report.errors
    );
    let mir = report.mir.as_ref().expect("clean program lowers to MIR");
    let program = backend::lower(mir, &sources)
        .unwrap_or_else(|errors| panic!("a tagged-union result must lower: {errors:?}"));
    backend::verify(&program).expect("lowered program must verify");
    let id = program.functions.iter().find(|f| f.name == "id").unwrap();
    assert_eq!(id.result, BType::Enum);
    assert_eq!(id.result_words, 2, "a tagged union spans two words");
}

// ---------------------------------------------------------------------------
// Ownership
// ---------------------------------------------------------------------------

#[test]
fn explicit_tags_do_not_change_copy_semantics() {
    // A unit-only enum copies freely even with explicit discriminants.
    let src = "enum E { A = 5, B = 6 } fn main() { let a = E::A; let b = a; let c = a; rt_print_int(1); return; }";
    assert!(
        check_ownership(src).is_empty(),
        "{:?}",
        check_ownership(src)
    );
}

#[test]
fn tagged_owned_payloads_still_move_with_explicit_tags() {
    // Construction transfers an owned payload (a literal is an immutable
    // constant and copies, so the payload must be an owned allocation);
    // using it afterwards is E-S10.
    let src = "enum E { A(Str) = 5, B } fn main() { let s = rt_str_alloc(2); let e = E::A(s); rt_print_str(s); return; }";
    let errors = check_ownership(src);
    assert!(
        errors
            .iter()
            .any(|e| e.kind() == mink::semantics::SemanticErrorKind::UseOfMovedValue),
        "using a moved payload must be E-S10: {:?}",
        errors
    );
}

#[test]
fn payload_binding_still_moves_owned_payloads_with_explicit_tags() {
    let src = "enum E { A(Str) = 5, B } fn main() { let s = rt_str_alloc(2); let e = E::A(s); match e { E::A(x) => { rt_print_str(x); }, E::B => { } } match e { E::A(_) => { }, E::B => { } } return; }";
    let errors = check_ownership(src);
    assert!(
        errors
            .iter()
            .any(|e| e.kind() == mink::semantics::SemanticErrorKind::UseOfMovedValue),
        "binding an owned payload consumes the scrutinee: {:?}",
        errors
    );
}

// ---------------------------------------------------------------------------
// Native execution
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "mink_discriminants_test_{}_{name}",
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

#[test]
fn native_dispatch_with_explicit_tags() {
    let exe = build(
        "enum E { A = 5, B, C = 100, D }
         fn label(e) {
             match e {
                 E::A => { return 1; },
                 E::B => { return 2; },
                 E::C => { return 3; },
                 E::D => { return 4; },
             }
         }
         fn main() {
             rt_print_int(label(E::A));
             rt_print_int(label(E::B));
             rt_print_int(label(E::C));
             rt_print_int(label(E::D));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "1\n2\n3\n4");
}

#[test]
fn native_payload_extraction_with_explicit_tags() {
    let exe = build(
        "enum Shape { Circle(Int) = 10, Nothing = -1 }
         fn area(s) {
             match s {
                 Shape::Circle(r) => { return r * r; },
                 Shape::Nothing => { return 0; },
             }
         }
         fn main() {
             rt_print_int(area(Shape::Circle(5)));
             rt_print_int(area(Shape::Nothing));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "25\n0");
}

#[test]
fn native_equality_with_explicit_tags() {
    let exe = build(
        "enum E { A = 7, B = 8 }
         fn main() {
             let a = E::A;
             let b = E::B;
             if a == E::A { rt_print_int(1); }
             if b == E::B { rt_print_int(2); }
             if a != b { rt_print_int(3); }
             if a == E::B { rt_print_int(9); }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "1\n2\n3");
}

#[test]
fn native_negative_radix_and_separated_tags() {
    let exe = build(
        "enum E { A = 0x10, B = 1_000, C = -5, D }
         fn main() {
             let a = E::A;
             let b = E::B;
             let c = E::C;
             let d = E::D;
             if a == E::A { rt_print_int(16); }
             if b == E::B { rt_print_int(1000); }
             if c == E::C { rt_print_int(-5); }
             if d == E::D { rt_print_int(-4); }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "16\n1000\n-5\n-4");
}

#[test]
fn native_mixed_unit_and_payload_variants_with_explicit_tags() {
    let exe = build(
        "enum E { A = 3, B(Int) = 40, C = 500, D(Int) = 6000 }
         fn f(e) {
             match e {
                 E::B(x) => { return x + 1; },
                 E::D(x) => { return x - 1; },
                 E::A => { return 0; },
                 E::C => { return -1; },
             }
         }
         fn main() {
             rt_print_int(f(E::A));
             rt_print_int(f(E::B(41)));
             rt_print_int(f(E::C));
             rt_print_int(f(E::D(6001)));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "0\n42\n-1\n6000");
}

fn emit_image(src: &str) -> mink::backend::EmittedImage {
    let mut sources = SourceMap::new();
    let name = std::thread::current()
        .name()
        .unwrap_or("image")
        .replace("::", "_");
    let path = std::env::temp_dir().join(format!(
        "mink_discriminants_img_{}_{name}.mink",
        std::process::id()
    ));
    std::fs::write(&path, src).unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let mir = report.mir.expect("clean program");
    backend::compile(&mir, &sources, mink::backend::Target::native()).expect("compile succeeds")
}

#[test]
fn native_images_are_deterministic_with_explicit_tags() {
    let src = "enum E { A = 5, B(Int) = 42 } fn main() { let e = E::B(7); match e { E::B(x) => { rt_print_int(x); }, E::A => { } } return; }";
    let first = emit_image(src);
    let second = emit_image(src);
    assert_eq!(first, second);
}
