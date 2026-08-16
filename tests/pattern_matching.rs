//! Integration tests for the pattern matching foundation (session 18):
//! `match` statements over scalar values — `Int` (integer-literal
//! patterns), `Bool` (`true`/`false`), and enums (variant paths `E::V`) —
//! plus `_` wildcard and `name` binding patterns, first-match-wins
//! semantics, compile-time exhaustiveness (E-T24), unreachable-arm
//! rejection (E-T25), invalid scrutinee rejection (E-T26), diagnostics
//! (parser, semantic, type), HIR/MIR/backend lowering, and native
//! execution.
//!
//! The rules under test are documented in
//! `docs/implementation/PATTERN_MATCHING_IMPLEMENTATION.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

use mink::ast::{Ast, ItemKind, StmtKind};
use mink::backend::{self, BType};
use mink::mir::{
    self, MirFn, MirItemKind, MirOperandKind, MirProgram, MirRvalueKind, MirStmtKind, MirTargetKind,
};
use mink::parser::{ParseErrorKind, ParseOutput, parse};
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

/// All semantic errors of `kind`.
fn semantic_errors(
    semantic: &SemanticResult,
    kind: SemanticErrorKind,
) -> Vec<&mink::semantics::SemanticError> {
    semantic
        .errors()
        .iter()
        .filter(|error| error.kind() == kind)
        .collect()
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

/// The `match` statements of the function named `fn_name`, in source order.
fn match_stmts<'a>(ast: &'a Ast, fn_name: &str) -> Vec<&'a mink::ast::MatchStmt> {
    let f = ast
        .items()
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Fn(f) if f.name.name == fn_name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no function named `{fn_name}`"));
    let mut matches = Vec::new();
    for stmt in &f.body.stmts {
        if let StmtKind::Match(m) = &stmt.kind {
            matches.push(m);
        }
    }
    matches
}

// ---------------------------------------------------------------------------
// Parser: match statements and patterns
// ---------------------------------------------------------------------------

#[test]
fn match_statement_parses() {
    let src = "fn main() { match x { 1 => { a(); }, _ => { b(); } } }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let matches = match_stmts(&ast, "main");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].arms.len(), 2);
}

#[test]
fn match_with_trailing_comma_parses() {
    let src = "fn main() { match x { 1 => { a(); }, _ => { b(); }, } }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
}

#[test]
fn match_single_arm_needs_no_comma() {
    // The frozen grammar requires commas between arms (a trailing comma is
    // allowed); a single arm needs no comma.
    let src = "fn main() { match x { _ => { a(); } } }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
}

#[test]
fn all_pattern_forms_parse() {
    let src = "enum E { A } fn main() { match x {
        1 => { a(); },
        -5 => { b(); },
        true => { c(); },
        false => { d(); },
        E::A => { e(); },
        name => { f(); },
        _ => { g(); },
    } }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let matches = match_stmts(&ast, "main");
    assert_eq!(matches[0].arms.len(), 7);
    let kinds: Vec<String> = matches[0]
        .arms
        .iter()
        .map(|arm| match &arm.pattern {
            mink::ast::Pattern::Int { negative, .. } => {
                format!("int({negative})")
            }
            mink::ast::Pattern::Bool { value, .. } => format!("bool({value})"),
            mink::ast::Pattern::EnumVariant {
                name,
                variant,
                payload,
            } => {
                let mut rendered = format!("variant({}::{})", name.name, variant.name);
                if payload.is_some() {
                    rendered.push_str("(payload)");
                }
                rendered
            }
            mink::ast::Pattern::Binding(ident) => format!("binding({})", ident.name),
            mink::ast::Pattern::Wildcard { .. } => "wildcard".to_string(),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "int(false)",
            "int(true)",
            "bool(true)",
            "bool(false)",
            "variant(E::A)",
            "binding(name)",
            "wildcard"
        ]
    );
}

#[test]
fn match_parse_errors_are_structured() {
    // Missing `=>` after a pattern: expected-fat-arrow (E-P24).
    let errors = parse_errors("fn main() { match x { 1 { } } }");
    assert!(
        errors.contains(&ParseErrorKind::ExpectedFatArrow),
        "{errors:?}"
    );

    // A non-pattern (a string literal) where a pattern is required: E-P23.
    let errors = parse_errors("fn main() { match x { \"s\" => { } } }");
    assert!(
        errors.contains(&ParseErrorKind::ExpectedPattern),
        "{errors:?}"
    );

    // Missing block after `=>`.
    let errors = parse_errors("fn main() { match x { 1 => 2 } }");
    assert!(
        errors.contains(&ParseErrorKind::ExpectedBlock),
        "{errors:?}"
    );

    // Missing arm-list brace at end of input.
    let errors = parse_errors("fn main() { match x { 1 => { } ");
    assert!(
        errors.contains(&ParseErrorKind::UnclosedBrace),
        "{errors:?}"
    );

    // Missing scrutinee block opener.
    let errors = parse_errors("fn main() { match x 1 => { } }");
    assert!(
        errors.contains(&ParseErrorKind::ExpectedBlock),
        "{errors:?}"
    );
}

#[test]
fn bad_match_arm_recovers_to_next_arm() {
    // A malformed first arm (missing `=>`) must not swallow the second arm.
    let src = "fn main() { match x { 1 { }, _ => { } } }";
    let parsed = parse_src(src);
    let (ast, _, errors) = parsed.into_parts();
    assert!(!errors.is_empty());
    let matches = match_stmts(&ast, "main");
    assert!(
        !matches[0].arms.is_empty(),
        "recovery should keep at least one arm"
    );
}

// ---------------------------------------------------------------------------
// Semantics: pattern bindings and arm scope
// ---------------------------------------------------------------------------

#[test]
fn pattern_binding_resolves_in_arm_body() {
    let src = "fn f() { match 5 { n => { rt_print_int(n); }, } }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert!(semantic.errors().is_empty(), "{:?}", semantic.errors());
    // The binding `n` resolves: it must be a symbol with the scrutinee's
    // type (Int).
    let ty = symbol_type(&types, &semantic, "n");
    assert!(matches!(types.types().kind(ty), Some(TypeKind::Int)));
}

#[test]
fn pattern_binding_shadows_outer_name() {
    let src = "fn f() { let x = 1; match x { x => { rt_print_int(x); }, } }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert!(semantic.errors().is_empty(), "{:?}", semantic.errors());
    // The arm's `x` is a distinct symbol from the outer `let x`.
    let symbols: Vec<&str> = semantic
        .symbols()
        .iter()
        .filter(|s| s.name == "x")
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(symbols.len(), 2, "outer binding plus the arm binding");
}

#[test]
fn pattern_binding_is_immutable() {
    let src = "fn f() { match 5 { n => { n = 6; }, } }";
    let (_s, _a, semantic, _types) = check_src(src);
    assert_eq!(
        semantic_errors(&semantic, SemanticErrorKind::AssignmentToImmutable).len(),
        1
    );
}

#[test]
fn break_and_continue_inside_match_arms_reach_the_loop() {
    // Match arms inherit the enclosing loop context: `break` and
    // `continue` inside an arm are valid (no semantic errors), and a
    // `break` outside any loop is still rejected.
    let src = "fn f() { loop { match 1 { _ => { break; } } } }";
    let (_s, _a, semantic, _types) = check_src(src);
    assert!(semantic.errors().is_empty(), "{:?}", semantic.errors());

    let src = "fn f() { match 1 { _ => { break; } } }";
    let (_s, _a, semantic, _types) = check_src(src);
    assert_eq!(
        semantic_errors(&semantic, SemanticErrorKind::BreakOutsideLoop).len(),
        1
    );
}

// ---------------------------------------------------------------------------
// Type system: scrutinee typing, pattern typing, exhaustiveness
// ---------------------------------------------------------------------------

#[test]
fn match_scrutinee_type_is_recorded() {
    let src = "enum E { A, B } fn main() { match E::A { E::A => { }, E::B => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn enum_patterns_pin_an_unresolved_scrutinee() {
    // A function parameter's type is pinned by the variant patterns; the
    // match is exhaustive over all variants without a catch-all.
    let src =
        "enum E { A, B } fn f(p) { match p { E::A => { }, E::B => { } } } fn main() { f(E::A); }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn int_patterns_pin_an_unresolved_scrutinee() {
    let src = "fn f(p) { match p { 1 => { }, _ => { } } } fn main() { f(1); }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    let p = symbol_type(&types, &_semantic, "p");
    assert!(matches!(types.types().kind(p), Some(TypeKind::Int)));
}

#[test]
fn pattern_mismatching_the_scrutinee_type_is_rejected() {
    // An integer pattern on a Bool scrutinee: type mismatch.
    let src = "fn main() { let b = true; match b { 1 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(
        !type_errors(&types, TypeErrorKind::TypeMismatch).is_empty(),
        "{:?}",
        types.errors()
    );

    // A variant pattern on an Int scrutinee: type mismatch.
    let src = "enum E { A } fn main() { match 1 { E::A => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(
        !type_errors(&types, TypeErrorKind::TypeMismatch).is_empty(),
        "{:?}",
        types.errors()
    );
}

#[test]
fn unknown_variant_pattern_is_rejected() {
    let src = "enum E { A } fn main() { match E::A { E::B => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::UnknownVariant).len(), 1);
}

#[test]
fn variant_pattern_on_a_non_enum_type_is_rejected() {
    let src = "struct S { x: Int } fn main() { match 1 { S::Q => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::NotAnEnum).len(), 1);
}

#[test]
fn non_matchable_scrutinees_are_rejected() {
    // Structs, arrays, strings, floats, and chars are not matchable in
    // this milestone: each is a single E-T26.
    let programs = [
        "struct S { x: Int } fn main() { let s = S { x: 1 }; match s { 1 => { }, _ => { } } }",
        "fn main() { let a = [1, 2]; match a { 1 => { }, _ => { } } }",
        "fn main() { let s = \"hi\"; match s { 1 => { }, _ => { } } }",
        "fn main() { let f = 1.5; match f { 1 => { }, _ => { } } }",
        "fn main() { let c = 'a'; match c { 1 => { }, _ => { } } }",
    ];
    for src in programs {
        let (_s, _a, _semantic, types) = check_src(src);
        assert_eq!(
            type_errors(&types, TypeErrorKind::InvalidMatchScrutinee).len(),
            1,
            "for {src}: {:?}",
            types.errors()
        );
    }
}

#[test]
fn non_exhaustive_enum_match_is_rejected() {
    let src = "enum E { A, B, C } fn main() { let e = E::A; match e { E::A => { }, E::B => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::NonExhaustiveMatch);
    assert_eq!(errors.len(), 1, "{:?}", types.errors());
    // The diagnostic names the missing variant.
    assert!(
        errors[0]
            .actual()
            .is_some_and(|detail| detail.contains("`C`")),
        "missing variant must be named: {:?}",
        errors[0].actual()
    );
}

#[test]
fn exhaustive_enum_match_needs_no_catch_all() {
    let src = "enum E { A, B, C } fn main() { let e = E::A; match e { E::A => { }, E::B => { }, E::C => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn int_match_requires_a_catch_all() {
    let src = "fn main() { let x = 1; match x { 1 => { }, 2 => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::NonExhaustiveMatch).len(),
        1
    );
}

#[test]
fn bool_match_requires_both_values_or_a_catch_all() {
    // Only `true`: not exhaustive.
    let src = "fn main() { let b = true; match b { true => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::NonExhaustiveMatch).len(),
        1
    );

    // Both values: exhaustive without a catch-all.
    let src = "fn main() { let b = true; match b { true => { }, false => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn empty_enum_match_is_vacuously_exhaustive() {
    // `enum Empty {}` has no constructible values: a zero-arm match (or a
    // catch-all) is exhaustive.
    let src = "enum Empty { } fn f(e) { match e { } } fn main() { }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn arms_after_a_catch_all_are_unreachable() {
    let src = "enum E { A } fn main() { let e = E::A; match e { _ => { }, E::A => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );

    let src = "fn main() { let x = 1; match x { n => { }, 1 => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
}

#[test]
fn duplicate_patterns_are_unreachable() {
    // The same variant twice: the second arm can never run.
    let src = "enum E { A, B } fn main() { let e = E::A; match e { E::A => { }, E::A => { }, E::B => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );

    // The same integer literal twice (different spellings, same value).
    let src = "fn main() { let x = 5; match x { 5 => { }, 0x5 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );

    // The same boolean twice.
    let src = "fn main() { let b = true; match b { true => { }, true => { }, false => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
}

#[test]
fn binding_pattern_type_matches_the_scrutinee() {
    // A binding copies the scrutinee value: its type is the enum type.
    let src = "enum E { A, B } fn main() { let e = E::A; match e { v => { let w = v; } } }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    let v = symbol_type(&types, &semantic, "v");
    assert!(
        matches!(types.types().kind(v), Some(TypeKind::Enum(_))),
        "binding must have the enum type"
    );
}

// ---------------------------------------------------------------------------
// HIR lowering
// ---------------------------------------------------------------------------

/// Parses, analyzes, type-checks, and lowers through HIR.
fn lower_hir(src: &str) -> (mink::hir::HirProgram, mir::MirProgram) {
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

#[test]
fn match_lowers_to_hir() {
    let src = "enum E { A } fn main() { match E::A { E::A => { }, _ => { } } }";
    let (hir, _mir) = lower_hir(src);
    let main = hir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            mink::hir::HirItemKind::Fn(f) if f.name.name == "main" => Some(f),
            _ => None,
        })
        .unwrap();
    let m = main
        .body
        .stmts
        .iter()
        .find_map(|stmt| match &stmt.kind {
            mink::hir::HirStmtKind::Match(m) => Some(m),
            _ => None,
        })
        .expect("match must lower to HIR");
    assert_eq!(m.arms.len(), 2);
    assert!(matches!(
        &m.arms[0].pattern,
        mink::hir::HirPattern::EnumVariant { .. }
    ));
    assert!(matches!(
        &m.arms[1].pattern,
        mink::hir::HirPattern::Wildcard { .. }
    ));
}

#[test]
fn match_binding_lowers_to_a_resolved_identifier() {
    let src = "fn main() { match 5 { n => { rt_print_int(n); } } }";
    let (hir, _mir) = lower_hir(src);
    let main = hir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            mink::hir::HirItemKind::Fn(f) if f.name.name == "main" => Some(f),
            _ => None,
        })
        .unwrap();
    let m = main
        .body
        .stmts
        .iter()
        .find_map(|stmt| match &stmt.kind {
            mink::hir::HirStmtKind::Match(m) => Some(m),
            _ => None,
        })
        .unwrap();
    let mink::hir::HirPattern::Binding(ident) = &m.arms[0].pattern else {
        panic!("expected a binding pattern");
    };
    assert_eq!(ident.name, "n");
    assert!(
        matches!(hir.types.kind(ident.ty), Some(TypeKind::Int)),
        "binding must be typed Int"
    );
}

// ---------------------------------------------------------------------------
// MIR lowering: branch chain and binding copies
// ---------------------------------------------------------------------------

/// The MIR function named `name`.
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
/// constant with discriminant `discriminant`.
fn compares_against_enum_discriminant(f: &MirFn, discriminant: u32) -> bool {
    f.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            let MirRvalueKind::Binary { op, rhs, .. } = &rvalue.kind else {
                return false;
            };
            *op == mink::ast::BinaryOp::Eq
                && matches!(
                    &rhs.kind,
                    MirOperandKind::Constant(c)
                        if matches!(c.kind, mir::MirConstantKind::Enum { variant } if variant == discriminant)
                )
        })
    })
}

#[test]
fn match_lowers_to_enum_discriminant_comparisons() {
    let src = "enum D { A, B, C } fn main() { let d = D::A; match d { D::A => { }, D::B => { }, D::C => { } } }";
    let (_hir, mir) = lower_hir(src);
    let main = mir_fn(&mir, "main");
    // Every variant pattern must lower to an `==` comparison against its
    // discriminant.
    assert!(compares_against_enum_discriminant(main, 0), "A");
    assert!(compares_against_enum_discriminant(main, 1), "B");
    assert!(compares_against_enum_discriminant(main, 2), "C");
}

#[test]
fn int_pattern_lowers_to_an_equality_comparison() {
    let src = "fn main() { let x = 5; match x { 5 => { }, _ => { } } }";
    let (_hir, mir) = lower_hir(src);
    let main = mir_fn(&mir, "main");
    let finds_int_comparison = main.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            let MirRvalueKind::Binary { op, rhs, .. } = &rvalue.kind else {
                return false;
            };
            *op == mink::ast::BinaryOp::Eq
                && matches!(&rhs.kind, MirOperandKind::Constant(c) if matches!(c.kind, mir::MirConstantKind::Int))
        })
    });
    assert!(
        finds_int_comparison,
        "int pattern must lower to == against the literal"
    );
}

#[test]
fn binding_pattern_copies_the_scrutinee() {
    let src = "fn main() { match 7 { n => { rt_print_int(n); } } }";
    let (_hir, mir) = lower_hir(src);
    let main = mir_fn(&mir, "main");
    let binding_index = main
        .locals
        .iter()
        .position(|l| l.name == "n")
        .expect("binding local `n` exists");
    let binding_id = mir::LocalId::new(binding_index as u32);
    // The binding local must be assigned somewhere in the function (the
    // scrutinee value, folded to a constant when the scrutinee is a
    // literal).
    let assigned = main.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { target, .. } = &stmt.kind;
            matches!(target.kind, MirTargetKind::Local(id) if id == binding_id)
        })
    });
    assert!(assigned, "the binding local must be assigned");
}

// ---------------------------------------------------------------------------
// Backend lowering and verification
// ---------------------------------------------------------------------------

/// Runs the front end on `src` and lowers the optimized MIR into backend
/// instructions, asserting every stage is clean.
fn lower_backend(src: &str) -> (mir::MirProgram, mink::backend::BProgram) {
    let mut sources = SourceMap::new();
    let name = std::thread::current()
        .name()
        .unwrap_or("backend")
        .replace("::", "_");
    let path = std::env::temp_dir().join(format!(
        "mink_match_test_{}_{name}.mink",
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
fn match_lowers_through_the_backend() {
    let src = "enum E { A, B } fn main() { let e = E::A; match e { E::A => { rt_print_int(1); }, E::B => { rt_print_int(2); } } return; }";
    let (_mir, program) = lower_backend(src);
    let main = program
        .functions
        .iter()
        .find(|f| f.name == "main")
        .expect("main lowered");
    // The scrutinee local is enum-typed; the match produces branches.
    assert!(
        main.locals.iter().any(|l| l.ty == BType::Enum),
        "enum scrutinee local must lower to BType::Enum"
    );
    assert!(
        main.blocks
            .iter()
            .any(|b| matches!(b.terminator, mink::backend::BTerminator::Branch { .. })),
        "match must lower to conditional branches"
    );
}

#[test]
fn match_lowering_is_deterministic() {
    let src = "enum E { A, B } fn main() { let e = E::A; match e { E::A => { }, E::B => { } } match 3 { 1 => { }, _ => { } } return; }";
    let (_mir, first) = lower_backend(src);
    let (_mir, second) = lower_backend(src);
    assert_eq!(first, second);
}

/// Compiles `src` all the way to an image.
fn emit_image(src: &str) -> mink::backend::EmittedImage {
    let mut sources = SourceMap::new();
    let name = std::thread::current()
        .name()
        .unwrap_or("image")
        .replace("::", "_");
    let path =
        std::env::temp_dir().join(format!("mink_match_img_{}_{name}.mink", std::process::id()));
    std::fs::write(&path, src).unwrap();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    let mir = report.mir.expect("clean program");
    backend::compile(&mir, &sources, mink::backend::Target::native()).expect("compile succeeds")
}

#[test]
fn match_images_are_byte_identical() {
    let src = "enum E { A, B, C } fn main() { let e = E::B; match e { E::A => { }, E::B => { }, E::C => { } } return; }";
    assert_eq!(emit_image(src).bytes, emit_image(src).bytes);
}

// ---------------------------------------------------------------------------
// Native execution
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mink_match_test_{}_{name}", std::process::id()));
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
fn native_enum_match_all_variants() {
    let exe = build(
        "enum D { North, South, East, West }
         fn label(d) {
             match d {
                 D::North => { return 10; },
                 D::South => { return 20; },
                 D::East => { return 30; },
                 D::West => { return 40; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(label(D::North));
             rt_print_int(label(D::South));
             rt_print_int(label(D::East));
             rt_print_int(label(D::West));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "10\n20\n30\n40");
}

#[test]
fn native_enum_match_with_catch_all() {
    let exe = build(
        "enum D { A, B, C }
         fn main() {
             let mut d = D::A;
             match d {
                 D::A => { rt_print_int(1); },
                 D::B => { rt_print_int(2); },
                 _ => { rt_print_int(3); },
             }
             d = D::C;
             match d {
                 D::A => { rt_print_int(4); },
                 D::B => { rt_print_int(5); },
                 _ => { rt_print_int(6); },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "1\n6");
}

#[test]
fn native_int_match_first_match_wins() {
    let exe = build(
        "fn main() {
             match 5 {
                 1 => { rt_print_int(1); },
                 5 => { rt_print_int(2); },
                 _ => { rt_print_int(3); },
             }
             match 0 {
                 0 => { rt_print_int(4); },
                 _ => { rt_print_int(5); },
             }
             match 100 {
                 0 => { rt_print_int(6); },
                 1 => { rt_print_int(7); },
                 n => { rt_print_int(n); },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "2\n4\n100");
}

#[test]
fn native_negative_int_patterns() {
    let exe = build(
        "fn main() {
             match -7 {
                 -1 => { rt_print_int(1); },
                 -7 => { rt_print_int(2); },
                 _ => { rt_print_int(3); },
             }
             match 7 {
                 -7 => { rt_print_int(4); },
                 _ => { rt_print_int(5); },
             }
             let x = -3;
             match x {
                 -3 => { rt_print_int(6); },
                 _ => { rt_print_int(7); },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "2\n5\n6");
}

#[test]
fn native_bool_match() {
    let exe = build(
        "fn main() {
             match true {
                 true => { rt_print_int(1); },
                 false => { rt_print_int(2); },
             }
             match false {
                 true => { rt_print_int(3); },
                 false => { rt_print_int(4); },
             }
             let mut b = true;
             b = false;
             match b {
                 true => { rt_print_int(5); },
                 _ => { rt_print_int(6); },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "1\n4\n6");
}

#[test]
fn native_binding_pattern_copies_the_scrutinee() {
    let exe = build(
        "fn main() {
             match 42 {
                 n => { rt_print_int(n); },
             }
             let x = 8;
             match x {
                 y => { rt_print_int(y * 2); },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "42\n16");
}

#[test]
fn native_match_through_struct_member() {
    // The scrutinee is a member access on a function parameter whose type
    // is resolved by the call site after the body is checked: the deferred
    // re-type pass must still type the patterns and exhaustiveness.
    let exe = build(
        "enum Color { Red, Green, Blue }
         struct Tag { c: Color, id: Int }
         fn label(t) {
             match t.c {
                 Color::Red => { return 10; },
                 Color::Green => { return 20; },
                 Color::Blue => { return 30; },
             }
             return 99;
         }
         fn main() {
             let t = Tag { c: Color::Green, id: 5 };
             rt_print_int(label(t));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "20");
}

#[test]
fn native_match_inside_loops() {
    let exe = build(
        "fn main() {
             let mut total = 0;
             for i in 0..5 {
                 match i {
                     0 => { total = total + 1; },
                     3 => { total = total + 10; },
                     _ => { total = total + 100; },
                 }
             }
             rt_print_int(total);
             let mut count = 0;
             loop {
                 match count {
                     0 => { count = count + 1; },
                     2 => { break; },
                     _ => { count = count + 1; },
                 }
             }
             rt_print_int(count);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "311\n2");
}

#[test]
fn native_nested_match() {
    let exe = build(
        "enum D { A, B }
         fn main() {
             let d = D::A;
             match d {
                 D::A => {
                     match 1 {
                         1 => { rt_print_int(7); },
                         _ => { rt_print_int(8); },
                     }
                 },
                 _ => {
                     match 2 {
                         2 => { rt_print_int(9); },
                         _ => { rt_print_int(10); },
                     }
                 },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "7");
}

#[test]
fn native_match_exit_code_from_int() {
    // A match can drive the function's return value.
    let exe = build(
        "fn pick(x) {
             match x {
                 1 => { return 10; },
                 2 => { return 20; },
                 _ => { return 30; },
             }
         }
         fn main() {
             return pick(2);
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 20);
    assert_eq!(stdout, b"");
}

#[test]
fn native_many_arms_stay_word_sized() {
    // 100 arms over 100 variants: every discriminant still fits one word,
    // and the match dispatches correctly.
    let variants = (0..100)
        .map(|i| format!("V{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let arms = (0..100)
        .map(|i| format!("Big::V{i} => {{ return {i}; }},"))
        .collect::<Vec<_>>()
        .join(" ");
    let src = format!(
        "enum Big {{ {variants} }}
         fn pick(b) {{ match b {{ {arms} _ => {{ return 999; }} }} }}
         fn main() {{
             rt_print_int(pick(Big::V0));
             rt_print_int(pick(Big::V50));
             rt_print_int(pick(Big::V99));
             return;
         }}"
    );
    let exe = build(&src);
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "0\n50\n99");
}
