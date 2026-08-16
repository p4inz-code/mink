//! Integration tests for the data-carrying enum variants (sum types)
//! milestone (session 19): payload types in enum declarations
//! (`enum E { V(Type) }`), construction (`E::V(expr)`), payload patterns
//! (`E::V(pattern)` with binding extraction), the tagged-union layout,
//! payload-aware exhaustiveness, ownership of owned payloads, MIR/backend
//! lowering, and native execution.
//!
//! The rules under test are documented in
//! `docs/implementation/ENUM_TYPES_IMPLEMENTATION.md` and specified in
//! `docs/language/CORE_LANGUAGE.md` and `docs/language/TYPE_SYSTEM.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

use mink::ast::{Ast, ExprKind, ItemKind, Pattern};
use mink::backend::{self, BType};
use mink::mir::{self, MirFn, MirItemKind, MirProgram, MirRvalueKind, MirStmtKind};
use mink::parser::{ParseErrorKind, ParseOutput, parse};
use mink::runtime::layout::{enum_layout, scalar_size_align};
use mink::semantics::{SemanticErrorKind, SemanticResult};
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

/// The enum's declared variant payload types, in declaration order. Enum
/// type names are not symbols, so the enum is located through the type
/// table by name.
fn variant_payloads(types: &TypeResult, semantic: &SemanticResult, name: &str) -> Vec<String> {
    let info = types
        .types()
        .enums()
        .iter()
        .find(|info| info.name == name)
        .unwrap_or_else(|| panic!("no enum named `{name}`"));
    let _ = semantic;
    info.variants
        .iter()
        .map(|variant| {
            variant
                .payload
                .map(|payload| types.types().display(payload))
                .unwrap_or_else(|| "unit".to_string())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Parser: payload types, construction, and payload patterns
// ---------------------------------------------------------------------------

#[test]
fn data_carrying_variant_declarations_parse() {
    let src = "enum Shape { Circle(Int), Nothing } fn main() {}";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let e = first_enum(&ast);
    assert_eq!(e.variants.len(), 2);
    let mink::ast::TyKind::Named(first) = e.variants[0].payload.as_ref().unwrap().kind.clone()
    else {
        panic!("payload type must be a named type");
    };
    assert_eq!(first.name, "Int");
    assert!(
        e.variants[1].payload.is_none(),
        "unit variant has no payload"
    );
}

#[test]
fn enum_payloads_may_reference_structs_and_enums() {
    let src = "struct P { x: Int }
               enum A { P(P), Q(Int) }
               enum B { X(A), Y }
               fn main() {}";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(!types.has_errors(), "{:?}", types.errors());
    assert_eq!(variant_payloads(&types, &semantic, "A"), vec!["P", "Int"]);
    assert_eq!(variant_payloads(&types, &semantic, "B"), vec!["A", "unit"]);
}

#[test]
fn data_carrying_construction_parses() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::B(5); }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let main = ast
        .items()
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Fn(f) if f.name.name == "main" => Some(f),
            _ => None,
        })
        .unwrap();
    let mink::ast::StmtKind::Let(binding) = &main.body.stmts[0].kind else {
        panic!("expected a let binding");
    };
    let ExprKind::EnumVariant { payload, .. } = &binding.init.kind else {
        panic!("expected an enum-variant construction");
    };
    assert!(payload.is_some(), "construction carries its payload");
}

#[test]
fn payload_pattern_parses() {
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(1); match e { E::B(x) => { rt_print_int(x); }, E::A => { } } return; }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let main = ast
        .items()
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Fn(f) if f.name.name == "main" => Some(f),
            _ => None,
        })
        .unwrap();
    let mink::ast::StmtKind::Match(m) = &main.body.stmts[1].kind else {
        panic!("expected a match statement");
    };
    let Pattern::EnumVariant { payload, .. } = &m.arms[0].pattern else {
        panic!("expected an enum-variant pattern");
    };
    let Some(inner) = payload else {
        panic!("data-carrying pattern carries its payload pattern");
    };
    assert!(matches!(inner.as_ref(), Pattern::Binding(_)));
}

#[test]
fn empty_payload_is_a_parse_error() {
    // `Variant()` — a data-carrying variant carries exactly one payload
    // (E-P25), in declarations, constructions, and patterns alike.
    assert_eq!(
        parse_errors("enum E { V() } fn main() {}"),
        vec![ParseErrorKind::EmptyPayload]
    );
    assert_eq!(
        parse_errors("enum E { V(Int) } fn main() { let x = E::V(); }"),
        vec![ParseErrorKind::EmptyPayload]
    );
    assert_eq!(
        parse_errors(
            "enum E { V(Int) } fn main() { let e = E::V(1); match e { E::V() => { }, E::A => { } } return; }"
        ),
        vec![ParseErrorKind::EmptyPayload]
    );
}

#[test]
fn unclosed_payload_is_a_parse_error() {
    assert_eq!(
        parse_errors("enum E { V(Int } fn main() {}"),
        vec![ParseErrorKind::ExpectedRParen]
    );
    assert_eq!(
        parse_errors("enum E { V(Int) } fn main() { let x = E::V(5; }"),
        vec![ParseErrorKind::ExpectedRParen]
    );
}

#[test]
fn multiple_payloads_are_rejected() {
    // A data-carrying variant carries exactly one payload; a second
    // payload type is an unexpected `,`.
    assert_eq!(
        parse_errors("enum E { V(Int, Int) } fn main() {}"),
        vec![ParseErrorKind::ExpectedRParen]
    );
}

// ---------------------------------------------------------------------------
// Type system: construction, payload patterns, and exhaustiveness
// ---------------------------------------------------------------------------

#[test]
fn construction_types_are_clean() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::B(5); let a = E::A; }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(!types.has_errors(), "{:?}", types.errors());
    let x_ty = symbol_type(&types, &semantic, "x");
    assert!(matches!(types.types().kind(x_ty), Some(TypeKind::Enum(_))));
    let a_ty = symbol_type(&types, &semantic, "a");
    assert_eq!(
        types.types().canonical(x_ty),
        types.types().canonical(a_ty),
        "all variants of an enum share its type"
    );
}

#[test]
fn construction_payload_mismatch_reports() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::B(true); }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::VariantPayloadMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T28");
    assert_eq!(errors[0].expected(), Some("Int"));
    assert_eq!(errors[0].actual(), Some("Bool"));
}

#[test]
fn payload_on_unit_variant_is_rejected() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::A(5); }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::VariantPayloadArity);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T29");
}

#[test]
fn missing_payload_is_rejected() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::B; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::VariantPayloadArity);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T29");
}

#[test]
fn unsupported_payload_types_are_rejected() {
    // Pointers, references, arrays, and function types cannot be payloads:
    // a payload must be a value type with a deterministic layout (E-T27).
    assert_eq!(
        type_errors(
            &check_src("enum E { V(Ptr<Int>) } fn main() {}").3,
            TypeErrorKind::InvalidVariantPayload
        )
        .len(),
        1
    );
    assert_eq!(
        type_errors(
            &check_src("enum E { V(&Int) } fn main() {}").3,
            TypeErrorKind::InvalidVariantPayload
        )
        .len(),
        1
    );
    assert_eq!(
        type_errors(
            &check_src("enum E { V([Int; 2]) } fn main() {}").3,
            TypeErrorKind::InvalidVariantPayload
        )
        .len(),
        1
    );
}

#[test]
fn tagged_enum_equality_is_rejected() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::B(1); let y = E::B(2); if x == y { rt_print_int(1); } return; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::EnumEquality);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T30");
}

#[test]
fn unit_only_enum_equality_is_preserved() {
    // Session 17 behavior: unit-only enums still compare by discriminant.
    let src = "enum D { A, B } fn main() { let x = D::A; let y = D::B; if x == y { rt_print_int(1); } return; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(!types.has_errors(), "{:?}", types.errors());
}

#[test]
fn payload_match_exhaustiveness_is_clean() {
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(1); match e { E::B(_) => { }, E::A => { } } return; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(!types.has_errors(), "{:?}", types.errors());
}

#[test]
fn missing_variant_in_payload_match_reports() {
    let src =
        "enum E { A, B(Int) } fn main() { let e = E::B(1); match e { E::B(_) => { } } return; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::NonExhaustiveMatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T24");
    assert!(
        errors[0].actual().unwrap().contains("`A`"),
        "message must name the missing variant: {}",
        errors[0].actual().unwrap()
    );
}

#[test]
fn payload_pattern_mismatch_reports() {
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(1); match e { E::B(true) => { }, E::A => { }, E::B(_) => { } } return; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert!(
        !errors.is_empty(),
        "a bool payload pattern cannot match Int"
    );
}

#[test]
fn repeated_payload_pattern_is_unreachable() {
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(1); match e { E::B(_) => { }, E::B(_) => { }, E::A => { } } return; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::UnreachableMatchArm);
    assert_eq!(errors.len(), 1, "{:?}", types.errors());
}

#[test]
fn literal_payload_patterns_are_not_unreachable() {
    // `E::B(1)` and `E::B(2)` cover different payload values, and the
    // trailing `E::B(_)` completes the variant — no arm is unreachable.
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(1); match e { E::B(1) => { }, E::B(2) => { }, E::B(_) => { }, E::A => { } } return; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(!types.has_errors(), "{:?}", types.errors());
}

#[test]
fn repeated_literal_payload_is_unreachable() {
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(1); match e { E::B(1) => { }, E::B(1) => { }, E::B(_) => { }, E::A => { } } return; }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::UnreachableMatchArm);
    assert_eq!(errors.len(), 1, "{:?}", types.errors());
}

#[test]
fn nested_payload_exhaustiveness() {
    // A payload that is itself an enum must have its variants covered
    // (recursively) for the match to be exhaustive.
    let clean = "enum E2 { X, Y } enum E { A, B(E2) } fn main() { let e = E::B(E2::X); match e { E::B(E2::X) => { }, E::B(E2::Y) => { }, E::A => { } } return; }";
    let (_s, _a, _semantic, types) = check_src(clean);
    assert!(!types.has_errors(), "{:?}", types.errors());

    let partial = "enum E2 { X, Y } enum E { A, B(E2) } fn main() { let e = E::B(E2::X); match e { E::B(E2::X) => { }, E::A => { } } return; }";
    let (_s, _a, _semantic, types) = check_src(partial);
    let errors = type_errors(&types, TypeErrorKind::NonExhaustiveMatch);
    assert_eq!(errors.len(), 1, "{:?}", types.errors());
}

#[test]
fn payload_bindings_type_from_the_payload() {
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(5); match e { E::B(x) => { rt_print_int(x + 1); }, E::A => { } } return; }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(!types.has_errors(), "{:?}", types.errors());
}

// ---------------------------------------------------------------------------
// Layout: tagged unions
// ---------------------------------------------------------------------------

#[test]
fn tagged_enum_layout_is_a_word_plus_payload() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::B(1); }";
    let (_s, _a, semantic, types) = check_src(src);
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
    assert_eq!(layout.variants.len(), 2);
    assert_eq!(layout.variants[0].size, 0, "unit variant has no payload");
    assert_eq!(layout.variants[1].size, 8, "Int payload is one word");
}

#[test]
fn tagged_enum_is_not_scalar() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::B(1); }";
    let (_s, _a, semantic, types) = check_src(src);
    let ty = symbol_type(&types, &semantic, "x");
    assert!(
        scalar_size_align(types.types(), ty).is_none(),
        "a tagged union is an aggregate, not a scalar"
    );
}

#[test]
fn unit_only_enum_stays_scalar() {
    let src = "enum E { A, B } fn main() { let x = E::A; }";
    let (_s, _a, semantic, types) = check_src(src);
    let ty = symbol_type(&types, &semantic, "x");
    assert_eq!(
        scalar_size_align(types.types(), ty),
        Some((8, 8)),
        "unit-only enums remain single-word discriminants"
    );
}

#[test]
fn recursive_enum_payload_is_rejected() {
    let src = "enum E { V(E) } fn main() {}";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidAggregateLayout);
    assert_eq!(errors.len(), 1, "{:?}", types.errors());
}

#[test]
fn mutually_recursive_enum_payloads_are_rejected() {
    let src = "enum A { X(B) } enum B { Y(A) } fn main() {}";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidAggregateLayout);
    assert!(!errors.is_empty(), "{:?}", types.errors());
}

// ---------------------------------------------------------------------------
// Ownership: payloads move
// ---------------------------------------------------------------------------

/// Runs the front end plus the ownership checker, returning all semantic
/// errors.
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

#[test]
fn construction_transfers_an_owned_payload() {
    // `E::V(s)` moves the owned string into the value; using `s` after is
    // a use of a moved value. (A string literal is an immutable constant
    // and copies, so the payload must be an owned allocation.)
    let src = "enum E { A, V(Str) } fn main() { let s = rt_str_alloc(2); let e = E::V(s); rt_print_str(s); }";
    let errors = check_ownership(src);
    let moved = errors
        .iter()
        .filter(|e| e.kind() == SemanticErrorKind::UseOfMovedValue)
        .count();
    assert_eq!(moved, 1, "{:?}", errors);
}

#[test]
fn copy_payload_construction_is_clean() {
    let src = "enum E { A, B(Int) } fn main() { let x = 5; let e = E::B(x); rt_print_int(x); }";
    let errors = check_ownership(src);
    assert!(errors.is_empty(), "{:?}", errors);
}

#[test]
fn payload_binding_moves_out_of_the_scrutinee() {
    // Matching `E::V(x)` moves the owned payload into `x`; the scrutinee
    // enum's payload is consumed, so using it after the match is an error.
    let src = "enum E { A, V(Str) } fn main() { let s = rt_str_alloc(2); let e = E::V(s); match e { E::V(x) => { rt_print_str(x); }, E::A => { } } match e { E::V(_) => { }, E::A => { } } return; }";
    let errors = check_ownership(src);
    let moved = errors
        .iter()
        .filter(|e| e.kind() == SemanticErrorKind::UseOfMovedValue)
        .count();
    assert!(moved >= 1, "{:?}", errors);
}

#[test]
fn copy_payload_binding_leaves_the_scrutinee_usable() {
    // An `Int` payload is Copy: binding it does not consume the enum.
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(5); match e { E::B(x) => { rt_print_int(x); }, E::A => { } } match e { E::B(x) => { rt_print_int(x); }, E::A => { } } return; }";
    let errors = check_ownership(src);
    assert!(errors.is_empty(), "{:?}", errors);
}

#[test]
fn unit_variant_matching_does_not_move() {
    let src = "enum E { A, B(Str) } fn main() { let s = \"hi\"; let e = E::A; match e { E::A => { }, E::B(_) => { } } match e { E::A => { }, E::B(_) => { } } return; }";
    let errors = check_ownership(src);
    assert!(errors.is_empty(), "{:?}", errors);
}

// ---------------------------------------------------------------------------
// MIR lowering
// ---------------------------------------------------------------------------

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

#[test]
fn construction_lowers_to_enum_init() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::B(5); }";
    let (_hir, mir) = lower_mir(src);
    let main = mir_fn(&mir, "main");
    assert!(
        main.blocks.iter().any(|block| {
            block.stmts.iter().any(|stmt| {
                let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
                matches!(
                    &rvalue.kind,
                    MirRvalueKind::EnumInit {
                        discriminant: 1,
                        payload: Some(_),
                    }
                )
            })
        }),
        "data-carrying construction must lower to EnumInit with the payload"
    );
}

#[test]
fn payload_match_lowers_to_tag_and_payload() {
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(5); match e { E::B(x) => { rt_print_int(x); }, E::A => { } } return; }";
    let (_hir, mir) = lower_mir(src);
    let main = mir_fn(&mir, "main");
    let has_tag = main.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            matches!(&rvalue.kind, MirRvalueKind::EnumTag { .. })
        })
    });
    assert!(has_tag, "a payload match must extract the discriminant tag");
    let has_payload = main.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            matches!(&rvalue.kind, MirRvalueKind::EnumPayload { .. })
        })
    });
    assert!(has_payload, "a payload binding must extract the payload");
}

#[test]
fn payload_match_lowering_is_deterministic() {
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(5); match e { E::B(x) => { rt_print_int(x); }, E::A => { } } return; }";
    let (first_hir, first_mir) = lower_mir(src);
    let (second_hir, second_mir) = lower_mir(src);
    assert_eq!(first_mir, second_mir);
    assert_eq!(first_hir, second_hir);
}

// ---------------------------------------------------------------------------
// Backend lowering and verification
// ---------------------------------------------------------------------------

/// Runs the front end on `src` and lowers the optimized MIR into backend
/// instructions, asserting every stage is clean.
fn lower_backend(src: &str) -> (MirProgram, mink::backend::BProgram) {
    let mut sources = SourceMap::new();
    let name = std::thread::current()
        .name()
        .unwrap_or("backend")
        .replace("::", "_");
    let path = std::env::temp_dir().join(format!(
        "mink_sumtypes_test_{}_{name}.mink",
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

#[test]
fn tagged_enum_locals_are_multi_word() {
    let src = "enum E { A, B(Int) } fn main() { let x = E::B(5); }";
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
fn tagged_enum_results_are_rejected() {
    // The calling convention returns one word; a tagged union cannot be a
    // function result.
    let src = "enum E { A, B(Int) } fn id(x) { return x; } fn main() { let x = id(E::B(5)); }";
    let mut sources = SourceMap::new();
    let name = std::thread::current()
        .name()
        .unwrap_or("result")
        .replace("::", "_");
    let path = std::env::temp_dir().join(format!(
        "mink_sumtypes_result_{}_{name}.mink",
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
    assert!(
        backend::lower(mir, &sources).is_err(),
        "a tagged-union result must be rejected by the backend"
    );
}

// ---------------------------------------------------------------------------
// Native execution
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("mink_sumtypes_test_{}_{name}", std::process::id()));
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
fn native_payload_match_extracts_and_prints() {
    let exe = build(
        "enum Shape { Nothing, Point(Int) }
         fn main() {
             let s = Shape::Point(42);
             match s {
                 Shape::Nothing => { rt_print_int(0); },
                 Shape::Point(x) => { rt_print_int(x); },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "42");
}

#[test]
fn native_payload_match_dispatches_between_variants() {
    let exe = build(
        "enum Shape { Circle(Int), Square(Int), Nothing }
         fn area(s) {
             match s {
                 Shape::Circle(r) => { return r * r * 3; },
                 Shape::Square(x) => { return x * x; },
                 Shape::Nothing => { return 0; },
             }
         }
         fn main() {
             rt_print_int(area(Shape::Circle(2)));
             rt_print_int(area(Shape::Square(5)));
             rt_print_int(area(Shape::Nothing));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "12\n25\n0");
}

#[test]
fn native_string_payloads_round_trip() {
    let exe = build(
        "enum E { A, V(Str) }
         fn pick(e) {
             match e {
                 E::V(s) => { rt_print_str(s); },
                 E::A => { rt_print_int(9); },
             }
         }
         fn main() {
             pick(E::V(\"hello\"));
             pick(E::A);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "hello\n9");
}

#[test]
fn native_struct_payloads_flow_through_matches() {
    let exe = build(
        "struct Pt { x: Int, y: Int }
         enum E { Origin, At(Pt) }
         fn sum(e) {
             match e {
                 E::At(p) => { return p.x + p.y; },
                 E::Origin => { return 0; },
             }
         }
         fn main() {
             rt_print_int(sum(E::Origin));
             rt_print_int(sum(E::At(Pt { x: 3, y: 4 })));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "0\n7");
}

#[test]
fn native_nested_enum_payloads_match() {
    let exe = build(
        "enum Inner { X, Y }
         enum Outer { Wrap(Inner), Bare }
         fn unwrap(o) {
             match o {
                 Outer::Wrap(Inner::X) => { return 1; },
                 Outer::Wrap(Inner::Y) => { return 2; },
                 Outer::Bare => { return 3; },
             }
         }
         fn main() {
             rt_print_int(unwrap(Outer::Wrap(Inner::Y)));
             rt_print_int(unwrap(Outer::Wrap(Inner::X)));
             rt_print_int(unwrap(Outer::Bare));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "2\n1\n3");
}

#[test]
fn native_unit_variants_still_work_alongside_payloads() {
    let exe = build(
        "enum D { A, B(Int), C }
         fn main() {
             let mut e = D::C;
             e = D::B(7);
             match e {
                 D::A => { rt_print_int(1); },
                 D::B(x) => { rt_print_int(x); },
                 D::C => { rt_print_int(3); },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "7");
}

#[test]
fn native_images_are_deterministic_for_sum_types() {
    let src = "enum E { A, B(Int) } fn main() { let e = E::B(5); match e { E::B(x) => { rt_print_int(x); }, E::A => { } } return; }";
    let first = emit_image(src);
    let second = emit_image(src);
    assert_eq!(first, second);
}

fn emit_image(src: &str) -> mink::backend::EmittedImage {
    let mut sources = SourceMap::new();
    let name = std::thread::current()
        .name()
        .unwrap_or("image")
        .replace("::", "_");
    let path = std::env::temp_dir().join(format!(
        "mink_sumtypes_img_{}_{name}.mink",
        std::process::id()
    ));
    std::fs::write(&path, src).unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let mir = report.mir.expect("clean program");
    backend::compile(&mir, &sources, mink::backend::Target::native()).expect("compile succeeds")
}
