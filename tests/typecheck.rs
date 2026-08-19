//! Integration tests for type analysis: literal typing, declaration and
//! identifier typing, operator typing, assignment, calls, ranges, control
//! flow, type diagnostics, error-cascade suppression, the type environment,
//! and malformed-input robustness.
//!
//! The rules under test are documented in `docs/language/CORE_LANGUAGE.md`
//! §26 and `docs/implementation/TYPE_SYSTEM_IMPLEMENTATION.md`.

use std::path::Path;

use mink::ast::{
    AssignOp, Ast, BinaryOp, Block, Expr, ExprKind, FnItem, Ident, IfStmt, Item, ItemKind, Param,
    Stmt, StmtKind, UnaryOp,
};
use mink::parser;
use mink::semantics::{SemanticErrorKind, SemanticResult, SymbolKind};
use mink::source::{SourceId, SourceMap, Span};
use mink::typecheck::{TypeError, TypeErrorKind, TypeId, TypeResult};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses, semantically analyzes, and type-checks `src`, asserting that it
/// lexes and parses cleanly (type tests start from valid syntax).
fn check_src(src: &str) -> (SourceMap, Ast, SemanticResult, TypeResult) {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).expect("the file just added");
    let parsed = parser::parse(file);
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
fn type_errors(types: &TypeResult, kind: TypeErrorKind) -> Vec<&TypeError> {
    types
        .errors()
        .iter()
        .filter(|error| error.kind() == kind)
        .collect()
}

/// The inferred type of the first symbol named `name`.
fn symbol_ty(types: &TypeResult, semantic: &SemanticResult, name: &str) -> TypeId {
    let symbol = semantic
        .symbols()
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named `{name}`"));
    types
        .symbol_type(symbol.id)
        .unwrap_or_else(|| panic!("no type recorded for `{name}`"))
}

/// The rendered name of a type id.
fn type_name(types: &TypeResult, id: TypeId) -> String {
    types.types().display(id)
}

/// Asserts that the expression at `needle` has the rendered type `expected`.
fn assert_expr_type(src: &str, types: &TypeResult, needle: &str, expected: &str) {
    let ty = types
        .expr_type(text_span(src, needle))
        .unwrap_or_else(|| panic!("no type recorded for `{needle}`"));
    assert_eq!(type_name(types, ty), expected, "type of `{needle}`");
}

/// The span of the `needle` text, assuming it appears exactly once. The
/// source file registered by [`check_src`] always has id `0`.
fn text_span(src: &str, needle: &str) -> Span {
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not found"));
    Span::new(
        SourceId::new(0),
        start as u32..start as u32 + needle.len() as u32,
    )
}

// ---------------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------------

#[test]
fn integer_literal_types_as_int() {
    let (_sources, _ast, _semantic, types) = check_src("let x = 1;");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn float_literal_types_as_float() {
    let (_sources, _ast, _semantic, types) = check_src("let x = 2.5;");
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "x")),
        "Float"
    );
}

#[test]
fn string_literal_types_as_str() {
    let (_sources, _ast, _semantic, types) = check_src("let x = \"hi\";");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Str");
}

#[test]
fn char_literal_types_as_char() {
    let (_sources, _ast, _semantic, types) = check_src("let x = 'a';");
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "x")),
        "Char"
    );
}

#[test]
fn bool_literals_type_as_bool() {
    let (_sources, _ast, _semantic, types) = check_src("let t = true; let f = false;");
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "t")),
        "Bool"
    );
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "f")),
        "Bool"
    );
}

#[test]
fn null_literal_types_as_null() {
    let (_sources, _ast, _semantic, types) = check_src("let x = null;");
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "x")),
        "Null"
    );
}

#[test]
fn literal_expression_types_are_recorded() {
    let src = "let x = 1; let y = 2.5; let s = \"a\"; let c = 'b'; let t = true; let n = null;";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_expr_type(src, &types, "1", "Int");
    assert_expr_type(src, &types, "2.5", "Float");
    assert_expr_type(src, &types, "\"a\"", "Str");
    assert_expr_type(src, &types, "'b'", "Char");
    assert_expr_type(src, &types, "true", "Bool");
    assert_expr_type(src, &types, "null", "Null");
}

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

#[test]
fn declaration_binding_has_initializer_type() {
    let (_sources, _ast, _semantic, types) = check_src("fn f() { let x = 10; }");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn reference_uses_declaration_type() {
    let src = "fn f() { let x = 10; let y = x; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "y")), "Int");
    assert!(!types.has_errors());
}

#[test]
fn nested_block_reference_keeps_declaration_type() {
    let src = "fn f() { let x = 1.5; if true { let y = x; y; } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "y")),
        "Float"
    );
    assert!(!types.has_errors());
}

#[test]
fn shadowed_binding_gets_its_own_type() {
    let src = "fn f() { let x = 1; if true { let x = \"s\"; x; } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    // The inner shadow is a `Str`; the outer binding stays `Int`.
    let decls = _ast
        .items()
        .iter()
        .flat_map(|item| match &item.kind {
            ItemKind::Fn(f) => stmt_idents(&f.body),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    let inner = decls[1];
    let outer = symbol_ty(&types, &_semantic, "x");
    assert_eq!(
        type_name(&types, outer),
        "Int",
        "outer shadow must keep its type"
    );
    let inner_symbol = _semantic
        .symbols()
        .iter()
        .find(|s| s.span == inner)
        .unwrap();
    let inner_ty = types.symbol_type(inner_symbol.id).unwrap();
    assert_eq!(type_name(&types, inner_ty), "Str");
}

/// Collects declaration identifier spans of `let`/`const` bindings in a
/// block (in source order).
fn stmt_idents(block: &Block) -> Vec<Span> {
    let mut out = Vec::new();
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Let(binding) => out.push(binding.name.span),
            StmtKind::Const(binding) => out.push(binding.name.span),
            StmtKind::If(stmt) => {
                out.extend(stmt_idents(&stmt.then_block));
                if let Some(mink::ast::ElseBranch::Block(block)) = &stmt.else_branch {
                    out.extend(stmt_idents(block));
                }
            }
            StmtKind::While { body, .. } => out.extend(stmt_idents(body)),
            StmtKind::For { body, .. } => out.extend(stmt_idents(body)),
            StmtKind::Loop(body) => out.extend(stmt_idents(body)),
            _ => {}
        }
    }
    out
}

#[test]
fn unresolved_identifier_gets_error_type() {
    let src = "fn f() { let x = missing; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    // The semantic stage reports the unresolved name; type analysis quietly
    // gives `x` the error type and reports no type error.
    assert!(semantic.has_errors());
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    assert!(!types.has_errors());
    let ty = symbol_ty(&types, &semantic, "x");
    assert_eq!(type_name(&types, ty), "unknown");
}

#[test]
fn module_binding_type_is_visible_in_function() {
    let src = "let base = 1; fn f() { let x = base; x + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

// ---------------------------------------------------------------------------
// Declarations
// ---------------------------------------------------------------------------

#[test]
fn let_infers_from_initializer() {
    let (_sources, _ast, _semantic, types) = check_src("let x = 1 + 2;");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn let_mut_infers_same_type_and_stays_mutable() {
    let src = "fn f() { let mut x = 1; x = 2; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &semantic, "x")), "Int");
    let x = semantic.symbols().iter().find(|s| s.name == "x").unwrap();
    assert_eq!(x.kind, SymbolKind::Let { mutable: true });
}

#[test]
fn const_infers_from_initializer() {
    let (_sources, _ast, _semantic, types) = check_src("const c = 2.5;");
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "c")),
        "Float"
    );
}

#[test]
fn declaration_types_propagate() {
    let src = "fn f() { let a = 1; let b = a; let c = b + 1; c; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["a", "b", "c"] {
        assert_eq!(
            type_name(&types, symbol_ty(&types, &_semantic, name)),
            "Int"
        );
    }
}

#[test]
fn module_scope_types_are_order_independent() {
    // `b`'s initializer references `a` before `a`'s own initializer is
    // analyzed; both end up `Int`.
    let src = "let b = a; const a = 1; fn f() { let x = b; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "b")), "Int");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn for_variable_infers_element_type() {
    let src = "fn f() { for i in 0..10 { i + 1; } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "i")), "Int");
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

#[test]
fn arithmetic_produces_operand_type() {
    let src = "fn f() { let x = 1 + 2 * 3 - 4 / 5 % 6; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
    assert_expr_type(src, &types, "1 + 2 * 3 - 4 / 5 % 6", "Int");
}

#[test]
fn float_arithmetic_produces_float() {
    let src = "fn f() { let x = 1.5 + 2.25; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "x")),
        "Float"
    );
}

#[test]
fn mixed_integer_float_arithmetic_is_rejected() {
    // MINK defines no implicit numeric conversions at this stage, so mixed
    // integer/float arithmetic is rejected rather than silently coerced.
    let src = "fn f() { let x = 1 + 2.5; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T02");
    assert_eq!(errors[0].operator(), Some("+"));
    assert_eq!(errors[0].actual(), Some("types `Int` and `Float`"));
    assert_eq!(errors[0].span(), text_span(src, "1 + 2.5"));
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "x")),
        "unknown"
    );
}

#[test]
fn arithmetic_on_bool_is_rejected() {
    let src = "fn f() { let x = true - 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("-"));
}

#[test]
fn logical_operations_require_bool_operands() {
    let src = "fn f() { let x = true && false || true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "x")),
        "Bool"
    );
}

#[test]
fn logical_on_int_is_rejected() {
    let src = "fn f() { let x = 1 && true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("&&"));
}

#[test]
fn comparison_produces_bool() {
    let src = "fn f() { let a = 1 < 2; let b = 2.5 >= 1.5; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "a")),
        "Bool"
    );
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "b")),
        "Bool"
    );
}

#[test]
fn comparison_with_bool_is_rejected() {
    let src = "fn f() { let x = 1 < true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("<"));
}

#[test]
fn string_comparison_is_rejected() {
    // String ordering is not defined at this stage.
    let src = "fn f() { let x = \"a\" < \"b\"; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::InvalidOperator).len(), 1);
}

#[test]
fn equality_produces_bool() {
    let src = "fn f() { let a = 1 == 1; let b = true != false; let c = \"x\" == \"x\"; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["a", "b", "c"] {
        assert_eq!(
            type_name(&types, symbol_ty(&types, &_semantic, name)),
            "Bool"
        );
    }
}

#[test]
fn mixed_type_equality_is_rejected() {
    let src = "fn f() { let x = 1 == \"a\"; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("=="));
}

#[test]
fn int_float_equality_is_rejected() {
    let src = "fn f() { let x = 1 == 1.0; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::InvalidOperator).len(), 1);
}

#[test]
fn null_equality_is_allowed() {
    let src = "fn f() { let x = null == null; let y = null != null; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "x")),
        "Bool"
    );
}

#[test]
fn bitwise_operations_require_int() {
    let src = "fn f() { let x = 1 & 2 | 3 ^ 4; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn bitwise_on_bool_is_rejected() {
    let src = "fn f() { let x = true & false; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("&"));
}

#[test]
fn shift_operations_require_int() {
    let src = "fn f() { let x = 1 << 2 >> 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn shift_with_bool_is_rejected() {
    let src = "fn f() { let x = 1 << true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("<<"));
}

#[test]
fn unary_negation_types_numerics() {
    let src = "fn f() { let a = -1; let b = -1.5; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "a")), "Int");
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "b")),
        "Float"
    );
}

#[test]
fn unary_logical_not_types_bool() {
    let src = "fn f() { let x = !true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "x")),
        "Bool"
    );
}

#[test]
fn unary_bitwise_not_types_int() {
    let src = "fn f() { let x = ~1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn unary_not_on_int_is_rejected() {
    let src = "fn f() { let x = !1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("!"));
    assert_eq!(errors[0].actual(), Some("type `Int`"));
}

#[test]
fn unary_negation_on_bool_is_rejected() {
    let src = "fn f() { let x = -true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("-"));
}

#[test]
fn bitwise_not_on_float_is_rejected() {
    let src = "fn f() { let x = ~1.5; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("~"));
}

// ---------------------------------------------------------------------------
// Assignment
// ---------------------------------------------------------------------------

#[test]
fn valid_mutable_assignment() {
    let src = "fn f() { let mut x = 1; x = 2; x = 3; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn incompatible_assignment_reports_mismatch() {
    let src = "fn f() { let mut x = 1; x = true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T01");
    assert_eq!(errors[0].expected(), Some("Int"));
    assert_eq!(errors[0].actual(), Some("Bool"));
    // The primary span is the offending value; the related span is the
    // assignment target (the single-char `x` at the start of `x = true`).
    assert_eq!(errors[0].span(), text_span(src, "true"));
    let assign = text_span(src, "x = true");
    assert_eq!(
        errors[0].related(),
        Some(Span::new(assign.file(), assign.start()..assign.start() + 1))
    );
}

#[test]
fn immutable_assignment_reports_only_semantic_error() {
    // `x` is immutable, so the semantic stage reports E-S03; the type
    // checker must not add a misleading type-mismatch on top.
    let src = "fn f() { let x = 1; x = true; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::AssignmentToImmutable).len(),
        1
    );
    assert!(!types.has_errors());
}

#[test]
fn const_assignment_reports_only_semantic_error() {
    let src = "const x = 1; fn f() { x = \"s\"; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::AssignmentToConstant).len(),
        1
    );
    assert!(!types.has_errors());
}

#[test]
fn compound_assignment_is_valid() {
    let src = "fn f() { let mut x = 1; x += 2; x -= 1; x *= 3; x /= 2; x %= 2; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn compound_assignment_with_bad_operand_is_rejected() {
    let src = "fn f() { let mut x = 1; x += true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("+="));
}

#[test]
fn assignment_chain_types_propagate() {
    let src = "fn f() { let mut a = 1; let mut b = 2; let mut g = 0; g = a = b = 3; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["a", "b", "g"] {
        assert_eq!(
            type_name(&types, symbol_ty(&types, &_semantic, name)),
            "Int"
        );
    }
}

#[test]
fn member_and_index_assignment_is_typed() {
    // Member/index assignment is fully typed: the assigned value must
    // unify with the field/element type (E-T01 on conflict).
    let src = "struct P { f: Int } fn f() { let mut o = P { f: 1 }; let mut arr = [1, 2]; o.f = 2; arr[0] = 3; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());

    let src = "struct P { f: Int } fn f() { let mut o = P { f: 1 }; o.f = \"s\"; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);

    let src = "fn f() { let mut arr = [1, 2]; arr[0] = \"s\"; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
}

// ---------------------------------------------------------------------------
// Enums (session 17)
// ---------------------------------------------------------------------------

#[test]
fn enum_variant_expression_has_the_enum_type() {
    let src = "enum E { A, B } fn main() { let x = E::A; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &semantic, "x")), "E");
}

#[test]
fn enum_variant_paths_are_nominally_distinct() {
    // Two enums are distinct types even when they declare the same
    // variants; comparing variants across enums is a type error (E-T02).
    let src = "enum E { A } enum F { A } fn main() { let b = E::A == F::A; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T02");
}

#[test]
fn unknown_variant_is_rejected() {
    let src = "enum E { A } fn main() { let x = E::B; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::UnknownVariant);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T23");
    assert_eq!(errors[0].span(), text_span(src, "B"));
}

#[test]
fn variant_access_on_non_enum_is_rejected() {
    let src = "struct S { x: Int } fn main() { let y = S::Q; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::NotAnEnum);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T22");
}

#[test]
fn enum_assignment_must_match() {
    let src = "enum E { A } enum F { A } fn main() { let mut e = E::A; e = F::A; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
}

#[test]
fn enum_equality_and_inequality_type_check() {
    let src = "enum E { A, B } fn main() { let b1 = E::A == E::A; let b2 = E::A != E::B; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "b1")),
        "Bool"
    );
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "b2")),
        "Bool"
    );
}

#[test]
fn enum_in_struct_field_is_typed() {
    let src = "enum C { R } struct T { c: C } fn main() { let t = T { c: C::R }; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "t")), "T");
}

// ---------------------------------------------------------------------------
// Calls
// ---------------------------------------------------------------------------

#[test]
fn valid_call_type_checks() {
    let src = "fn f(p) { p; } fn g() { f(1); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn call_with_wrong_arity_reports() {
    let src = "fn f(p) {} fn g() { f(1, 2); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::WrongArgumentCount);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T05");
    assert_eq!(errors[0].expected(), Some("1"));
    assert_eq!(errors[0].actual(), Some("2"));
}

#[test]
fn calling_literal_is_rejected() {
    let src = "fn f() { 1(2); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::NotCallable);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T04");
    assert_eq!(errors[0].actual(), Some("Int"));
    assert_eq!(errors[0].span(), text_span(src, "1"));
}

#[test]
fn calling_non_function_binding_is_rejected() {
    let src = "fn f() { let x = 1; x(2); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::NotCallable);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].actual(), Some("Int"));
}

#[test]
fn call_argument_conflict_reports_mismatch() {
    // The body constrains `p` to `Int` (p + 1); passing `true` conflicts.
    let src = "fn f(p) { p + 1; } fn g() { f(true); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Int"));
    assert_eq!(errors[0].actual(), Some("Bool"));
    assert_eq!(errors[0].span(), text_span(src, "true"));
}

#[test]
fn conflicting_arguments_at_one_call_site() {
    // The first call pins `p` to `Int`; the second call's `true` conflicts.
    let src = "fn f(p) {} fn g() { f(1); f(true); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].span(), text_span(src, "true"));
}

#[test]
fn call_result_type_propagates() {
    let src = "fn f() { return 1; } fn g() { let x = f(); x + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn recursive_call_is_allowed() {
    let src = "fn f() { f(); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn call_through_error_callee_poisons_result() {
    // An unresolved callee is an error type; the call result must be the
    // error type too (not a fresh variable), so downstream operations stay
    // silently unknown instead of being typed as concrete values.
    let src = "fn f() { let y = missing(1) + 2; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &semantic, "y")),
        "unknown"
    );
}

#[test]
fn comparison_result_is_bool_even_with_unknown_operands() {
    // `a == b` is `Bool` regardless of the (unconstrained) operand types,
    // so operating on the result as a number is a genuine error.
    let src = "fn f() { return; } fn g() { let a = f(); let b = f(); let r = a == b; r + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("+"));
    assert_expr_type(src, &types, "a == b", "Bool");
}

#[test]
fn logical_result_is_bool_even_with_unknown_operands() {
    let src = "fn f() { return; } fn g() { let a = f(); let b = f(); let r = a && b; if r { } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_expr_type(src, &types, "a && b", "Bool");
    // The recorded condition reference (`r` inside `if r`) is Bool too.
    let cond = src.find("if r").unwrap() + 3;
    let r_ref = Span::new(SourceId::new(0), cond as u32..cond as u32 + 1);
    assert_eq!(type_name(&types, types.expr_type(r_ref).unwrap()), "Bool");
}

#[test]
fn call_through_unknown_callee_is_deferred() {
    // The callee's type is not yet known (a function without a typed
    // return); calling it defers honestly instead of reporting an error.
    let src = "fn f() { return; } fn g() { let h = f(); h(1); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "h")),
        "unresolved"
    );
}

#[test]
fn member_index_call_chain_is_deferred() {
    let src = "fn foo(v) { v; } fn f() { let i = foo(1).member[0](x); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn calling_unresolved_name_reports_only_semantic_error() {
    let src = "fn f() { missing(1); }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    assert!(!types.has_errors());
}

// ---------------------------------------------------------------------------
// Control flow
// ---------------------------------------------------------------------------

#[test]
fn typed_returns_are_consistent() {
    let src = "fn f() { return 1; return 2; } fn g() { return \"s\"; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn conflicting_return_types_are_rejected() {
    let src = "fn f() { return 1; return true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Int"));
    assert_eq!(errors[0].actual(), Some("Bool"));
    assert_eq!(errors[0].span(), text_span(src, "true"));
}

#[test]
fn return_type_propagates_to_callers() {
    let src = "fn f() { return 1; } fn g() { let x = f(); return x; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn if_condition_must_be_bool() {
    let src = "fn f() { if 1 { } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Bool"));
    assert_eq!(errors[0].actual(), Some("Int"));
    assert_eq!(errors[0].span(), text_span(src, "1"));
}

#[test]
fn while_condition_must_be_bool() {
    let src = "fn f() { while 1 { } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Bool"));
}

#[test]
fn if_on_error_condition_is_deferred() {
    // Session 07 pins *unconstrained* conditions to `Bool`, but error-typed
    // conditions (from unresolved names) still defer silently: the root
    // semantic error is reported and no type noise is added on top.
    let src = "fn f() { if missing { } }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    assert!(!types.has_errors());
}

#[test]
fn for_over_non_range_is_rejected() {
    let src = "fn f() { for i in 1 { } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::NotIterable);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T06");
    assert_eq!(errors[0].actual(), Some("Int"));
}

// ---------------------------------------------------------------------------
// Ranges
// ---------------------------------------------------------------------------

#[test]
fn int_range_types_as_range_of_int() {
    let src = "fn f() { let r = 0 .. 10; for i in 0 ..= 5 { i; } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "r")),
        "Range<Int>"
    );
}

#[test]
fn float_range_types_as_range_of_float() {
    let src = "fn f() { let r = 0.5 .. 1.5; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "r")),
        "Range<Float>"
    );
}

#[test]
fn mixed_numeric_range_is_rejected() {
    let src = "fn f() { let r = 0 .. 1.5; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidRange);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T03");
    assert_eq!(errors[0].actual(), Some("`Int` and `Float`"));
    assert_eq!(errors[0].span(), text_span(src, "0 .. 1.5"));
}

#[test]
fn range_with_non_numeric_endpoint_is_rejected() {
    let src = "fn f() { let r = 0 .. \"a\"; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidRange);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].actual(), Some("`Int` and `Str`"));
}

// ---------------------------------------------------------------------------
// Error cascades
// ---------------------------------------------------------------------------

#[test]
fn unknown_symbol_produces_only_semantic_error() {
    let src = "fn f() { missing; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    assert!(!types.has_errors());
}

#[test]
fn unknown_symbol_in_expression_does_not_cascade() {
    let src = "fn f() { let x = missing; x + 1; x = true; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    // No operator, assignment, or cascade errors on top of the root.
    assert!(!types.has_errors());
}

#[test]
fn multiple_independent_type_errors_are_all_reported() {
    let src = "fn f() { let mut a = 1; a = true; let mut b = 2; b = \"s\"; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 2);
}

#[test]
fn semantic_and_type_errors_are_both_reported() {
    let src = "fn f() { missing; let mut x = 1; x = true; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    assert_eq!(type_errors(&types, TypeErrorKind::TypeMismatch).len(), 1);
}

#[test]
fn error_type_propagates_through_declarations_silently() {
    let src = "fn f() { let x = missing; let y = x + 1; let z = y; z; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    assert!(!types.has_errors());
    for name in ["x", "y", "z"] {
        assert_eq!(
            type_name(&types, symbol_ty(&types, &semantic, name)),
            "unknown"
        );
    }
}

#[test]
fn operator_error_does_not_cascade_into_declaration_errors() {
    let src = "fn f() { let x = 1 + \"s\"; x; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::InvalidOperator).len(), 1);
    // The poisoned declaration and its uses add no further errors.
    assert_eq!(types.errors().len(), 1);
}

// ---------------------------------------------------------------------------
// Type environment
// ---------------------------------------------------------------------------

#[test]
fn symbol_type_map_is_populated() {
    let src = "let a = 1; let b = 1.5; fn f(p) { let c = p; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(type_name(&types, symbol_ty(&types, &semantic, "a")), "Int");
    assert_eq!(
        type_name(&types, symbol_ty(&types, &semantic, "b")),
        "Float"
    );
    let f = semantic.symbols().iter().find(|s| s.name == "f").unwrap();
    let f_ty = types.symbol_type(f.id).unwrap();
    assert_eq!(type_name(&types, f_ty), "fn(unresolved) -> unresolved");
}

#[test]
fn equal_concrete_types_share_identity() {
    let src = "fn f() { let a = 1; let b = 2; let c = 1.5; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let a = symbol_ty(&types, &_semantic, "a");
    let b = symbol_ty(&types, &_semantic, "b");
    let c = symbol_ty(&types, &_semantic, "c");
    assert_eq!(types.types().canonical(a), types.types().canonical(b));
    assert_ne!(types.types().canonical(a), types.types().canonical(c));
}

#[test]
fn equal_range_types_share_identity() {
    let src = "fn f() { let a = 0 .. 10; let b = 1 .. 2; let c = 0.0 .. 1.0; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let a = symbol_ty(&types, &_semantic, "a");
    let b = symbol_ty(&types, &_semantic, "b");
    let c = symbol_ty(&types, &_semantic, "c");
    assert_eq!(types.types().canonical(a), types.types().canonical(b));
    assert_ne!(types.types().canonical(a), types.types().canonical(c));
}

#[test]
fn inference_variable_resolves_after_constraint() {
    // `p` is unconstrained until the body pins it; afterwards the
    // function type reflects the resolved parameter.
    let src = "fn f(p) { p + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let f = _semantic.symbols().iter().find(|s| s.name == "f").unwrap();
    let f_ty = types.symbol_type(f.id).unwrap();
    assert_eq!(type_name(&types, f_ty), "fn(Int) -> unresolved");
}

#[test]
fn expression_types_are_lookupable_by_span() {
    let src = "fn f() { let x = 1 + 2 * 3; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_expr_type(src, &types, "1 + 2 * 3", "Int");
    assert_expr_type(src, &types, "2 * 3", "Int");
    // Lookup by an exact span returns the recorded type.
    let ty = types.expr_type(text_span(src, "2 * 3")).unwrap();
    assert_eq!(type_name(&types, ty), "Int");
    // A span with no recorded expression returns None.
    assert!(types.expr_type(text_span(src, "fn f")).is_none());
}

#[test]
fn expression_type_of_condition_and_iterable() {
    let src = "fn f() { if 1 < 2 { } for i in 0 .. 5 { } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_expr_type(src, &types, "1 < 2", "Bool");
    assert_expr_type(src, &types, "0 .. 5", "Range<Int>");
}

// ---------------------------------------------------------------------------
// Robustness: unusual shapes and scale must not panic
// ---------------------------------------------------------------------------

#[test]
fn deeply_nested_control_flow_does_not_panic() {
    let mut src = String::from("fn f() {");
    for _ in 0..100 {
        src.push_str("if true {");
    }
    src.push_str("loop { break; }");
    for _ in 0..100 {
        src.push('}');
    }
    src.push('}');
    let (_sources, _ast, _semantic, types) = check_src(&src);
    assert!(!types.has_errors());
}

#[test]
fn many_functions_and_declarations_scale() {
    let mut src = String::from("const base = 1;");
    for i in 0..200 {
        src.push_str(&format!("fn f{i}(p) {{ let v = p + base; return v; }}"));
    }
    let (_sources, _ast, _semantic, types) = check_src(&src);
    assert!(!types.has_errors());
    // Every symbol has a recorded type.
    assert!(
        types
            .symbol_type(_semantic.symbols().iter().next().unwrap().id)
            .is_some()
    );
}

#[test]
fn long_expression_chain_does_not_panic() {
    let mut expr = String::from("1");
    for _ in 0..300 {
        expr.push_str(" + 1");
    }
    let src = format!("fn f() {{ let s = {expr}; s; }}");
    let (_sources, _ast, _semantic, types) = check_src(&src);
    assert!(!types.has_errors());
}

#[test]
fn chained_operator_errors_are_bounded() {
    // The first invalid operation poisons its result; the later operators
    // see an unknown operand and stay quiet, so a chain reports one root
    // error instead of exploding combinatorially.
    let src = "fn f() { let s = true + true + true + true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert_eq!(type_errors(&types, TypeErrorKind::InvalidOperator).len(), 1);
    assert_eq!(types.errors().len(), 1);
}

#[test]
fn unknown_names_throughout_stay_quiet() {
    // Unresolved names inside operators, calls, members, and assignments
    // produce exactly their semantic errors and no type noise.
    let src = "fn f() { let x = a + b; x; let mut y = c(1).d[0]; y = e; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        4
    );
    assert!(!types.has_errors());
}

/// Manually constructed unusual AST shapes (rejected by the parser, but
/// possibly present in tooling/AI-generated trees) must not panic the type
/// checker: literal and group assignment targets, literal callees, and
/// missing declarations.
#[test]
fn hand_built_unusual_asts_do_not_panic() {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("weird.mink"), "");
    let file_id = sources.get(id).unwrap().id();
    let span = Span::new(file_id, 0..0);
    let int = || Expr {
        kind: ExprKind::Int,
        span,
    };
    let lit_target_assign = Expr {
        kind: ExprKind::Assign {
            op: AssignOp::Assign,
            target: Box::new(int()),
            value: Box::new(int()),
        },
        span,
    };
    let group_target_assign = Expr {
        kind: ExprKind::Assign {
            op: AssignOp::AddAssign,
            target: Box::new(Expr {
                kind: ExprKind::Group(Box::new(int())),
                span,
            }),
            value: Box::new(int()),
        },
        span,
    };
    let literal_call = Expr {
        kind: ExprKind::Call {
            callee: Box::new(int()),
            args: vec![int()],
        },
        span,
    };
    let stmts = vec![
        Stmt {
            kind: StmtKind::Expr(lit_target_assign),
            span,
        },
        Stmt {
            kind: StmtKind::Expr(group_target_assign),
            span,
        },
        Stmt {
            kind: StmtKind::Expr(literal_call),
            span,
        },
    ];
    let ast = Ast::new(vec![Item {
        kind: ItemKind::Fn(FnItem {
            name: Ident {
                name: "f".to_string(),
                span,
            },
            params: vec![Param {
                name: Ident {
                    name: "p".to_string(),
                    span,
                },
                ty: None,
                span,
            }],
            return_ty: None,
            body: Block {
                stmts,
                result: None,
                span,
            },
        }),
        span,
    }]);
    let semantic = mink::semantics::analyze(&ast);
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    // The literal/group targets are treated as plain expressions; the
    // literal callee is a genuine not-callable error — but nothing panics.
    assert_eq!(type_errors(&types, TypeErrorKind::NotCallable).len(), 1);
}

#[test]
fn empty_program_type_checks() {
    let (_sources, _ast, _semantic, types) = check_src("");
    assert!(!types.has_errors());
    assert!(types.expr_types().is_empty());
    // The arena still exists (it holds the placeholder and interned types).
    assert!(!types.types().is_empty());
}

// ---------------------------------------------------------------------------
// Inference (session 07)
// ---------------------------------------------------------------------------

#[test]
fn chained_declarations_resolve_through_use() {
    // Module-scope declarations are order-independent and their inference
    // variables link transitively: the head of the chain resolves once the
    // chain meets a concrete type.
    let src = "let a = b; let b = c; const c = 1; fn f() { a + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["a", "b", "c"] {
        assert_eq!(
            type_name(&types, symbol_ty(&types, &_semantic, name)),
            "Int"
        );
    }
}

#[test]
fn mutually_constrained_declarations_resolve() {
    // `x` and `y` constrain each other; the constraint `x + 1` pins the
    // shared variable and both resolve.
    let src = "let x = y; let y = x + 1; fn f() { x; y; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["x", "y"] {
        assert_eq!(
            type_name(&types, symbol_ty(&types, &_semantic, name)),
            "Int"
        );
    }
}

#[test]
fn deep_inference_chain_resolves_deterministically() {
    // A 200-link declaration chain ending in a concrete type: the head
    // reference must resolve through the whole chain (exercises path
    // compression) without leaking unresolved variables.
    let mut src = String::new();
    for i in 0..200 {
        src.push_str(&format!("let v{i} = v{}; ", i + 1));
    }
    src.push_str("const v200 = 1; fn f() { v0 + 1; }");
    let (_sources, _ast, _semantic, types) = check_src(&src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "v0")),
        "Int"
    );
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "v200")),
        "Int"
    );
}

#[test]
fn parameter_type_inferred_from_argument_and_body() {
    // The parameter is constrained by the body (`return p`) and by the
    // call-site argument (`f(1)`); both share one inference variable.
    let src = "fn f(p) { return p; } fn g() { let x = f(1); x + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    let f = _semantic.symbols().iter().find(|s| s.name == "f").unwrap();
    let f_ty = types.symbol_type(f.id).unwrap();
    assert_eq!(type_name(&types, f_ty), "fn(Int) -> Int");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn return_inference_from_single_path() {
    let src = "fn f() { return 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    let f = _semantic.symbols().iter().find(|s| s.name == "f").unwrap();
    let f_ty = types.symbol_type(f.id).unwrap();
    assert_eq!(type_name(&types, f_ty), "fn() -> Int");
}

#[test]
fn return_inference_across_branches() {
    // Multiple return paths unify with the same result variable; the
    // parameter is pinned by the condition.
    let src = "fn f(c) { if c { return 1; } return 2; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    let f = _semantic.symbols().iter().find(|s| s.name == "f").unwrap();
    let f_ty = types.symbol_type(f.id).unwrap();
    assert_eq!(type_name(&types, f_ty), "fn(Bool) -> Int");
}

#[test]
fn recursive_function_infers_parameters_and_result() {
    // `n > 0` pins the parameter to `Int`; `return 0` pins the result to
    // `Int`; the recursive call unifies the result with itself.
    let src = "fn f(n) { if n > 0 { return f(n - 1); } return 0; } fn g() { let x = f(3); x; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    let f = _semantic.symbols().iter().find(|s| s.name == "f").unwrap();
    let f_ty = types.symbol_type(f.id).unwrap();
    assert_eq!(type_name(&types, f_ty), "fn(Int) -> Int");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn mutually_recursive_functions_type_check() {
    let src = "fn even(n) { if n == 0 { return true; } return odd(n - 1); }\n\
               fn odd(n) { if n == 0 { return false; } return even(n - 1); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    let even = _semantic
        .symbols()
        .iter()
        .find(|s| s.name == "even")
        .unwrap();
    let odd = _semantic
        .symbols()
        .iter()
        .find(|s| s.name == "odd")
        .unwrap();
    assert_eq!(
        type_name(&types, types.symbol_type(even.id).unwrap()),
        "fn(Int) -> Bool"
    );
    assert_eq!(
        type_name(&types, types.symbol_type(odd.id).unwrap()),
        "fn(Int) -> Bool"
    );
}

#[test]
fn mutually_constrained_calls_resolve() {
    // `f` returns `g(p)` and `g` returns `q`: the two functions share
    // parameter and result variables, so one call-site argument (`f(1)`)
    // resolves both signatures.
    let src = "fn f(p) { return g(p); } fn g(q) { return q; } fn h() { let x = f(1); x; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["f", "g"] {
        let symbol = _semantic.symbols().iter().find(|s| s.name == name).unwrap();
        assert_eq!(
            type_name(&types, types.symbol_type(symbol.id).unwrap()),
            "fn(Int) -> Int"
        );
    }
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn conflicting_returns_across_branches_are_rejected() {
    let src = "fn f(c) { if c { return 1; } return 1.5; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T01");
    assert_eq!(errors[0].expected(), Some("Int"));
    assert_eq!(errors[0].actual(), Some("Float"));
    assert_eq!(errors[0].span(), text_span(src, "1.5"));
}

#[test]
fn unconstrained_condition_is_pinned_to_bool() {
    // The expected type `Bool` flows into the condition, so the otherwise
    // unconstrained function result is determined rather than leaked.
    let src = "fn f() { return; } fn g() { if f() { } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    let f = _semantic.symbols().iter().find(|s| s.name == "f").unwrap();
    let f_ty = types.symbol_type(f.id).unwrap();
    assert_eq!(type_name(&types, f_ty), "fn() -> Bool");
}

#[test]
fn while_condition_pins_unconstrained_expression_to_bool() {
    let src = "fn f() { return; } fn g() { while f() { break; } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    let f = _semantic.symbols().iter().find(|s| s.name == "f").unwrap();
    let f_ty = types.symbol_type(f.id).unwrap();
    assert_eq!(type_name(&types, f_ty), "fn() -> Bool");
}

#[test]
fn result_driven_arithmetic_pins_callee_result() {
    // Using an unconstrained call result in arithmetic pins the callee's
    // result type through the concrete-constraint path.
    let src = "fn f() { return; } fn g() { let x = f() + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    let f = _semantic.symbols().iter().find(|s| s.name == "f").unwrap();
    let f_ty = types.symbol_type(f.id).unwrap();
    assert_eq!(type_name(&types, f_ty), "fn() -> Int");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn pinned_condition_conflicts_with_numeric_use() {
    // The condition pins the function result to `Bool`; using it as a
    // number later is a genuine operator error.
    let src = "fn f() { return; } fn g() { if f() { } f() + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].operator(), Some("+"));
    assert_eq!(errors[0].actual(), Some("types `Bool` and `Int`"));
}

#[test]
fn logical_operands_are_pinned_to_bool() {
    let src = "fn f() { return; } fn g() { let a = f(); let b = f(); let r = a && b; a; b; r; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["a", "b", "r"] {
        assert_eq!(
            type_name(&types, symbol_ty(&types, &_semantic, name)),
            "Bool"
        );
    }
}

#[test]
fn shift_operands_are_pinned_to_int() {
    let src = "fn f() { return; } fn g() { let a = f(); let b = f(); let r = a << b; a; b; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["a", "b", "r"] {
        assert_eq!(
            type_name(&types, symbol_ty(&types, &_semantic, name)),
            "Int"
        );
    }
}

#[test]
fn unary_not_pins_operand_to_bool() {
    let src = "fn f() { return; } fn g() { let a = f(); let b = !a; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "a")),
        "Bool"
    );
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "b")),
        "Bool"
    );
}

#[test]
fn unary_bitwise_not_pins_operand_to_int() {
    let src = "fn f() { return; } fn g() { let a = f(); let b = ~a; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "a")), "Int");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "b")), "Int");
}

#[test]
fn unary_negation_defers_on_ambiguous_operand() {
    // `-` accepts both numerics, so an unconstrained operand cannot be
    // pinned: it stays unresolved rather than being guessed.
    let src = "fn f() { return; } fn g() { let a = f(); let b = -a; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "a")),
        "unresolved"
    );
}

#[test]
fn unknown_iterable_is_pinned_to_range() {
    let src = "fn f() { return; } fn g() { let r = f(); for i in r { i; } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    // The structure of `r` is determined (a range); only its element type
    // is genuinely unknown at this point.
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "r")),
        "Range<unresolved>"
    );
}

#[test]
fn pinned_range_propagates_to_loop_variable() {
    // `for` pins the iterable to `Range<T>`; using the loop variable in
    // arithmetic resolves `T` and flows back into the iterable's type.
    let src = "fn f() { return; } fn g() { let r = f(); for i in r { i + 1; } }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "r")),
        "Range<Int>"
    );
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "i")), "Int");
}

#[test]
fn no_determinable_type_leaks_unresolved() {
    // Every type the language can determine must be determined: conditions,
    // logical operands, shift operands, and unary operands pin their
    // variables, so none of these symbols stays unresolved.
    let src = "fn a() { return; } fn b() { return; } fn c() { return; }\n\
               fn g() { if a() { } let l = a() && true; let s = b() << 1; let n = !c(); l; s; n; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["a", "b", "c", "l", "s", "n"] {
        let ty = symbol_ty(&types, &_semantic, name);
        assert!(
            types.types().is_resolved(ty),
            "`{name}` must be fully resolved, was {}",
            type_name(&types, ty)
        );
    }
}

#[test]
fn genuinely_ambiguous_types_stay_unresolved() {
    // Arithmetic on two unconstrained operands cannot be pinned (Int and
    // Float are both valid); staying unresolved is the honest outcome.
    let src = "fn f() { return; } fn g() { let a = f(); let b = f(); let r = a + b; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    for name in ["a", "b", "r"] {
        assert_eq!(
            type_name(&types, symbol_ty(&types, &_semantic, name)),
            "unresolved"
        );
    }
}

#[test]
fn incompatible_constraints_through_calls_are_rejected() {
    // The body pins `p` to `Int`; the second call-site argument conflicts.
    let src = "fn f(p) { p + 1; } fn g() { f(1); f(1.5); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Int"));
    assert_eq!(errors[0].actual(), Some("Float"));
    assert_eq!(errors[0].span(), text_span(src, "1.5"));
}

#[test]
fn incompatible_constraints_via_condition_and_call_are_rejected() {
    // The condition pins `p` to `Bool`; the later call argument conflicts.
    let src = "fn f(p) { p; } fn g() { if f(true) { } f(1); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Bool"));
    assert_eq!(errors[0].actual(), Some("Int"));
    assert_eq!(errors[0].span(), text_span(src, "1"));
}

#[test]
fn error_type_blocks_further_constraints_silently() {
    // An unresolved initializer poisons the declaration; conditions,
    // operators, and logical uses of the poisoned value stay quiet.
    let src = "fn f() { let x = missing; if x { } x + 1; x && true; }";
    let (_sources, _ast, semantic, types) = check_src(src);
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    assert!(!types.has_errors());
}

#[test]
fn independent_inference_conflicts_are_all_reported() {
    let src = "fn f(p) { p; } fn g(q) { q; } fn h() { f(1); f(1.5); g(true); g(1); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 2);
}

/// Hand-built ASTs exercising the session-07 pin paths with unresolved
/// identifiers (parser-rejected shapes that tooling may still produce) must
/// not panic: conditions, iterables, and unary/binary operands that are
/// error types stay quiet.
#[test]
fn hand_built_inference_shapes_do_not_panic() {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("weird_infer.mink"), "");
    let file_id = sources.get(id).unwrap().id();
    let mut pos = 0u32;
    let mut next_span = move || {
        let span = Span::new(file_id, pos..pos + 1);
        pos += 1;
        span
    };
    let ident = |name: &str, span: Span| Expr {
        kind: ExprKind::Ident(Ident {
            name: name.to_string(),
            span,
        }),
        span,
    };
    let u = ident("u", next_span());
    let v = ident("v", next_span());
    let w = ident("w", next_span());
    let x = ident("x", next_span());
    let y = ident("y", next_span());
    let empty = Block {
        result: None,
        stmts: Vec::new(),
        span: next_span(),
    };
    let stmts = vec![
        Stmt {
            kind: StmtKind::If(IfStmt {
                cond: u,
                then_block: empty.clone(),
                else_branch: None,
                span: next_span(),
            }),
            span: next_span(),
        },
        Stmt {
            kind: StmtKind::For {
                name: Ident {
                    name: "i".to_string(),
                    span: next_span(),
                },
                iterable: v,
                body: empty.clone(),
            },
            span: next_span(),
        },
        Stmt {
            kind: StmtKind::Expr(Expr {
                kind: ExprKind::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(w),
                },
                span: next_span(),
            }),
            span: next_span(),
        },
        Stmt {
            kind: StmtKind::Expr(Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::And,
                    lhs: Box::new(x),
                    rhs: Box::new(y),
                },
                span: next_span(),
            }),
            span: next_span(),
        },
    ];
    let ast = Ast::new(vec![Item {
        kind: ItemKind::Fn(FnItem {
            name: Ident {
                name: "f".to_string(),
                span: next_span(),
            },
            params: Vec::new(),
            return_ty: None,
            body: Block {
                stmts,
                result: None,
                span: next_span(),
            },
        }),
        span: next_span(),
    }]);
    let semantic = mink::semantics::analyze(&ast);
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    // The five unresolved identifiers are semantic errors; type analysis
    // pins nothing (error types absorb) and reports nothing extra.
    assert_eq!(
        error_spans(&semantic, SemanticErrorKind::UnresolvedName).len(),
        5
    );
    assert!(!types.has_errors());
}

/// Spans of all semantic errors of `kind`.
fn error_spans(result: &SemanticResult, kind: SemanticErrorKind) -> Vec<Span> {
    result
        .errors()
        .iter()
        .filter(|error| error.kind() == kind)
        .map(mink::semantics::SemanticError::span)
        .collect()
}

// ---------------------------------------------------------------------------
// Strings and pointers (session 13)
// ---------------------------------------------------------------------------

#[test]
fn string_literal_expr_type_is_str_in_fn_body() {
    let src = "fn main() { let s = \"hi\"; s; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "s")), "Str");
    assert_expr_type(src, &types, "\"hi\"", "Str");
}

#[test]
fn str_intrinsics_type_check() {
    let src = "fn main() { let s = rt_str_alloc(4); rt_str_len(s); rt_str_byte(s, 0); rt_str_set_byte(s, 0, 65); rt_print_str(s); rt_str_free(s); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "s")), "Str");
    assert_expr_type(src, &types, "rt_str_alloc(4)", "Str");
    assert_expr_type(src, &types, "rt_str_len(s)", "Int");
    assert_expr_type(src, &types, "rt_str_byte(s, 0)", "Int");
}

#[test]
fn rt_alloc_returns_ptr_int() {
    let src = "fn main() { let p = rt_alloc(16); p; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "p")),
        "Ptr<Int>"
    );
    assert_expr_type(src, &types, "rt_alloc(16)", "Ptr<Int>");
}

#[test]
fn pointer_arithmetic_keeps_pointer_type() {
    let src = "fn main() { let p = rt_alloc(16); let q = p + 1; let r = q - 2; r; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "p")),
        "Ptr<Int>"
    );
    assert_expr_type(src, &types, "p + 1", "Ptr<Int>");
    assert_expr_type(src, &types, "q - 2", "Ptr<Int>");
}

#[test]
fn pointer_minus_pointer_is_rejected() {
    // Only byte-addressed arithmetic is defined: `p - p` has no meaning
    // until subtraction of two pointers is specified.
    let src = "fn main() { let p = rt_alloc(16); let q = p + 8; q - p; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T02");
}

#[test]
fn pointer_plus_pointer_is_rejected() {
    let src = "fn main() { let p = rt_alloc(16); let q = rt_alloc(16); p + q; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T02");
}

#[test]
fn pointer_arithmetic_with_bool_is_rejected() {
    let src = "fn main() { let p = rt_alloc(16); p + true; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T02");
}

#[test]
fn pointer_multiply_is_rejected() {
    // Pointer arithmetic is only `+`/`-`; every other operator is invalid.
    for src in [
        "fn main() { let p = rt_alloc(16); p * 2; }",
        "fn main() { let p = rt_alloc(16); 2 * p; }",
        "fn main() { let p = rt_alloc(16); p / 2; }",
        "fn main() { let p = rt_alloc(16); p % 2; }",
        "fn main() { let p = rt_alloc(16); p << 1; }",
        "fn main() { let p = rt_alloc(16); p & 1; }",
    ] {
        let (_sources, _ast, _semantic, types) = check_src(src);
        let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
        assert_eq!(errors.len(), 1, "expected one E-T02 for `{src}`");
        assert_eq!(errors[0].code(), "E-T02");
    }
}

#[test]
fn int_minus_pointer_is_rejected() {
    // Subtraction is directional: only `p - n` is byte-addressed
    // arithmetic; `n - p` is invalid.
    let src = "fn main() { let p = rt_alloc(16); 2 - p; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T02");
}

#[test]
fn pointer_arithmetic_with_unconstrained_offset_pins_int() {
    // `p + x` with `x` otherwise unconstrained pins `x` to `Int`.
    let src = "fn main() { let p = rt_alloc(16); let x = p + 1; let y = x - 2; y; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_expr_type(src, &types, "p + 1", "Ptr<Int>");
    assert_expr_type(src, &types, "x - 2", "Ptr<Int>");
}

#[test]
fn pointer_equality_is_bool() {
    let src =
        "fn main() { let p = rt_alloc(16); let q = p + 8; let b = p == q; let c = p != q; b; c; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_expr_type(src, &types, "p == q", "Bool");
    assert_expr_type(src, &types, "p != q", "Bool");
}

#[test]
fn pointer_equality_with_non_pointer_is_rejected() {
    let src = "fn main() { let p = rt_alloc(16); p == 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
}

#[test]
fn null_pointer_zero_in_pointer_argument_position() {
    // The literal `0` is the null pointer constant only in pointer-typed
    // argument positions; it must not change the type of ordinary `0`s.
    let src = "fn main() { rt_mem_load(0); rt_free(0); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn zero_is_still_int_outside_pointer_positions() {
    let src = "fn main() { let z = 0; rt_print_int(z); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "z")), "Int");
}

#[test]
fn string_cannot_feed_raw_memory_intrinsics() {
    let src = "fn main() { let s = rt_str_alloc(4); rt_free(s); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Ptr<Int>"));
    assert_eq!(errors[0].actual(), Some("Str"));
}

#[test]
fn pointer_cannot_feed_string_intrinsics() {
    let src = "fn main() { let p = rt_alloc(16); rt_str_len(p); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Str"));
    assert_eq!(errors[0].actual(), Some("Ptr<Int>"));
}

#[test]
fn int_cannot_feed_pointer_intrinsics() {
    // Only the literal `0` is the null pointer constant; a computed `Int`
    // is not a pointer.
    let src = "fn main() { let n = 1; rt_free(n); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Ptr<Int>"));
    assert_eq!(errors[0].actual(), Some("Int"));
}

#[test]
fn string_arithmetic_is_rejected() {
    let src = "fn main() { let s = rt_str_alloc(4); s + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T02");
}

#[test]
fn pointer_types_unify_across_calls() {
    // A user function parameter constrained to `Ptr<Int>` accepts a
    // pointer from `rt_alloc` and rejects strings and ints.
    let src = "fn f(p) { rt_mem_load(p); } fn main() { let p = rt_alloc(16); f(p); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn string_types_unify_across_calls() {
    let src = "fn f(s) { rt_str_len(s); } fn main() { let s = rt_str_alloc(4); f(s); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn pointer_mismatch_across_calls_is_reported() {
    let src = "fn f(p) { rt_mem_load(p); } fn main() { f(\"hi\"); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].expected(), Some("Ptr<Int>"));
    assert_eq!(errors[0].actual(), Some("Str"));
}

// ---------------------------------------------------------------------------
// References (session 16)
// ---------------------------------------------------------------------------

#[test]
fn borrow_expressions_type_as_references() {
    let src = "fn f() { let v = 10; let r = &v; let m = &mut v; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "r")),
        "&Int"
    );
    assert_eq!(
        type_name(&types, symbol_ty(&types, &_semantic, "m")),
        "&mut Int"
    );
    assert_expr_type(src, &types, "&v", "&Int");
    assert_expr_type(src, &types, "&mut v", "&mut Int");
}

#[test]
fn shared_and_mutable_reference_types_are_distinct() {
    // `&T` and `&mut T` must not unify: assigning a shared reference where
    // a mutable one is required is a mismatch.
    let src = "fn f() { let v = 10; let r = &v; let m = &mut v; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    let r = symbol_ty(&types, &_semantic, "r");
    let m = symbol_ty(&types, &_semantic, "m");
    assert_ne!(r, m, "`&T` and `&mut T` must be distinct interned types");
}

#[test]
fn deref_reads_type_as_the_referent() {
    let src = "fn f() { let v = 10; let r = &v; let x = *r; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_expr_type(src, &types, "*r", "Int");
    assert_eq!(type_name(&types, symbol_ty(&types, &_semantic, "x")), "Int");
}

#[test]
fn deref_writes_through_mutable_references_type_check() {
    let src = "fn f() { let v = 10; let m = &mut v; *m = 42; *m += 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn reference_referent_mismatch_is_rejected() {
    // Writing a `Str` through an `&mut Int` is a type mismatch (E-T01),
    // not a silent acceptance.
    let src = "fn f() { let v = 10; let m = &mut v; *m = \"hi\"; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert!(!errors.is_empty());
    assert_eq!(errors[0].code(), "E-T01");
}

#[test]
fn assignment_through_immutable_reference_is_rejected() {
    let src = "fn f() { let v = 10; let r = &v; *r = 5; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::AssignThroughImmutableRef);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T21");
}

#[test]
fn deref_of_non_reference_is_rejected() {
    let src = "fn f() { let x = 10; *x; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::DerefNonReference);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T20");
}

#[test]
fn borrow_of_non_place_is_rejected() {
    // Borrowing a literal is not a place expression (E-T19).
    let src = "fn f() { let r = &10; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidBorrowTarget);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T19");
}

#[test]
fn borrow_of_an_expression_result_is_rejected() {
    // Borrowing a call result (not a place) is rejected (E-T19); the
    // borrow checker also rejects borrowing a reference (reborrowing is
    // deferred), but non-place borrows are already caught here.
    let src = "fn g() { return 1; } fn f() { let r = &g(); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidBorrowTarget);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T19");
}

#[test]
fn reference_field_types_resolve() {
    let src = "struct S { r: &Int } fn main() { }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
}

#[test]
fn references_are_distinct_from_pointers() {
    // `&v` is a `&Int`, never a `Ptr<Int>`; the two type families must
    // not unify with each other.
    let src = "fn f() { let v = 10; let r = &v; rt_mem_load(r); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::TypeMismatch);
    assert!(!errors.is_empty());
    assert_eq!(errors[0].expected(), Some("Ptr<Int>"));
    assert_eq!(errors[0].actual(), Some("&Int"));
}

#[test]
fn references_flow_through_calls() {
    let src = "fn bump(p) { *p = *p + 1; } fn main() { let v = 10; let m = &mut v; bump(m); }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    assert!(!types.has_errors());
    assert_expr_type(src, &types, "*p", "Int");
}

#[test]
fn reference_binary_operations_are_rejected() {
    // References are not numeric: `r + 1` is invalid (E-T02).
    let src = "fn f() { let v = 10; let r = &v; r + 1; }";
    let (_sources, _ast, _semantic, types) = check_src(src);
    let errors = type_errors(&types, TypeErrorKind::InvalidOperator);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code(), "E-T02");
}
