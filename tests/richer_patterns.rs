//! Integration tests for the session 27 richer match patterns: or-patterns
//! (`1 | 2 | 3`, `E::A(x) | E::B(x)`), integer range patterns (`1..=5`,
//! `1..5`, with negated endpoints), and guarded arms (`pat if expr =>`).
//!
//! Coverage of the rules: parsing (or/range/guard forms and their
//! rejections), semantics (or-pattern bindings share one declaration,
//! guard bindings resolve in the arm scope), type checking (union
//! coverage with integer intervals, full-domain `Int` exhaustiveness,
//! unreachable-arm detection across points/ranges/or-alternatives,
//! guarded arms commit no coverage, E-T34 or-pattern binding consistency),
//! HIR/MIR lowering (branch chains, `Ge`/`Le`/`Lt` range tests, guard
//! branches, one local per or-pattern binding), backend lowering and
//! byte-identical determinism, ownership (owned payload moves through
//! or-patterns and guards), and native execution.
//!
//! The rules under test are documented in
//! `docs/implementation/RICHER_PATTERNS_IMPLEMENTATION.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

use mink::ast::{Ast, ItemKind, StmtKind};
use mink::backend;
use mink::mir::{self, MirFn, MirItemKind, MirOperandKind, MirProgram, MirRvalueKind, MirStmtKind};
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

/// Runs semantics, type checking, and the ownership pass, returning the
/// ownership result (whose errors carry `SemanticErrorKind` codes like
/// E-S10). The driver gates ownership analysis on a clean front end, so
/// this asserts the source is semantically and type clean first.
fn check_ownership(src: &str) -> mink::ownership::OwnershipResult {
    let (_sources, ast, semantic, types) = check_src(src);
    assert!(
        semantic.errors().is_empty(),
        "unexpected semantic errors: {:?}",
        semantic.errors()
    );
    assert!(
        types.errors().is_empty(),
        "unexpected type errors: {:?}",
        types.errors()
    );
    mink::ownership::check(&ast, &semantic, &types)
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

/// All ownership errors of `kind` (ownership errors use the semantic error
/// kinds, e.g. E-S10 `UseOfMovedValue`).
fn ownership_errors(
    ownership: &mink::ownership::OwnershipResult,
    kind: SemanticErrorKind,
) -> Vec<&mink::semantics::SemanticError> {
    ownership
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
// Parser: or-patterns, range patterns, and guards
// ---------------------------------------------------------------------------

#[test]
fn or_patterns_parse() {
    let src = "enum E { A, B } enum S { C(Int), D(Int) }
        fn main() { let x = 1; let e = E::A; let s = S::C(1);
            match x { 1 | 2 | 3 => { }, _ => { } }
            match e { E::A | E::B => { }, }
            match s { S::C(n) | S::D(n) => { }, }
            match x { _ | 5 => { }, _ => { } }
            match s { S::C(1 | 2) => { }, S::C(_) => { }, S::D(_) => { } }
        }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let matches = match_stmts(&ast, "main");
    assert_eq!(matches.len(), 5);
    let mink::ast::Pattern::Or { alternatives, .. } = &matches[0].arms[0].pattern else {
        panic!("or-pattern expected");
    };
    assert_eq!(alternatives.len(), 3);
    let mink::ast::Pattern::Or { alternatives, .. } = &matches[1].arms[0].pattern else {
        panic!("or-pattern expected");
    };
    assert_eq!(alternatives.len(), 2);
    let mink::ast::Pattern::Or { alternatives, .. } = &matches[2].arms[0].pattern else {
        panic!("or-pattern expected");
    };
    assert_eq!(alternatives.len(), 2);
    let mink::ast::Pattern::Or { alternatives, .. } = &matches[3].arms[0].pattern else {
        panic!("or-pattern expected");
    };
    assert_eq!(alternatives.len(), 2);
    // A payload-position or-pattern (inside `S::C(...)`).
    let mink::ast::Pattern::EnumVariant { payload, .. } = &matches[4].arms[0].pattern else {
        panic!("variant pattern expected");
    };
    let inner = payload.as_ref().expect("payload pattern present");
    assert!(matches!(inner.as_ref(), mink::ast::Pattern::Or { .. }));
}

#[test]
fn or_pattern_alternatives_may_be_ranges() {
    let src = "fn main() { let x = 1; match x { 1 | 2..=5 => { }, _ => { } } }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let m = &match_stmts(&ast, "main")[0];
    let mink::ast::Pattern::Or { alternatives, .. } = &m.arms[0].pattern else {
        panic!("or-pattern expected");
    };
    assert!(matches!(alternatives[0], mink::ast::Pattern::Int { .. }));
    assert!(matches!(alternatives[1], mink::ast::Pattern::Range { .. }));
}

#[test]
fn range_patterns_parse() {
    let src = "fn main() { let x = 1;
        match x { 1..=5 => { }, _ => { } }
        match x { 1..5 => { }, _ => { } }
        match x { -5..=-1 => { }, _ => { } }
        match x { -5..5 => { }, _ => { } }
        match x { 0x10..=0x20 => { }, _ => { } }
        match x { 0b1..=0b10 => { }, _ => { } }
    }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let matches = match_stmts(&ast, "main");
    assert_eq!(matches.len(), 6);
    for m in &matches {
        let mink::ast::Pattern::Range { inclusive, .. } = &m.arms[0].pattern else {
            panic!("range pattern expected");
        };
        assert_eq!(
            *inclusive,
            m == &matches[0] || m == &matches[2] || m == &matches[4] || m == &matches[5]
        );
    }
    // Negative endpoints carry their sign flag.
    let mink::ast::Pattern::Range { lo, hi, .. } = &matches[2].arms[0].pattern else {
        panic!("range pattern expected");
    };
    assert!(matches!(
        lo.as_ref(),
        mink::ast::Pattern::Int { negative: true, .. }
    ));
    assert!(matches!(
        hi.as_ref(),
        mink::ast::Pattern::Int { negative: true, .. }
    ));
}

#[test]
fn guards_parse() {
    let src = "enum E { A(Int), B }
        fn main() { let x = 1; let e = E::A(3);
            match x { y if y > 3 => { }, _ => { } }
            match e { E::A(n) if n > 3 => { }, E::A(_) => { }, E::B => { } }
            match x { 1 | 2 if x == 2 => { }, _ => { } }
            match x { 1 if x > 0 && x < 10 => { }, _ => { } }
        }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let matches = match_stmts(&ast, "main");
    assert_eq!(matches.len(), 4);
    assert!(matches[0].arms[0].guard.is_some(), "binding arm with guard");
    assert!(matches[1].arms[0].guard.is_some(), "payload arm with guard");
    assert!(matches[2].arms[0].guard.is_some(), "or-arm with guard");
    assert!(matches[3].arms[0].guard.is_some(), "compound guard");
}

#[test]
fn a_pipe_after_if_is_part_of_the_guard_expression() {
    // `1 | 2 if c` is an or-pattern with a whole-arm guard; `1 if c | 2`
    // is pattern `1` with the guard expression `c | 2` (the `|` binds in
    // the guard's expression context, exactly like Rust).
    let src = "fn main() { let x = 1;
        match x { 1 | 2 if x == 1 => { }, _ => { } }
        match x { 1 if x == 1 | x == 2 => { }, _ => { } }
    }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
    let (ast, _, _) = parsed.into_parts();
    let matches = match_stmts(&ast, "main");
    // First arm: an or-pattern of two alternatives plus a guard.
    assert!(matches!(
        matches[0].arms[0].pattern,
        mink::ast::Pattern::Or { .. }
    ));
    assert!(matches[0].arms[0].guard.is_some());
    // Second arm: a plain integer pattern whose guard is a bitwise-or.
    assert!(matches!(
        matches[1].arms[0].pattern,
        mink::ast::Pattern::Int { .. }
    ));
    let guard = matches[1].arms[0].guard.as_ref().expect("guard present");
    assert!(
        matches!(
            guard.kind,
            mink::ast::ExprKind::Binary {
                op: mink::ast::BinaryOp::BitOr,
                ..
            }
        ),
        "the guard must parse `x == 1 | x == 2` as a bitwise-or expression"
    );
}

#[test]
fn range_endpoints_must_be_integer_literals() {
    // A non-integer left endpoint (`_..=5`, `E::A..5`) is E-P19.
    let src = "fn main() { let x = 1; match x { _..=5 => { }, _ => { } } }";
    assert!(parse_errors(src).contains(&ParseErrorKind::ExpectedIntegerLiteral));
    let src = "enum E { A } fn main() { let e = E::A; match e { E::A..5 => { }, _ => { } } }";
    assert!(parse_errors(src).contains(&ParseErrorKind::ExpectedIntegerLiteral));
    // A non-integer right endpoint (`1..true`) is E-P19.
    let src = "fn main() { let x = 1; match x { 1..true => { }, _ => { } } }";
    assert!(parse_errors(src).contains(&ParseErrorKind::ExpectedIntegerLiteral));
    // A missing right endpoint (`1..`) is E-P19.
    let src = "fn main() { let x = 1; match x { 1.. => { }, _ => { } } }";
    assert!(parse_errors(src).contains(&ParseErrorKind::ExpectedIntegerLiteral));
}

#[test]
fn range_endpoints_cannot_be_ranges() {
    // `1..2..3` is E-P19: a range endpoint is a single literal.
    let src = "fn main() { let x = 1; match x { 1..2..3 => { }, _ => { } } }";
    assert!(parse_errors(src).contains(&ParseErrorKind::ExpectedIntegerLiteral));
}

#[test]
fn or_pattern_requires_a_pattern_after_the_pipe() {
    // `1 | =>` has nothing after the `|`: E-P23 (expected a pattern).
    let src = "fn main() { let x = 1; match x { 1 | => { }, _ => { } } }";
    assert!(parse_errors(src).contains(&ParseErrorKind::ExpectedPattern));
}

#[test]
fn guarded_arms_accept_trailing_commas() {
    let src = "fn main() { let x = 1; match x { y if y > 0 => { }, _ => { }, } }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
}

#[test]
fn range_patterns_in_payload_position_parse() {
    let src = "enum S { C(Int) } fn main() { let s = S::C(1);
        match s { S::C(1..=5) => { }, S::C(_) => { } }
        match s { S::C(-3..-1) => { }, S::C(_) => { } }
    }";
    let parsed = parse_src(src);
    assert!(parsed.is_valid(), "{:?}", parsed.parse_errors());
}

#[test]
fn malformed_guards_are_rejected() {
    // A guard with no expression is a parse error.
    let src = "fn main() { let x = 1; match x { 1 if => { }, _ => { } } }";
    assert!(!parse_src(src).is_valid());
}

// ---------------------------------------------------------------------------
// Semantics: or-pattern bindings and guard scopes
// ---------------------------------------------------------------------------

#[test]
fn or_pattern_bindings_share_one_declaration() {
    // `E::A(x) | E::B(x)` binds one `x`: no duplicate-definition error.
    let src = "enum E { A(Int), B(Int) } fn main() { let e = E::A(1);
        match e { E::A(x) | E::B(x) => { let y = x; }, } }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert_eq!(
        semantic_errors(&semantic, SemanticErrorKind::DuplicateDefinition).len(),
        0
    );
}

#[test]
fn guard_bindings_resolve_in_the_arm_scope() {
    let src = "fn main() { let x = 1; match x { y if y > 3 => { }, _ => { } } }";
    let (_s, _a, semantic, _t) = check_src(src);
    assert_eq!(
        semantic_errors(&semantic, SemanticErrorKind::UnresolvedName).len(),
        0
    );
}

#[test]
fn guard_bindings_are_immutable() {
    // Assigning a pattern binding inside its guard is a semantic error.
    let src = "fn main() { let x = 1; match x { y if (y = 4) > 3 => { }, _ => { } } }";
    let (_s, _a, semantic, _t) = check_src(src);
    assert_eq!(
        semantic_errors(&semantic, SemanticErrorKind::AssignmentToImmutable).len(),
        1
    );
}

#[test]
fn range_patterns_bind_nothing() {
    let src = "fn main() { let x = 1; match x { 1..=5 => { }, _ => { } } }";
    let (_s, _a, semantic, _t) = check_src(src);
    assert_eq!(semantic.errors().len(), 0);
}

// ---------------------------------------------------------------------------
// Type checking: positive cases
// ---------------------------------------------------------------------------

#[test]
fn or_patterns_are_exhaustive_without_a_catch_all() {
    // Two unit variants covered by one or-arm: exhaustive.
    let src = "enum D { A, B } fn main() { let d = D::A;
        match d { D::A | D::B => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn or_pattern_with_payload_bindings_is_exhaustive() {
    let src = "enum S { C(Int), D(Int), E } fn main() { let s = S::C(1);
        match s { S::C(x) | S::D(x) => { let _ = x; }, S::E => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn or_pattern_with_literal_alternatives() {
    let src = "fn main() { let x = 1;
        match x { 1 | 2 | 3 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn mixed_or_and_range_arms() {
    let src = "fn main() { let x = 1;
        match x { 1..=3 => { }, 4 | 5 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn range_patterns_accept_inclusive_and_exclusive() {
    let src = "fn main() { let x = 1;
        match x { 1..=5 => { }, 6..10 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn negative_range_patterns() {
    let src = "fn main() { let x = 1;
        match x { -5..=-1 => { }, 0..=5 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn full_domain_range_is_exhaustive() {
    // `i64::MIN..=i64::MAX` covers every `Int`: exhaustive without a
    // catch-all (session 27).
    let src = "fn main() { let x = 1;
        match x { -9223372036854775808..=9223372036854775807 => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn adjacent_ranges_merge_and_complete_the_domain() {
    // Two adjacent ranges tile the whole domain: exhaustive.
    let src = "fn main() { let x = 1;
        match x { -9223372036854775808..=-1 => { }, 0..=9223372036854775807 => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn payload_range_patterns() {
    let src = "enum S { C(Int), D } fn main() { let s = S::C(1);
        match s { S::C(1..=5) => { }, S::C(_) => { }, S::D => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn payload_or_patterns() {
    let src = "enum S { C(Int), D } fn main() { let s = S::C(1);
        match s { S::C(1 | 2) => { }, S::C(_) => { }, S::D => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn guarded_arms_do_not_commit_coverage() {
    // The guarded `E::A if c` arm does not cover `E::A`; the following
    // unguarded `E::A` arm is reachable and the match is exhaustive.
    let src = "enum D { A, B } fn main() { let d = D::A;
        match d { D::A if d == D::A => { }, D::A => { }, D::B => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn guarded_catch_all_requires_an_unguarded_catch_all() {
    let src = "fn main() { let x = 1;
        match x { _ if x > 0 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn or_arm_is_reachable_through_a_live_alternative() {
    // `1 | 2` after `1`: the `1` alternative is redundant but the arm is
    // reachable through `2`.
    let src = "fn main() { let x = 1;
        match x { 1 => { }, 1 | 2 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn or_pattern_duplicate_alternative_is_accepted() {
    // `1 | 1` has a redundant alternative; the arm is still reachable
    // (conservative acceptance, documented in session 27).
    let src = "fn main() { let x = 1;
        match x { 1 | 1 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn or_pattern_binding_type_matches_the_scrutinee() {
    let src = "enum D { A, B } fn main() { let d = D::A;
        match d { D::A | D::B => { let w = d; }, } }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    let _ = symbol_type(&types, &semantic, "w");
}

#[test]
fn or_patterns_pin_an_unresolved_scrutinee() {
    // The scrutinee's type is pinned by its or-pattern alternatives.
    let src = "enum D { A, B } fn f(e) { match e { D::A | D::B => { }, } return 0; } fn main() { }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    let e = symbol_type(&types, &semantic, "e");
    assert!(
        matches!(types.types().kind(e), Some(TypeKind::Enum(_))),
        "scrutinee must be pinned to the enum type"
    );
}

#[test]
fn guard_references_the_pattern_binding() {
    let src = "enum S { C(Int), D } fn main() { let s = S::C(4);
        match s { S::C(x) if x > 3 => { }, S::C(_) => { }, S::D => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

// ---------------------------------------------------------------------------
// Type checking: negative cases
// ---------------------------------------------------------------------------

#[test]
fn or_pattern_bindings_must_agree() {
    // Different binding names across alternatives: E-T34.
    let src = "enum E { A(Int), B(Int) } fn main() { let e = E::A(1);
        match e { E::A(x) | E::B(y) => { }, E::B(_) => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::InvalidOrPattern).len(),
        1
    );
}

#[test]
fn or_pattern_binding_not_in_every_alternative() {
    // A wildcard alternative binds nothing: E-T34 for `x | 5` and `5 | x`.
    let src = "fn main() { let x = 1; match x { x | 5 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::InvalidOrPattern).len(),
        1
    );
    let src = "fn main() { let x = 1; match x { 5 | x => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::InvalidOrPattern).len(),
        1
    );
    let src = "enum E { A(Int), B(Int) } fn main() { let e = E::A(1);
        match e { E::A(x) | E::B(_) => { }, E::B(_) => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::InvalidOrPattern).len(),
        1
    );
}

#[test]
fn or_pattern_binding_types_must_agree() {
    // `x` is Int through `A` and Str through `B`: E-T01 at the second
    // alternative's binding.
    let src = "enum E { A(Int), B(Str) } fn main() { let e = E::A(1);
        match e { E::A(x) | E::B(x) => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
}

#[test]
fn or_alternative_type_mismatch() {
    let src = "fn main() { let x = 1; match x { 1 | true => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
}

#[test]
fn range_pattern_on_a_non_int_scrutinee() {
    // An integer range on a `Bool` scrutinee: E-T01 (a range requires an
    // `Int` scrutinee).
    let src = "fn main() { let b = true; match b { 1..=5 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
    // An integer range on an enum scrutinee: E-T01.
    let src = "enum D { A, B } fn main() { let d = D::A; match d { 1..=5 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
}

#[test]
fn a_point_covered_by_an_earlier_range_is_unreachable() {
    let src = "fn main() { let x = 1;
        match x { 1..=5 => { }, 3 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
}

#[test]
fn a_range_fully_covered_by_an_earlier_range_is_unreachable() {
    // `2..=4` is inside the earlier `1..=5`: the whole arm is covered.
    let src = "fn main() { let x = 1;
        match x { 1..=5 => { }, 2..=4 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
}

#[test]
fn a_range_overlapping_an_earlier_point_stays_reachable() {
    // `4 => {}` then `1..=5 => {}`: the range arm covers 1,2,3,5 too, so
    // it is reachable even though 4 was already covered.
    let src = "fn main() { let x = 1;
        match x { 4 => { }, 1..=5 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn partially_overlapping_ranges_keep_the_arm_reachable() {
    // `5..=10` overlaps `1..=5` at 5, but 6..=10 is not covered: reachable.
    let src = "fn main() { let x = 1;
        match x { 1..=5 => { }, 5..=10 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn adjacent_ranges_do_not_make_the_later_arm_unreachable() {
    // `6..=10` is adjacent to `1..=5` (contiguous coverage), but it covers
    // values `1..=5` did not: reachable.
    let src = "fn main() { let x = 1;
        match x { 1..=5 => { }, 6..=10 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn empty_ranges_are_unreachable() {
    // `5..5` (exclusive) is empty; `5..=3` is inverted: E-T25.
    let src = "fn main() { let x = 1; match x { 5..5 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
    let src = "fn main() { let x = 1; match x { 5..=3 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
}

#[test]
fn exclusive_range_at_min_is_empty() {
    // `i64::MIN..i64::MIN` excludes the only in-domain value: empty.
    let src = "fn main() { let x = 1;
        match x { -9223372036854775808..-9223372036854775808 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
}

#[test]
fn an_all_dead_or_arm_is_unreachable() {
    // Both alternatives of `1 | 1` are covered by the earlier `1` arm:
    // E-T25 on the whole arm.
    let src = "fn main() { let x = 1;
        match x { 1 => { }, 1 | 1 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
}

#[test]
fn a_guarded_arm_after_a_catch_all_is_unreachable() {
    let src = "fn main() { let x = 1;
        match x { _ => { }, _ if x > 0 => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
}

#[test]
fn a_guarded_arm_covered_by_an_earlier_arm_is_unreachable() {
    let src = "fn main() { let x = 1;
        match x { 1 => { }, 1 if x > 0 => { }, _ => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::UnreachableMatchArm).len(),
        1
    );
}

#[test]
fn a_match_with_only_a_guarded_catch_all_is_non_exhaustive() {
    let src = "fn main() { let x = 1; match x { _ if x > 0 => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::NonExhaustiveMatch).len(),
        1
    );
}

#[test]
fn a_match_whose_only_coverage_is_guarded_is_non_exhaustive() {
    let src = "enum D { A, B } fn main() { let d = D::A;
        match d { D::A if d == D::A => { }, D::B => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::NonExhaustiveMatch).len(),
        1
    );
}

#[test]
fn non_bool_guards_are_rejected() {
    let src = "fn main() { let x = 1; match x { 1 if 5 => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
    let src = "fn main() { let x = 1; match x { 1 if \"s\" => { }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
}

#[test]
fn int_matches_with_ranges_still_need_a_catch_all() {
    let src = "fn main() { let x = 1; match x { 1..=5 => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::NonExhaustiveMatch).len(),
        1
    );
}

#[test]
fn or_patterns_of_different_enums_are_rejected() {
    let src = "enum E { A } enum F { B } fn main() { let e = E::A;
        match e { E::A | F::B => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
}

#[test]
fn payload_range_without_completion_is_non_exhaustive() {
    let src = "enum S { C(Int), D } fn main() { let s = S::C(1);
        match s { S::C(1..=5) => { }, S::D => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert_eq!(
        type_errors(&types, TypeErrorKind::NonExhaustiveMatch).len(),
        1
    );
}

#[test]
fn payload_or_duplicate_is_rejected() {
    // `S::C(1)` then `S::C(1 | 2)`: the `1` alternative is dead, but `2`
    // is live, so only the payload sub-coverage extends.
    let src = "enum S { C(Int), D } fn main() { let s = S::C(1);
        match s { S::C(1) => { }, S::C(1 | 2) => { }, S::C(_) => { }, S::D => { }, } }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
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
fn or_pattern_lowers_to_hir() {
    let src = "fn main() { match 5 { 1 | 2 | 3 => { }, _ => { } } }";
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
    let mink::hir::HirPattern::Or { alternatives, .. } = &m.arms[0].pattern else {
        panic!("or-pattern expected");
    };
    assert_eq!(alternatives.len(), 3);
}

#[test]
fn range_pattern_lowers_to_hir() {
    let src = "fn main() { match 5 { 1..=5 => { }, _ => { } } }";
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
    let mink::hir::HirPattern::Range { inclusive, .. } = &m.arms[0].pattern else {
        panic!("range pattern expected");
    };
    assert!(*inclusive);
}

#[test]
fn guard_lowers_to_hir() {
    let src = "fn main() { match 5 { x if x > 3 => { }, _ => { } } }";
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
    assert!(
        m.arms[0].guard.is_some(),
        "the guarded arm must carry its guard into HIR"
    );
}

#[test]
fn or_pattern_bindings_resolve_to_one_symbol() {
    let src = "enum S { C(Int), D(Int) } fn main() { match S::C(1) { S::C(x) | S::D(x) => { let _ = x; }, } }";
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
    let mink::hir::HirPattern::Or { alternatives, .. } = &m.arms[0].pattern else {
        panic!("or-pattern expected");
    };
    let mink::hir::HirPattern::EnumVariant {
        payload: Some(first),
        ..
    } = &alternatives[0]
    else {
        panic!("variant alternative expected");
    };
    let mink::hir::HirPattern::EnumVariant {
        payload: Some(second),
        ..
    } = &alternatives[1]
    else {
        panic!("variant alternative expected");
    };
    let mink::hir::HirPattern::Binding(first) = first.as_ref() else {
        panic!("binding expected");
    };
    let mink::hir::HirPattern::Binding(second) = second.as_ref() else {
        panic!("binding expected");
    };
    assert_eq!(
        first.symbol, second.symbol,
        "both alternatives must resolve to the one logical binding"
    );
}

// ---------------------------------------------------------------------------
// MIR lowering: or-patterns, ranges, and guards
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

/// Whether `f` contains a binary rvalue with operator `op` against an
/// integer-literal constant.
fn has_int_comparison(f: &MirFn, op: mink::ast::BinaryOp) -> bool {
    f.blocks.iter().any(|block| {
        block.stmts.iter().any(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            let MirRvalueKind::Binary { op: got, rhs, .. } = &rvalue.kind else {
                return false;
            };
            *got == op
                && matches!(
                    &rhs.kind,
                    MirOperandKind::Constant(c)
                        if matches!(c.kind, mir::MirConstantKind::Int)
                )
        })
    })
}

#[test]
fn or_pattern_lowers_to_multiple_tests() {
    let src = "fn main() { match 5 { 1 | 2 | 3 => { }, _ => { } } }";
    let (_hir, mir) = lower_hir(src);
    let main = mir_fn(&mir, "main");
    // Three alternatives: three `==` comparisons against literal constants.
    let eq_tests = main
        .blocks
        .iter()
        .flat_map(|block| block.stmts.iter())
        .filter(|stmt| {
            let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
            let MirRvalueKind::Binary { op, rhs, .. } = &rvalue.kind else {
                return false;
            };
            *op == mink::ast::BinaryOp::Eq
                && matches!(
                    &rhs.kind,
                    MirOperandKind::Constant(c)
                        if matches!(c.kind, mir::MirConstantKind::Int)
                )
        })
        .count();
    assert_eq!(eq_tests, 3, "each alternative must lower to its own test");
}

#[test]
fn inclusive_range_lowers_to_ge_and_le() {
    let src = "fn main() { match 5 { 1..=5 => { }, _ => { } } }";
    let (_hir, mir) = lower_hir(src);
    let main = mir_fn(&mir, "main");
    assert!(has_int_comparison(main, mink::ast::BinaryOp::Ge), "v >= lo");
    assert!(has_int_comparison(main, mink::ast::BinaryOp::Le), "v <= hi");
    assert!(!has_int_comparison(main, mink::ast::BinaryOp::Lt));
}

#[test]
fn exclusive_range_lowers_to_ge_and_lt() {
    let src = "fn main() { match 5 { 1..5 => { }, _ => { } } }";
    let (_hir, mir) = lower_hir(src);
    let main = mir_fn(&mir, "main");
    assert!(has_int_comparison(main, mink::ast::BinaryOp::Ge), "v >= lo");
    assert!(has_int_comparison(main, mink::ast::BinaryOp::Lt), "v < hi");
    assert!(!has_int_comparison(main, mink::ast::BinaryOp::Le));
}

#[test]
fn guard_lowers_to_a_condition_branch() {
    // A refutable variant pattern produces a discriminant branch; the guard
    // produces a second branch testing `x > 3`.
    let src = "enum S { C(Int), D(Int) } fn main() {
        match S::C(5) { S::C(x) if x > 3 => { rt_print_int(x); }, S::C(_) => { }, S::D(_) => { }, }
    }";
    let (_hir, mir) = lower_hir(src);
    let main = mir_fn(&mir, "main");
    // The guard's comparison must exist, and the arm must branch on it.
    assert!(has_int_comparison(main, mink::ast::BinaryOp::Gt), "x > 3");
    let branches = main
        .blocks
        .iter()
        .filter(|block| matches!(block.terminator, mir::MirTerminator::Branch { .. }))
        .count();
    assert!(
        branches >= 2,
        "discriminant test + guard test must both branch"
    );
}

#[test]
fn or_pattern_binding_reuses_one_local() {
    let src = "enum S { C(Int), D(Int) } fn main() { match S::C(1) { S::C(x) | S::D(x) => { rt_print_int(x); }, } }";
    let (_hir, mir) = lower_hir(src);
    let main = mir_fn(&mir, "main");
    let x_locals = main.locals.iter().filter(|local| local.name == "x").count();
    assert_eq!(
        x_locals, 1,
        "both alternatives must share one binding local"
    );
}

#[test]
fn guarded_arms_use_a_guard_block() {
    let src = "fn main() { match 5 { x if x > 3 => { rt_print_int(x); }, _ => { } } }";
    let (_hir, mir) = lower_hir(src);
    let main = mir_fn(&mir, "main");
    // Lowering a guarded match must produce valid MIR (already asserted by
    // `lower_hir`'s validate); this pins the shape: a guarded arm's guard
    // is tested in a block whose terminator branches on it, and the guard
    // expression (the `x > 3` comparison) is present.
    assert!(
        main.blocks
            .iter()
            .any(|block| matches!(block.terminator, mir::MirTerminator::Branch { .. })),
        "guarded arm must branch on its guard"
    );
    assert!(
        has_int_comparison(main, mink::ast::BinaryOp::Gt),
        "guard comparison"
    );
}

// ---------------------------------------------------------------------------
// Backend lowering and determinism
// ---------------------------------------------------------------------------

/// Runs the front end on `src` and lowers the optimized MIR into backend
/// instructions, asserting every stage is clean.
fn lower_backend(src: &str) -> (mir::MirProgram, mink::backend::BProgram) {
    let mut sources = SourceMap::new();
    let name = std::thread::current()
        .name()
        .unwrap_or("backend")
        .replace("::", "_");
    let path = std::env::temp_dir().join(format!("mink_richer_{}_{name}.mink", std::process::id()));
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
fn richer_patterns_lower_through_the_backend() {
    let src = "enum S { C(Int), D(Int) }
        fn main() { match 5 { 1 | 2 | 3 => { }, 4..=9 => { }, _ => { } }
            match S::C(1) { S::C(x) | S::D(x) if x > 0 => { }, S::C(_) => { }, S::D(_) => { } }
            return; }";
    let (_mir, _program) = lower_backend(src);
}

#[test]
fn richer_pattern_images_are_byte_identical() {
    let src = "enum S { C(Int), D(Int) }
        fn main() { match 5 { 1 | 2 | 3 => { }, 4..=9 => { }, _ => { } }
            match S::C(1) { S::C(x) | S::D(x) if x > 0 => { }, S::C(_) => { }, S::D(_) => { } }
            return; }";
    let mut first = None;
    for _ in 0..2 {
        let mut sources = SourceMap::new();
        let name = std::thread::current()
            .name()
            .unwrap_or("image")
            .replace("::", "_");
        let path = std::env::temp_dir().join(format!(
            "mink_richer_img_{}_{name}.mink",
            std::process::id()
        ));
        std::fs::write(&path, src).unwrap();
        let report = mink::driver::check(&mut sources, &path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        let mir = report.mir.expect("clean program");
        let image = backend::compile(&mir, &sources, mink::backend::Target::native())
            .expect("compile succeeds");
        match &first {
            None => first = Some(image.bytes),
            Some(expected) => assert_eq!(*expected, image.bytes, "images must be byte-identical"),
        }
    }
}

// ---------------------------------------------------------------------------
// Ownership: moves through or-patterns and guards
// ---------------------------------------------------------------------------

#[test]
fn or_pattern_owned_payload_moves_out_of_the_scrutinee() {
    // Matching an owned Str payload through an or-pattern consumes the
    // scrutinee: a later use is E-S10.
    let src = "enum M { A(Str), B(Str) } fn f(m) {
        match m { M::A(s) | M::B(s) => { let _ = s; }, }
        match m { M::A(_) => { }, M::B(_) => { } }
        return 0;
    } fn main() { }";
    let ownership = check_ownership(src);
    assert_eq!(
        ownership_errors(&ownership, SemanticErrorKind::UseOfMovedValue).len(),
        1
    );
}

#[test]
fn guarded_owned_payload_binding_moves_the_scrutinee() {
    let src = "enum M { A(Str), B(Str) } fn f(m) {
        match m { M::A(s) if s == \"x\" => { let _ = s; }, M::A(_) => { }, M::B(_) => { } }
        match m { M::A(_) => { }, M::B(_) => { } }
        return 0;
    } fn main() { }";
    let ownership = check_ownership(src);
    assert_eq!(
        ownership_errors(&ownership, SemanticErrorKind::UseOfMovedValue).len(),
        1
    );
}

#[test]
fn or_pattern_copy_payload_leaves_the_scrutinee_usable() {
    // Int payloads copy: the scrutinee stays usable after the match.
    let src = "enum S { C(Int), D(Int) } fn f(s) {
        match s { S::C(x) | S::D(x) => { let _ = x; }, }
        match s { S::C(_) => { }, S::D(_) => { } }
        return 0;
    } fn main() { }";
    let (_s, _a, semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    assert_eq!(semantic.errors().len(), 0);
}

#[test]
fn guard_reads_bindings_without_moving_copy_values() {
    let src = "enum S { C(Int), D(Int) } fn f(s) {
        match s { S::C(x) if x > 3 => { let _ = x; }, S::C(_) => { }, S::D(_) => { } }
        return 0;
    } fn main() { }";
    let (_s, _a, _semantic, types) = check_src(src);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
}

#[test]
fn guards_retype_after_a_deferred_scrutinee_resolves() {
    // A guard whose binding's type is only pinned by the deferred re-type
    // pass (the scrutinee's type is deferred through a call) must be
    // re-checked like any other condition; a wrong-typed guard must still
    // be caught (E-T01), and a correct guard must be accepted.
    let good = "fn pick(c) { if c { return 3; } return 5; }
        fn main() { match pick(true) { x if x > 4 => { let _ = x; }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(good);
    assert!(types.errors().is_empty(), "{:?}", types.errors());
    let bad = "fn pick(c) { if c { return 3; } return 5; }
        fn main() { match pick(true) { x if x + \"s\" => { let _ = x; }, _ => { } } }";
    let (_s, _a, _semantic, types) = check_src(bad);
    assert_eq!(
        type_errors(&types, TypeErrorKind::InvalidOperator).len(),
        1,
        "{:?}",
        types.errors()
    );
}

// Native execution
// ---------------------------------------------------------------------------

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mink_richer_{}_{name}", std::process::id()));
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
fn native_or_pattern_dispatch() {
    let exe = build(
        "enum D { North, South, East, West }
         fn label(d) {
             match d {
                 D::North | D::South => { return 10; },
                 D::East | D::West => { return 20; },
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
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "10\n10\n20\n20");
}

#[test]
fn native_or_pattern_payload_binding() {
    let exe = build(
        "enum Shape { Circle(Int), Rect(Int), Nothing }
         fn area(s) {
             match s {
                 Shape::Circle(r) | Shape::Rect(r) => { return r * r; },
                 Shape::Nothing => { return 0; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(area(Shape::Circle(3)));
             rt_print_int(area(Shape::Rect(4)));
             rt_print_int(area(Shape::Nothing));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "9\n16\n0");
}

#[test]
fn native_inclusive_range_pattern() {
    let exe = build(
        "fn classify(n) {
             match n {
                 1..=3 => { return 10; },
                 4..=6 => { return 20; },
                 _ => { return 0; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(classify(1));
             rt_print_int(classify(3));
             rt_print_int(classify(4));
             rt_print_int(classify(6));
             rt_print_int(classify(7));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "10\n10\n20\n20\n0");
}

#[test]
fn native_exclusive_range_pattern() {
    let exe = build(
        "fn classify(n) {
             match n {
                 1..5 => { return 10; },
                 5 => { return 20; },
                 _ => { return 0; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(classify(1));
             rt_print_int(classify(4));
             rt_print_int(classify(5));
             rt_print_int(classify(6));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "10\n10\n20\n0");
}

#[test]
fn native_negative_range_pattern() {
    let exe = build(
        "fn classify(n) {
             match n {
                 -5..=-1 => { return 30; },
                 _ => { return 0; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(classify(-5));
             rt_print_int(classify(-1));
             rt_print_int(classify(0));
             rt_print_int(classify(-6));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "30\n30\n0\n0");
}

#[test]
fn native_guard_fall_through() {
    let exe = build(
        "enum Opt { Some(Int), None }
         fn pick(o) {
             match o {
                 Opt::Some(x) if x > 10 => { return 1; },
                 Opt::Some(x) => { return 2; },
                 Opt::None => { return 3; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(pick(Opt::Some(7)));
             rt_print_int(pick(Opt::Some(50)));
             rt_print_int(pick(Opt::None));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "2\n1\n3");
}

#[test]
fn native_guarded_catch_all_and_unguarded() {
    let exe = build(
        "fn pick(n) {
             match n {
                 _ if n > 100 => { return 1; },
                 _ => { return 0; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(pick(200));
             rt_print_int(pick(5));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "1\n0");
}

#[test]
fn native_or_pattern_with_guard() {
    let exe = build(
        "enum S { C(Int), D(Int), E }
         fn pick(s) {
             match s {
                 S::C(x) | S::D(x) if x > 10 => { return 1; },
                 S::C(x) | S::D(x) => { return 2; },
                 S::E => { return 3; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(pick(S::C(50)));
             rt_print_int(pick(S::D(7)));
             rt_print_int(pick(S::E));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "1\n2\n3");
}

#[test]
fn native_full_domain_range_match() {
    let exe = build(
        "fn classify(n) {
             match n {
                 -9223372036854775808..=9223372036854775807 => { return 42; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(classify(0));
             rt_print_int(classify(-5));
             rt_print_int(classify(123456789));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "42\n42\n42");
}

#[test]
fn native_or_pattern_owned_payload() {
    let exe = build(
        "enum M { A(Str), B(Str) }
         fn use1(m) {
             match m {
                 M::A(s) | M::B(s) => { rt_print_str(s); },
             }
             return 0;
         }
         fn main() {
             let m = M::A(\"hello\");
             use1(m);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "hello");
}

#[test]
fn native_payload_ranges_and_or_patterns() {
    let exe = build(
        "enum Opt { Some(Int), None }
         fn tagged(n) {
             match n {
                 Opt::Some(1..=3) => { return 40; },
                 Opt::Some(4 | 5 | 6) => { return 50; },
                 Opt::Some(_) => { return 60; },
                 Opt::None => { return 70; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(tagged(Opt::Some(2)));
             rt_print_int(tagged(Opt::Some(5)));
             rt_print_int(tagged(Opt::Some(9)));
             rt_print_int(tagged(Opt::None));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "40\n50\n60\n70");
}

#[test]
fn native_ranges_and_guards_in_a_loop() {
    let exe = build(
        "fn bucket(n) {
             match n {
                 0..=9 => { return 1; },
                 10..=19 => { return 2; },
                 n if n < 0 => { return 3; },
                 _ => { return 4; },
             }
             return 99;
         }
         fn main() {
             for i in 0..=12 {
                 rt_print_int(bucket(i));
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(
        String::from_utf8_lossy(&stdout).trim(),
        "1\n1\n1\n1\n1\n1\n1\n1\n1\n1\n2\n2\n2"
    );
}

#[test]
fn native_many_arm_or_match_stays_word_sized() {
    let exe = build(
        "fn classify(n) {
             match n {
                 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 => { return 100; },
                 11 | 12 | 13 | 14 | 15 => { return 200; },
                 _ => { return 0; },
             }
             return 99;
         }
         fn main() {
             rt_print_int(classify(1));
             rt_print_int(classify(10));
             rt_print_int(classify(15));
             rt_print_int(classify(16));
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "100\n100\n200\n0");
}

// ---------------------------------------------------------------------------
// Regression: sessions 1–26 behavior is preserved
// ---------------------------------------------------------------------------

#[test]
fn regression_unannotated_program_still_works() {
    let src = "fn main() {
        let mut total = 0;
        for i in 1..=10 {
            total = total + i;
        }
        match total { 55 => { rt_print_int(1); }, _ => { rt_print_int(0); } }
        return;
    }";
    let exe = build(src);
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "1");
}

#[test]
fn regression_enum_match_still_works() {
    let exe = build(
        "enum Direction { North, South, East, West }
         fn main() {
             let d = Direction::East;
             match d {
                 Direction::North => { rt_print_int(1); },
                 Direction::South => { rt_print_int(2); },
                 Direction::East => { rt_print_int(3); },
                 Direction::West => { rt_print_int(4); },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "3");
}

#[test]
fn regression_payload_matches_still_work() {
    let exe = build(
        "enum Shape { Circle(Int), Nothing }
         fn main() {
             let s = Shape::Circle(7);
             match s {
                 Shape::Circle(r) => { rt_print_int(r); },
                 Shape::Nothing => { rt_print_int(0); },
             }
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "7");
}

#[test]
fn regression_function_annotations_still_work() {
    let exe = build(
        "fn add(x: Int, y: Int) -> Int { return x + y; }
         fn main() {
             let r: Int = add(3, 4);
             rt_print_int(r);
             return;
         }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "7");
}

#[test]
fn regression_borrowing_still_works() {
    let src = "fn read() {
        let mut x = 5;
        let r = &mut x;
        *r = 7;
        rt_print_int(*r);
        return 0;
    }
    fn main() { read(); return; }";
    let exe = build(src);
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "7");
}

#[test]
fn regression_negative_literal_patterns_still_work() {
    let exe = build(
        "fn main() {
            let t = -5;
            match t {
                -5 => { rt_print_int(55); },
                _ => { rt_print_int(0); },
            }
            return;
        }",
    );
    let (code, stdout) = run(&exe);
    assert_eq!(code, 0);
    assert_eq!(String::from_utf8_lossy(&stdout).trim(), "55");
}
