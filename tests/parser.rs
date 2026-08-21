//! Integration tests for the MINK parser and AST.
//!
//! Tests verify AST structure (node kinds, nesting, operators, precedence,
//! names, spans), valid and invalid programs, error recovery, and safety
//! invariants over malformed inputs.

use mink::ast::{
    AssignOp, BinaryOp, Block, ElseBranch, Expr, ExprKind, Ident, IfStmt, Item, ItemKind,
    MatchStmt, Stmt, StmtKind, Ty, TyKind, UnaryOp,
};
use mink::parser::{ParseErrorKind, ParseOutput, parse};
use mink::source::{SourceMap, Span};

/// Parses `src` as a virtual `test.mink` file.
fn parse_src(src: &str) -> ParseOutput {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file is present");
    parse(file)
}

/// Parses `src` and returns the AST, asserting there are no errors.
fn parsed(src: &str) -> mink::ast::Ast {
    let output = parse_src(src);
    assert!(
        output.lex_errors().is_empty(),
        "unexpected lexical errors for {src:?}: {:?}",
        output.lex_errors()
    );
    assert!(
        output.parse_errors().is_empty(),
        "unexpected parse errors for {src:?}: {:?}",
        output.parse_errors()
    );
    output.ast().clone()
}

/// Parses `src` and returns the parse-error kinds, in order.
fn error_kinds(src: &str) -> Vec<ParseErrorKind> {
    parse_src(src)
        .parse_errors()
        .iter()
        .map(|e| e.kind())
        .collect()
}

/// Parses `src` as the body of a function and returns its statements.
fn stmts(src: &str) -> Vec<Stmt> {
    let ast = parsed(&format!("fn f() {{ {src} }}"));
    let ItemKind::Fn(func) = &ast.items()[0].kind else {
        panic!("expected a function item")
    };
    func.body.stmts.clone()
}

/// Parses `src` as the body of a function and returns its first statement.
fn stmt(src: &str) -> Stmt {
    let mut statements = stmts(src);
    assert_eq!(statements.len(), 1, "expected one statement for {src:?}");
    statements.remove(0)
}

/// Parses `src` as an expression (`let v = <src>;` inside a function).
fn expr(src: &str) -> Expr {
    let statements = stmts(&format!("let v = {src};"));
    let StmtKind::Let(binding) = &statements[0].kind else {
        panic!("expected a let statement")
    };
    binding.init.clone()
}

/// Asserts `e` is a binary expression with `op`, returning (lhs, rhs).
fn binary(e: &Expr, op: BinaryOp) -> (&Expr, &Expr) {
    match &e.kind {
        ExprKind::Binary {
            op: actual,
            lhs,
            rhs,
        } => {
            assert_eq!(*actual, op, "unexpected operator for {e:?}");
            (lhs, rhs)
        }
        other => panic!("expected binary {op:?}, found {other:?}"),
    }
}

/// Asserts `e` is a unary expression with `op`, returning the operand.
fn unary(e: &Expr, op: UnaryOp) -> &Expr {
    match &e.kind {
        ExprKind::Unary {
            op: actual,
            operand,
        } => {
            assert_eq!(*actual, op, "unexpected operator for {e:?}");
            operand
        }
        other => panic!("expected unary {op:?}, found {other:?}"),
    }
}

/// Asserts `e` is an identifier with the given name.
fn ident(e: &Expr, name: &str) {
    match &e.kind {
        ExprKind::Ident(Ident { name: actual, .. }) => assert_eq!(actual, name),
        other => panic!("expected identifier {name:?}, found {other:?}"),
    }
}

/// Asserts `e` is an integer literal.
fn int_lit(e: &Expr) {
    assert!(
        matches!(&e.kind, ExprKind::Int),
        "expected Int, found {:?}",
        e.kind
    );
}

// ---------------------------------------------------------------------------
// Program shape
// ---------------------------------------------------------------------------

#[test]
fn empty_source_parses_to_empty_program() {
    let ast = parsed("");
    assert!(ast.is_empty());
    assert!(ast.items().is_empty());
}

#[test]
fn comments_and_whitespace_only_parse_to_empty_program() {
    let ast = parsed("// a comment\n/* block */\n\n  \t");
    assert!(ast.is_empty());
}

#[test]
fn minimal_function_parses() {
    let ast = parsed("fn main() {}");
    assert_eq!(ast.items().len(), 1);
    let Item {
        kind: ItemKind::Fn(func),
        span,
    } = &ast.items()[0]
    else {
        panic!("expected a function item")
    };
    assert_eq!(func.name.name, "main");
    assert!(func.params.is_empty());
    assert!(func.body.stmts.is_empty());
    assert_eq!(span.range(), 0..12);
}

#[test]
fn top_level_let_and_const_are_items() {
    let ast = parsed("let x = 1;\nconst LIMIT = 10;\n");
    assert_eq!(ast.items().len(), 2);
    assert!(matches!(ast.items()[0].kind, ItemKind::Let(_)));
    assert!(matches!(ast.items()[1].kind, ItemKind::Const(_)));
}

#[test]
fn multiple_items_are_kept_in_source_order() {
    let ast = parsed("fn a() {} fn b() {} const C = 1; let d = 2;");
    assert_eq!(ast.items().len(), 4);
    assert_eq!(ast.items()[0].span.range(), 0..9);
    assert_eq!(ast.items()[3].span.range(), 33..43);
}

#[test]
fn top_level_semicolons_are_ignored() {
    // An empty statement at module scope is accepted silently.
    let ast = parsed("fn main() {};\n;");
    assert_eq!(ast.items().len(), 1);
}

#[test]
fn parse_output_exposes_ast_and_counts() {
    let output = parse_src("fn main() { return 42; }");
    assert!(output.is_valid());
    assert_eq!(output.token_count(), 9); // fn main ( ) { return 42 ; } = 9
    assert!(output.lex_errors().is_empty());
    assert!(output.parse_errors().is_empty());
    assert_eq!(output.ast().items().len(), 1);

    let (ast, lex_errors, parse_errors) = parse_src("let x = 1;").into_parts();
    assert_eq!(ast.items().len(), 1);
    assert!(lex_errors.is_empty());
    assert!(parse_errors.is_empty());
}

// ---------------------------------------------------------------------------
// Declarations
// ---------------------------------------------------------------------------

#[test]
fn function_with_parameters() {
    let ast = parsed("fn add(a, b) {}");
    let ItemKind::Fn(func) = &ast.items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.params.len(), 2);
    assert_eq!(func.params[0].name.name, "a");
    assert_eq!(func.params[1].name.name, "b");
    assert_eq!(func.params[0].span.range(), 7..8);
}

#[test]
fn function_parameters_allow_trailing_comma() {
    let ast = parsed("fn f(a, b,) {}");
    let ItemKind::Fn(func) = &ast.items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.params.len(), 2);
}

#[test]
fn let_bindings_are_immutable_by_default() {
    let statements = stmts("let x = 1;");
    let StmtKind::Let(binding) = &statements[0].kind else {
        panic!("expected a let statement")
    };
    assert_eq!(binding.name.name, "x");
    assert!(!binding.mutable);
    assert!(matches!(&binding.init.kind, ExprKind::Int));
}

#[test]
fn let_mut_bindings_are_mutable() {
    let statements = stmts("let mut x = 1;");
    let StmtKind::Let(binding) = &statements[0].kind else {
        panic!("expected a let statement")
    };
    assert!(binding.mutable);
}

#[test]
fn const_bindings_parse() {
    let ast = parsed("const LIMIT = 100;");
    let ItemKind::Const(binding) = &ast.items()[0].kind else {
        panic!("expected a const item")
    };
    assert_eq!(binding.name.name, "LIMIT");
    assert!(matches!(&binding.init.kind, ExprKind::Int));
}

#[test]
fn bindings_appear_inside_function_bodies() {
    let statements = stmts("let a = 1; let mut b = 2; const C = 3;");
    assert_eq!(statements.len(), 3);
    assert!(matches!(statements[0].kind, StmtKind::Let(_)));
    assert!(matches!(statements[1].kind, StmtKind::Let(_)));
    assert!(matches!(statements[2].kind, StmtKind::Const(_)));
}

// ---------------------------------------------------------------------------
// Literals and identifiers
// ---------------------------------------------------------------------------

#[test]
fn every_literal_kind_is_represented() {
    let statements = stmts(
        "let a = 42; let b = 1.5; let c = \"hi\"; let d = 'x'; let e = true; let f = false; let g = null;",
    );
    let kinds: Vec<&ExprKind> = statements
        .iter()
        .map(|s| match &s.kind {
            StmtKind::Let(binding) => &binding.init.kind,
            other => panic!("expected let, found {other:?}"),
        })
        .collect();
    assert!(matches!(kinds[0], ExprKind::Int));
    assert!(matches!(kinds[1], ExprKind::Float));
    assert!(matches!(kinds[2], ExprKind::Str));
    assert!(matches!(kinds[3], ExprKind::Char));
    assert!(matches!(kinds[4], ExprKind::Bool(true)));
    assert!(matches!(kinds[5], ExprKind::Bool(false)));
    assert!(matches!(kinds[6], ExprKind::Null));
}

#[test]
fn identifier_expressions_preserve_spelling() {
    let e = expr("some_name");
    ident(&e, "some_name");
}

#[test]
fn literal_values_are_recovered_from_source_spans() {
    let mut map = SourceMap::new();
    let id = map.add(
        "test.mink",
        "fn f() { let a = 42; let b = 1.5; let c = \"hi\"; let d = 'x'; }",
    );
    let file = map.get(id).unwrap();
    let output = parse(file);
    assert!(output.is_valid());
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let expected_texts = ["42", "1.5", "\"hi\"", "'x'"];
    for (stmt, expected) in func.body.stmts.iter().zip(expected_texts) {
        let StmtKind::Let(binding) = &stmt.kind else {
            panic!("expected a let statement")
        };
        assert_eq!(file.span_text(binding.init.span), Some(expected));
    }
}

#[test]
fn strings_and_chars_support_unicode_content() {
    let e = expr("\"héllo 世界\"");
    assert!(matches!(e.kind, ExprKind::Str));
    let e = expr("'é'");
    assert!(matches!(e.kind, ExprKind::Char));
}

// ---------------------------------------------------------------------------
// Unary expressions
// ---------------------------------------------------------------------------

#[test]
fn unary_operators_parse() {
    let e = expr("-x");
    let operand = unary(&e, UnaryOp::Neg);
    ident(operand, "x");

    let e = expr("!ready");
    let operand = unary(&e, UnaryOp::Not);
    ident(operand, "ready");

    let e = expr("~bits");
    let operand = unary(&e, UnaryOp::BitNot);
    ident(operand, "bits");
}

#[test]
fn unary_operators_stack() {
    let e = expr("--x");
    let inner = unary(&e, UnaryOp::Neg);
    let operand = unary(inner, UnaryOp::Neg);
    ident(operand, "x");

    let e = expr("!!flag");
    let inner = unary(&e, UnaryOp::Not);
    let operand = unary(inner, UnaryOp::Not);
    ident(operand, "flag");
}

#[test]
fn unary_binds_tighter_than_binary() {
    let e = expr("-a * b");
    let (lhs, rhs) = binary(&e, BinaryOp::Mul);
    let operand = unary(lhs, UnaryOp::Neg);
    ident(operand, "a");
    ident(rhs, "b");
}

// ---------------------------------------------------------------------------
// Binary expressions, precedence, and associativity
// ---------------------------------------------------------------------------

#[test]
fn multiplication_binds_tighter_than_addition() {
    let e = expr("1 + 2 * 3");
    let (lhs, rhs) = binary(&e, BinaryOp::Add);
    int_lit(lhs);
    let (lhs, rhs) = binary(rhs, BinaryOp::Mul);
    int_lit(lhs);
    int_lit(rhs);
}

#[test]
fn addition_binds_looser_than_multiplication() {
    let e = expr("1 * 2 + 3");
    let (lhs, rhs) = binary(&e, BinaryOp::Add);
    int_lit(rhs);
    let (lhs, rhs) = binary(lhs, BinaryOp::Mul);
    int_lit(lhs);
    int_lit(rhs);
}

#[test]
fn grouping_overrides_precedence() {
    let e = expr("(1 + 2) * 3");
    let (lhs, rhs) = binary(&e, BinaryOp::Mul);
    int_lit(rhs);
    let ExprKind::Group(inner) = &lhs.kind else {
        panic!("expected a group, found {:?}", lhs.kind)
    };
    let (lhs, rhs) = binary(inner, BinaryOp::Add);
    int_lit(lhs);
    int_lit(rhs);
}

#[test]
fn binary_operators_are_left_associative() {
    // `a - b - c` groups as `(a - b) - c`.
    let e = expr("a - b - c");
    let (lhs, rhs) = binary(&e, BinaryOp::Sub);
    ident(rhs, "c");
    let (lhs, rhs) = binary(lhs, BinaryOp::Sub);
    ident(lhs, "a");
    ident(rhs, "b");
}

#[test]
fn equality_binds_looser_than_relational() {
    // `a == b < c` groups as `a == (b < c)`.
    let e = expr("a == b < c");
    let (lhs, rhs) = binary(&e, BinaryOp::Eq);
    ident(lhs, "a");
    let (lhs, rhs) = binary(rhs, BinaryOp::Lt);
    ident(lhs, "b");
    ident(rhs, "c");
}

#[test]
fn logical_and_binds_tighter_than_logical_or() {
    let e = expr("a || b && c");
    let (lhs, rhs) = binary(&e, BinaryOp::Or);
    ident(lhs, "a");
    let (lhs, rhs) = binary(rhs, BinaryOp::And);
    ident(lhs, "b");
    ident(rhs, "c");
}

#[test]
fn logical_not_applies_to_the_operand_only() {
    let e = expr("!a && b");
    let (lhs, rhs) = binary(&e, BinaryOp::And);
    let operand = unary(lhs, UnaryOp::Not);
    ident(operand, "a");
    ident(rhs, "b");
}

#[test]
fn bitwise_precedence_follows_c_convention() {
    // `a | b ^ c & d` groups as `a | (b ^ (c & d))`.
    let e = expr("a | b ^ c & d");
    let (lhs, rhs) = binary(&e, BinaryOp::BitOr);
    ident(lhs, "a");
    let (lhs, rhs) = binary(rhs, BinaryOp::BitXor);
    ident(lhs, "b");
    let (lhs, rhs) = binary(rhs, BinaryOp::BitAnd);
    ident(lhs, "c");
    ident(rhs, "d");
}

#[test]
fn shift_binds_looser_than_addition() {
    let e = expr("a << b + c");
    let (lhs, rhs) = binary(&e, BinaryOp::Shl);
    ident(lhs, "a");
    let (lhs, rhs) = binary(rhs, BinaryOp::Add);
    ident(lhs, "b");
    ident(rhs, "c");
}

#[test]
fn every_binary_operator_maps_to_its_kind() {
    let cases: &[(&str, BinaryOp)] = &[
        ("a + b", BinaryOp::Add),
        ("a - b", BinaryOp::Sub),
        ("a * b", BinaryOp::Mul),
        ("a / b", BinaryOp::Div),
        ("a % b", BinaryOp::Rem),
        ("a << b", BinaryOp::Shl),
        ("a >> b", BinaryOp::Shr),
        ("a < b", BinaryOp::Lt),
        ("a <= b", BinaryOp::Le),
        ("a > b", BinaryOp::Gt),
        ("a >= b", BinaryOp::Ge),
        ("a == b", BinaryOp::Eq),
        ("a != b", BinaryOp::Ne),
        ("a & b", BinaryOp::BitAnd),
        ("a ^ b", BinaryOp::BitXor),
        ("a | b", BinaryOp::BitOr),
        ("a && b", BinaryOp::And),
        ("a || b", BinaryOp::Or),
    ];
    for (src, expected) in cases {
        let e = expr(src);
        assert!(
            matches!(
                &e.kind,
                ExprKind::Binary { op, .. } if *op == *expected
            ),
            "for {src:?}: expected {expected:?}, found {:?}",
            e.kind
        );
    }
}

#[test]
fn chained_binary_operations_group_left() {
    // `1 + 2 + 3` groups as `(1 + 2) + 3`.
    let e = expr("1 + 2 + 3");
    let (lhs, rhs) = binary(&e, BinaryOp::Add);
    int_lit(rhs);
    let (lhs, rhs) = binary(lhs, BinaryOp::Add);
    int_lit(lhs);
    int_lit(rhs);
}

// ---------------------------------------------------------------------------
// Calls, member access, and indexing
// ---------------------------------------------------------------------------

#[test]
fn calls_with_arguments() {
    let e = expr("f(1, 2)");
    let ExprKind::Call { callee, args } = &e.kind else {
        panic!("expected a call, found {:?}", e.kind)
    };
    ident(callee, "f");
    assert_eq!(args.len(), 2);
    assert!(matches!(&args[0].kind, ExprKind::Int));
    assert!(matches!(&args[1].kind, ExprKind::Int));
}

#[test]
fn calls_allow_trailing_comma() {
    let e = expr("f(a, b,)");
    let ExprKind::Call { args, .. } = &e.kind else {
        panic!("expected a call")
    };
    assert_eq!(args.len(), 2);
}

#[test]
fn calls_without_arguments() {
    let e = expr("f()");
    let ExprKind::Call { args, .. } = &e.kind else {
        panic!("expected a call")
    };
    assert!(args.is_empty());
}

#[test]
fn nested_calls() {
    let e = expr("f(g(x))");
    let ExprKind::Call { callee, args } = &e.kind else {
        panic!("expected a call")
    };
    ident(callee, "f");
    assert_eq!(args.len(), 1);
    let ExprKind::Call { .. } = &args[0].kind else {
        panic!("expected a nested call")
    };
}

#[test]
fn member_access_chains() {
    let e = expr("a.b.c");
    let ExprKind::Member { base, member } = &e.kind else {
        panic!("expected member access")
    };
    assert_eq!(member.name, "c");
    let ExprKind::Member { base, member } = &base.kind else {
        panic!("expected nested member access")
    };
    assert_eq!(member.name, "b");
    ident(base, "a");
}

#[test]
fn method_calls_parse_as_member_then_call() {
    let e = expr("a.b(x)");
    let ExprKind::Call { callee, .. } = &e.kind else {
        panic!("expected a call")
    };
    let ExprKind::Member { base, member } = &callee.kind else {
        panic!("expected member access on the callee")
    };
    ident(base, "a");
    assert_eq!(member.name, "b");
}

#[test]
fn indexing_expressions() {
    let e = expr("a[0]");
    let ExprKind::Index { base, index } = &e.kind else {
        panic!("expected an index expression")
    };
    ident(base, "a");
    assert!(matches!(&index.kind, ExprKind::Int));

    let e = expr("a[i + 1]");
    let ExprKind::Index { index, .. } = &e.kind else {
        panic!("expected an index expression")
    };
    let (lhs, rhs) = binary(index, BinaryOp::Add);
    ident(lhs, "i");
    int_lit(rhs);
}

#[test]
fn mixed_postfix_chains() {
    let e = expr("a.b[0](x).c");
    let ExprKind::Member { base, member } = &e.kind else {
        panic!("expected member access")
    };
    assert_eq!(member.name, "c");
    let ExprKind::Call { callee, .. } = &base.kind else {
        panic!("expected a call")
    };
    let ExprKind::Index { base, .. } = &callee.kind else {
        panic!("expected an index expression")
    };
    let ExprKind::Member { base, member } = &base.kind else {
        panic!("expected member access")
    };
    ident(base, "a");
    assert_eq!(member.name, "b");
}

// ---------------------------------------------------------------------------
// References: borrows, derefs, and reference types (session 16)
// ---------------------------------------------------------------------------

#[test]
fn borrow_expressions_parse() {
    let e = expr("&v");
    let ExprKind::Borrow {
        mutable: false,
        operand,
    } = &e.kind
    else {
        panic!("expected a borrow, found {:?}", e.kind)
    };
    ident(operand, "v");

    let e = expr("&mut v");
    let ExprKind::Borrow {
        mutable: true,
        operand,
    } = &e.kind
    else {
        panic!("expected a mutable borrow, found {:?}", e.kind)
    };
    ident(operand, "v");
}

#[test]
fn borrow_operand_keeps_postfix_chains() {
    // `&p.x[0]` borrows the element, not just `p`.
    let e = expr("&p.x[0]");
    let ExprKind::Borrow { operand, .. } = &e.kind else {
        panic!("expected a borrow")
    };
    let ExprKind::Index { base, .. } = &operand.kind else {
        panic!("expected an index inside the borrow")
    };
    let ExprKind::Member { base, member } = &base.kind else {
        panic!("expected a member access inside the borrow")
    };
    ident(base, "p");
    assert_eq!(member.name, "x");
}

#[test]
fn deref_expressions_parse() {
    let e = expr("*r");
    let ExprKind::Deref { operand } = &e.kind else {
        panic!("expected a deref, found {:?}", e.kind)
    };
    ident(operand, "r");

    let e = expr("**r");
    let inner = match &e.kind {
        ExprKind::Deref { operand } => operand,
        _ => panic!("expected a deref"),
    };
    let ExprKind::Deref { operand } = &inner.kind else {
        panic!("expected a nested deref")
    };
    ident(operand, "r");
}

#[test]
fn deref_targets_parse_as_assignment_places() {
    let e = expr("*r = 5");
    let ExprKind::Assign { target, value, .. } = &e.kind else {
        panic!("expected an assignment")
    };
    let ExprKind::Deref { operand } = &target.kind else {
        panic!("expected a deref target, found {:?}", target.kind)
    };
    ident(operand, "r");
    int_lit(value);

    let e = expr("*r += 1");
    let ExprKind::Assign { op, .. } = &e.kind else {
        panic!("expected an assignment")
    };
    assert_eq!(*op, AssignOp::AddAssign);
}

#[test]
fn reference_types_parse_in_struct_fields() {
    // `&T` and `&mut T` appear in struct field declarations (the only
    // place the frozen grammar carries type annotations).
    let ast = parsed("struct A { r: &Int } struct B { r: &mut Int }");
    let ItemKind::Struct(s) = &ast.items()[0].kind else {
        panic!("expected a struct")
    };
    let TyKind::Ref {
        mutable: false,
        inner,
    } = &s.fields[0].ty.kind
    else {
        panic!("expected a shared reference type")
    };
    let TyKind::Named(ident) = &inner.kind else {
        panic!("expected the referent type")
    };
    assert_eq!(ident.name, "Int");

    let ItemKind::Struct(s) = &ast.items()[1].kind else {
        panic!("expected a struct")
    };
    let TyKind::Ref {
        mutable: true,
        inner,
    } = &s.fields[0].ty.kind
    else {
        panic!("expected a mutable reference type")
    };
    let TyKind::Named(ident) = &inner.kind else {
        panic!("expected the referent type")
    };
    assert_eq!(ident.name, "Int");
}

#[test]
fn reference_types_chain_into_aggregates() {
    // `&[Int; 4]` parses as a reference whose referent is an array type.
    let ast = parsed("struct A { r: &[Int; 4] }");
    let ItemKind::Struct(s) = &ast.items()[0].kind else {
        panic!("expected a struct")
    };
    let TyKind::Ref { inner, .. } = &s.fields[0].ty.kind else {
        panic!("expected a reference type")
    };
    assert!(matches!(&inner.kind, TyKind::Array { .. }));
}

#[test]
fn enum_declaration_is_an_item() {
    // `enum` declarations (session 17) are top-level items with variants
    // in declaration order; trailing commas and empty variant lists parse.
    let ast = parsed("enum Color { Red, Green, Blue, }");
    let ItemKind::Enum(e) = &ast.items()[0].kind else {
        panic!("expected an enum item")
    };
    assert_eq!(e.name.name, "Color");
    assert_eq!(
        e.variants
            .iter()
            .map(|v| v.name.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Red", "Green", "Blue"]
    );

    let ast = parsed("enum Empty {}");
    let ItemKind::Enum(e) = &ast.items()[0].kind else {
        panic!("expected an enum item")
    };
    assert!(e.variants.is_empty());
}

#[test]
fn enum_variant_paths_parse_as_expressions() {
    // `E::V` is the variant-construction expression; the enum and variant
    // names are kept as separate identifiers.
    let e = expr("Color::Red");
    let ExprKind::EnumVariant {
        name,
        variant,
        payload: _,
    } = &e.kind
    else {
        panic!("expected an enum-variant expression")
    };
    assert_eq!(name.name, "Color");
    assert_eq!(variant.name, "Red");
}

#[test]
fn enum_variant_path_requires_a_variant_name() {
    // `E::` with no variant is a structured parse error (E-P22).
    assert_eq!(
        error_kinds("enum E { A } fn main() { let x = E::; }"),
        vec![ParseErrorKind::ExpectedVariant]
    );
}

#[test]
fn dangling_reference_types_are_rejected_at_parse() {
    // `&` with no referent and `&&T` (a reference-to-reference type) do not
    // parse; type analysis would reject the latter anyway, so the parser
    // reports `ExpectedType` up front.
    assert_eq!(
        error_kinds("struct A { r: & }"),
        vec![ParseErrorKind::ExpectedType]
    );
    assert_eq!(
        error_kinds("struct A { r: &&Int }"),
        vec![ParseErrorKind::ExpectedType]
    );
}

// ---------------------------------------------------------------------------
// Assignment and ranges
// ---------------------------------------------------------------------------

#[test]
fn assignment_parses_with_target_and_value() {
    let e = expr("x = 5");
    let ExprKind::Assign { op, target, value } = &e.kind else {
        panic!("expected an assignment")
    };
    assert_eq!(*op, AssignOp::Assign);
    ident(target, "x");
    int_lit(value);
}

#[test]
fn assignment_is_right_associative() {
    let e = expr("a = b = c");
    let ExprKind::Assign { target, value, .. } = &e.kind else {
        panic!("expected an assignment")
    };
    ident(target, "a");
    let ExprKind::Assign { target, .. } = &value.kind else {
        panic!("expected a nested assignment")
    };
    ident(target, "b");
}

#[test]
fn every_compound_assignment_operator_parses() {
    let cases: &[(&str, AssignOp)] = &[
        ("x = 1", AssignOp::Assign),
        ("x += 1", AssignOp::AddAssign),
        ("x -= 1", AssignOp::SubAssign),
        ("x *= 1", AssignOp::MulAssign),
        ("x /= 1", AssignOp::DivAssign),
        ("x %= 1", AssignOp::RemAssign),
    ];
    for (src, expected) in cases {
        let e = expr(src);
        let ExprKind::Assign { op, .. } = &e.kind else {
            panic!("expected an assignment for {src:?}")
        };
        assert_eq!(*op, *expected, "for {src:?}");
    }
}

#[test]
fn assignment_targets_can_be_members_and_indexes() {
    let e = expr("a.b = 1");
    assert!(matches!(e.kind, ExprKind::Assign { .. }));

    let e = expr("a[0] = 1");
    assert!(matches!(e.kind, ExprKind::Assign { .. }));
}

#[test]
fn assignment_to_non_place_is_rejected() {
    assert_eq!(
        error_kinds("fn f() { 1 = 2; }"),
        vec![ParseErrorKind::ExpectedAssignmentTarget]
    );
    assert_eq!(
        error_kinds("fn f() { (x) = 5; }"),
        vec![ParseErrorKind::ExpectedAssignmentTarget]
    );
}

#[test]
fn range_expressions_parse() {
    let e = expr("0..10");
    let ExprKind::Range {
        inclusive,
        start,
        end,
    } = &e.kind
    else {
        panic!("expected a range")
    };
    assert!(!*inclusive);
    int_lit(start);
    int_lit(end);

    let e = expr("0..=10");
    let ExprKind::Range { inclusive, .. } = &e.kind else {
        panic!("expected a range")
    };
    assert!(*inclusive);
}

#[test]
fn ranges_bind_looser_than_binary_operators() {
    let e = expr("1 + 2..3 * 4");
    let ExprKind::Range { start, end, .. } = &e.kind else {
        panic!("expected a range")
    };
    let (lhs, rhs) = binary(start, BinaryOp::Add);
    int_lit(lhs);
    int_lit(rhs);
    let (lhs, rhs) = binary(end, BinaryOp::Mul);
    int_lit(lhs);
    int_lit(rhs);
}

// ---------------------------------------------------------------------------
// Statements and control flow
// ---------------------------------------------------------------------------

#[test]
fn return_without_value() {
    let s = stmt("return;");
    assert!(matches!(s.kind, StmtKind::Return(None)));
}

#[test]
fn return_with_value() {
    let s = stmt("return 42;");
    let StmtKind::Return(Some(value)) = &s.kind else {
        panic!("expected a return with a value")
    };
    assert!(matches!(&value.kind, ExprKind::Int));
}

#[test]
fn if_else_statement() {
    let s = stmt("if a { } else { }");
    let StmtKind::If(if_stmt) = &s.kind else {
        panic!("expected an if statement")
    };
    ident(&if_stmt.cond, "a");
    assert!(if_stmt.then_block.stmts.is_empty());
    let Some(ElseBranch::Block(block)) = &if_stmt.else_branch else {
        panic!("expected an else block")
    };
    assert!(block.stmts.is_empty());
}

#[test]
fn else_if_chains_nest() {
    let s = stmt("if a { } else if b { } else { }");
    let StmtKind::If(if_stmt) = &s.kind else {
        panic!("expected an if statement")
    };
    let Some(ElseBranch::If(nested)) = &if_stmt.else_branch else {
        panic!("expected an else-if branch")
    };
    ident(&nested.cond, "b");
    let Some(ElseBranch::Block(_)) = &nested.else_branch else {
        panic!("expected the final else block")
    };
}

#[test]
fn while_loop() {
    let s = stmt("while a { break; }");
    let StmtKind::While { cond, body } = &s.kind else {
        panic!("expected a while loop")
    };
    ident(cond, "a");
    assert!(matches!(&body.stmts[0].kind, StmtKind::Break(_)));
}

#[test]
fn for_loop_with_range() {
    let s = stmt("for i in 0..10 { }");
    let StmtKind::For {
        name,
        iterable,
        body,
    } = &s.kind
    else {
        panic!("expected a for loop")
    };
    assert_eq!(name.name, "i");
    assert!(matches!(
        iterable.kind,
        ExprKind::Range {
            inclusive: false,
            ..
        }
    ));
    assert!(body.stmts.is_empty());
}

#[test]
fn loop_break_continue() {
    let statements = stmts("loop { continue; break; }");
    let StmtKind::Loop(body) = &statements[0].kind else {
        panic!("expected a loop statement")
    };
    assert!(matches!(&body.stmts[0].kind, StmtKind::Continue));
    assert!(matches!(&body.stmts[1].kind, StmtKind::Break(_)));
}

#[test]
fn expression_statements_require_semicolons() {
    let s = stmt("f();");
    assert!(matches!(s.kind, StmtKind::Expr(_)));
}

#[test]
fn empty_statements_are_ignored() {
    let statements = stmts("; ; let x = 1; ;");
    assert_eq!(statements.len(), 1);
    assert!(matches!(&statements[0].kind, StmtKind::Let(_)));
}

#[test]
fn blocks_nest() {
    let statements = stmts("if a { if b { } }");
    let StmtKind::If(outer) = &statements[0].kind else {
        panic!("expected an if statement")
    };
    let StmtKind::If(inner) = &outer.then_block.stmts[0].kind else {
        panic!("expected a nested if statement")
    };
    ident(&inner.cond, "b");
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

#[test]
fn comments_around_items_and_statements() {
    let ast = parsed(
        "// header\nfn main() { // body start\n    let x = 1; /* inline */\n    return x; // tail\n} // end\n",
    );
    assert_eq!(ast.items().len(), 1);
    let ItemKind::Fn(func) = &ast.items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.body.stmts.len(), 2);
}

#[test]
fn comments_between_tokens_do_not_join_them() {
    let ast = parsed("fn/*a*/main/*b*/()/*c*/{}");
    let ItemKind::Fn(func) = &ast.items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.name.name, "main");
}

// ---------------------------------------------------------------------------
// Invalid input and recovery
// ---------------------------------------------------------------------------

#[test]
fn top_level_unexpected_tokens_are_rejected() {
    assert_eq!(error_kinds("123"), vec![ParseErrorKind::ExpectedItem]);
    assert_eq!(error_kinds("}"), vec![ParseErrorKind::ExpectedItem]);
    assert_eq!(
        error_kinds("fn main() {} else {}"),
        vec![ParseErrorKind::ExpectedItem]
    );
}

#[test]
fn function_requires_a_name() {
    assert_eq!(
        error_kinds("fn () {}"),
        vec![ParseErrorKind::ExpectedIdentifier]
    );
}

#[test]
fn function_requires_an_open_paren() {
    assert_eq!(
        error_kinds("fn main { }"),
        vec![ParseErrorKind::ExpectedLParen]
    );
}

#[test]
fn function_requires_a_block_body() {
    assert_eq!(
        error_kinds("fn main() 42;"),
        vec![ParseErrorKind::ExpectedBlock]
    );
}

#[test]
fn unclosed_paren_at_eof_is_reported() {
    assert_eq!(error_kinds("fn f("), vec![ParseErrorKind::UnclosedParen]);
    assert_eq!(error_kinds("fn f(a"), vec![ParseErrorKind::UnclosedParen]);
    assert_eq!(error_kinds("fn f(a,"), vec![ParseErrorKind::UnclosedParen]);
}

#[test]
fn unclosed_brace_at_eof_is_reported() {
    assert_eq!(error_kinds("fn f() {"), vec![ParseErrorKind::UnclosedBrace]);
    assert_eq!(
        error_kinds("fn f() { let x = 1;"),
        vec![ParseErrorKind::UnclosedBrace]
    );
    // Nested constructs inside a function: only the innermost unclosed brace
    // is reported, not every level still open at end of input.
    assert_eq!(
        error_kinds("fn f() { if x {"),
        vec![ParseErrorKind::UnclosedBrace]
    );
    assert_eq!(
        error_kinds("fn f() { while x {"),
        vec![ParseErrorKind::UnclosedBrace]
    );
    assert_eq!(
        error_kinds("fn f() { loop {"),
        vec![ParseErrorKind::UnclosedBrace]
    );
    assert_eq!(
        error_kinds("fn f() { for i in xs {"),
        vec![ParseErrorKind::UnclosedBrace]
    );
}

#[test]
fn unclosed_bracket_at_eof_is_reported() {
    assert_eq!(
        error_kinds("fn f() { let x = a[0; }"),
        vec![ParseErrorKind::ExpectedRBracket]
    );
    assert_eq!(
        error_kinds("fn f() { let x = a[0"),
        vec![ParseErrorKind::UnclosedBracket]
    );
}

#[test]
fn missing_equal_in_binding_is_reported() {
    assert_eq!(error_kinds("let x 5;"), vec![ParseErrorKind::ExpectedEqual]);
    assert_eq!(
        error_kinds("const C 5;"),
        vec![ParseErrorKind::ExpectedEqual]
    );
}

#[test]
fn missing_initializer_is_reported() {
    assert_eq!(
        error_kinds("let x = ;"),
        vec![ParseErrorKind::ExpectedExpression]
    );
}

#[test]
fn missing_semicolon_is_reported() {
    assert_eq!(
        error_kinds("let x = 1"),
        vec![ParseErrorKind::ExpectedSemicolon]
    );
    assert_eq!(
        error_kinds("fn f() { return 1 }"),
        vec![ParseErrorKind::ExpectedSemicolon]
    );
    assert_eq!(
        error_kinds("fn f() { break }"),
        vec![ParseErrorKind::ExpectedSemicolon]
    );
}

#[test]
fn missing_operand_is_reported() {
    assert_eq!(
        error_kinds("fn f() { let x = 1 + ; }"),
        vec![ParseErrorKind::ExpectedExpression]
    );
    assert_eq!(
        error_kinds("fn f() { let x = a && ; }"),
        vec![ParseErrorKind::ExpectedExpression]
    );
}

#[test]
fn unexpected_end_of_input_in_expression_is_reported() {
    // The missing expression is the primary error; the enclosing block is
    // also genuinely unclosed, so both facts are reported.
    assert_eq!(
        error_kinds("fn f() { let x = "),
        vec![ParseErrorKind::UnexpectedEof, ParseErrorKind::UnclosedBrace]
    );
}

#[test]
fn missing_rparen_in_group_is_reported() {
    assert_eq!(
        error_kinds("fn f() { let x = (1 + 2; }"),
        vec![ParseErrorKind::ExpectedRParen]
    );
}

#[test]
fn missing_block_after_if_is_reported() {
    assert_eq!(
        error_kinds("fn f() { if a 5; }"),
        vec![ParseErrorKind::ExpectedBlock]
    );
    assert_eq!(
        error_kinds("fn f() { if a { } else 5; }"),
        vec![ParseErrorKind::ExpectedBlock]
    );
}

#[test]
fn missing_in_in_for_loop_is_reported() {
    // The for loop's own block is skipped as a unit during recovery, so the
    // enclosing function still closes cleanly: a single error.
    assert_eq!(
        error_kinds("fn f() { for x 0..10 { } }"),
        vec![ParseErrorKind::ExpectedIn]
    );
    assert_eq!(
        error_kinds("fn f() { for x 0..10; }"),
        vec![ParseErrorKind::ExpectedIn]
    );
    assert_eq!(
        error_kinds("fn f() { for in xs { } }"),
        vec![ParseErrorKind::ExpectedIdentifier]
    );
}

#[test]
fn missing_comma_between_parameters_is_reported() {
    assert_eq!(
        error_kinds("fn f(a b) {}"),
        vec![ParseErrorKind::ExpectedComma]
    );
    assert_eq!(
        error_kinds("fn f() { g(1 2); }"),
        vec![ParseErrorKind::ExpectedComma]
    );
}

#[test]
fn stray_semicolon_in_args_does_not_swallow_the_block() {
    // A `;` inside an argument list ends the statement cleanly: the enclosing
    // function block stays intact and only the missing-comma error is
    // reported.
    assert_eq!(
        error_kinds("fn f() { g(1; } let x = 2;"),
        vec![ParseErrorKind::ExpectedComma]
    );
    assert_eq!(
        error_kinds("fn f(a } let x = 2;"),
        vec![ParseErrorKind::ExpectedComma]
    );
}

#[test]
fn multiple_independent_errors_are_all_reported() {
    let src = "fn f() { let a = ; let b = ; }";
    assert_eq!(
        error_kinds(src),
        vec![
            ParseErrorKind::ExpectedExpression,
            ParseErrorKind::ExpectedExpression,
        ]
    );
}

#[test]
fn recovery_preserves_later_items() {
    let output = parse_src("let x = ; fn main() { }");
    assert_eq!(
        output.parse_errors().len(),
        1,
        "the broken item must not poison the next item"
    );
    assert_eq!(output.ast().items().len(), 1);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.name.name, "main");
}

#[test]
fn recovery_preserves_later_statements() {
    let output = parse_src("fn f() { let a = ; let b = 2; }");
    assert_eq!(output.parse_errors().len(), 1);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.body.stmts.len(), 1);
    let StmtKind::Let(binding) = &func.body.stmts[0].kind else {
        panic!("expected a let statement")
    };
    assert_eq!(binding.name.name, "b");
}

#[test]
fn recovery_from_missing_block_keeps_parsing() {
    // `fn a() let x = 1; fn b() {}` has one missing block; the declaration
    // after it still parses as its own item.
    let output = parse_src("fn a() let x = 1; fn b() {}");
    assert_eq!(output.parse_errors().len(), 1);
    assert_eq!(output.ast().items().len(), 3);
    let ItemKind::Fn(func) = &output.ast().items()[2].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.name.name, "b");
}

#[test]
fn eof_mid_construct_never_panics() {
    for src in [
        "fn", "fn f", "fn f(", "fn f(a,", "let", "let x", "let x =", "if", "if (", "if x", "while",
        "for", "for x", "for x in", "loop", "loop {", "return", "return x", "break", "continue",
        "x =", "f(", "f(1,", "a[", "a[0", "(", "(1", "(1 +", "1 +", "a..",
    ] {
        let output = parse_src(src);
        assert!(
            !output.parse_errors().is_empty(),
            "input {src:?} should not be silently valid"
        );
    }
}

#[test]
fn malformed_input_never_panics_and_spans_stay_in_bounds() {
    // Deterministic pseudo-random byte corpus (valid UTF-8 via lossy decode,
    // mirroring how files are loaded), exercising every parser code path.
    let mut state = 0x853c49e6748fea9bu64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..2_000 {
        let len = (next() % 64) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        let src = String::from_utf8_lossy(&bytes);
        check_invariants(&src);
    }
}

#[test]
fn targeted_malformed_corpus() {
    for src in [
        "fn",
        "fn f(",
        "fn f() {",
        "let",
        "let x = ;",
        "if x {",
        "while {",
        "for x {",
        "loop { break",
        "return ;",
        "x =",
        "((",
        "))",
        "a[",
        "a]",
        "1..",
        "..2",
        "1..=2",
        "f(",
        "f(1,",
        "f(,)",
        ";;;",
        "@@@",
        "{{{{",
        "}}}}",
        "let mut",
        "const x =",
        "else",
        "in",
        "fn f() else",
        "1 = 2",
        "mut x = 1",
        "a . b",
        "x[0] = 1 = 2",
        "a || || b",
        "a + + b",
        "- - a",
        "\"str\"",
        "fn f() { \"unterminated",
        "let héllo = 5;",
        "if a { } else if",
        "fn main() { return; }",
        "fn a() {} fn b() {",
        "0x",
        "0b12",
        "a.b.c.d",
    ] {
        check_invariants(src);
    }
}

/// Asserts the parser invariants for `src`: it never panics, error spans are
/// in bounds, and every AST node span is in bounds and non-inverted.
fn check_invariants(src: &str) {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file is present");
    let output = parse(file);
    let text_len = src.len() as u32;

    for error in output.parse_errors() {
        let span = error.span();
        assert!(
            span.start() <= span.end(),
            "inverted error span for {src:?}"
        );
        assert!(
            span.end() <= text_len,
            "error span out of bounds for {src:?}"
        );
    }
    for item in output.ast().items() {
        walk_item(item, text_len, src);
    }
}

fn walk_item(item: &Item, text_len: u32, src: &str) {
    assert_span_ok(item.span, text_len, src);
    match &item.kind {
        ItemKind::Fn(func) => {
            walk_ident(&func.name, text_len, src);
            for param in &func.params {
                walk_ident(&param.name, text_len, src);
            }
            walk_block(&func.body, text_len, src);
        }
        ItemKind::Let(binding) => {
            walk_ident(&binding.name, text_len, src);
            walk_expr(&binding.init, text_len, src);
        }
        ItemKind::Const(binding) => {
            walk_ident(&binding.name, text_len, src);
            walk_expr(&binding.init, text_len, src);
        }
        ItemKind::Struct(s) => {
            walk_ident(&s.name, text_len, src);
            for field in &s.fields {
                walk_ident(&field.name, text_len, src);
                walk_ty(&field.ty, text_len);
            }
        }
        ItemKind::Enum(e) => {
            walk_ident(&e.name, text_len, src);
            for variant in &e.variants {
                walk_ident(&variant.name, text_len, src);
            }
        }
        ItemKind::Module(m) => {
            walk_ident(&m.name, text_len, src);
        }
        ItemKind::Use(u) => {
            for segment in &u.path {
                walk_ident(segment, text_len, src);
            }
        }
        ItemKind::Pub(p) => {
            walk_item(&p.item, text_len, src);
        }
    }
}

fn walk_ty(ty: &Ty, text_len: u32) {
    assert_span_ok(ty.span, text_len, "type");
    match &ty.kind {
        TyKind::Named(ident) => walk_ident(ident, text_len, "type"),
        TyKind::Ptr(inner) => walk_ty(inner, text_len),
        TyKind::Ref { inner, .. } => walk_ty(inner, text_len),
        TyKind::Array { elem, len } => {
            walk_ty(elem, text_len);
            walk_expr(len, text_len, "type");
        }
        TyKind::Tuple(elems) => {
            for elem in elems {
                walk_ty(elem, text_len);
            }
        }
    }
}

fn walk_block(block: &Block, text_len: u32, src: &str) {
    assert_span_ok(block.span, text_len, src);
    for stmt in &block.stmts {
        walk_stmt(stmt, text_len, src);
    }
}

fn walk_stmt(stmt: &Stmt, text_len: u32, src: &str) {
    assert_span_ok(stmt.span, text_len, src);
    match &stmt.kind {
        StmtKind::Let(binding) => {
            walk_ident(&binding.name, text_len, src);
            walk_expr(&binding.init, text_len, src);
        }
        StmtKind::Const(binding) => {
            walk_ident(&binding.name, text_len, src);
            walk_expr(&binding.init, text_len, src);
        }
        StmtKind::Return(value) => {
            if let Some(value) = value {
                walk_expr(value, text_len, src);
            }
        }
        StmtKind::Break(_) | StmtKind::Continue => {}
        StmtKind::If(if_stmt) => walk_if_stmt(if_stmt, text_len, src),
        StmtKind::While { cond, body } => {
            walk_expr(cond, text_len, src);
            walk_block(body, text_len, src);
        }
        StmtKind::For {
            name,
            iterable,
            body,
        } => {
            walk_ident(name, text_len, src);
            walk_expr(iterable, text_len, src);
            walk_block(body, text_len, src);
        }
        StmtKind::Loop(body) => walk_block(body, text_len, src),
        StmtKind::Match(stmt) => walk_match_stmt(stmt, text_len, src),
        StmtKind::Expr(expr) => walk_expr(expr, text_len, src),
    }
}

fn walk_match_stmt(stmt: &MatchStmt, text_len: u32, src: &str) {
    assert_span_ok(stmt.span, text_len, src);
    walk_expr(&stmt.scrutinee, text_len, src);
    for arm in &stmt.arms {
        assert_span_ok(arm.pattern.span(), text_len, src);
        walk_block(&arm.body, text_len, src);
    }
}

fn walk_if_stmt(if_stmt: &IfStmt, text_len: u32, src: &str) {
    assert_span_ok(if_stmt.span, text_len, src);
    walk_expr(&if_stmt.cond, text_len, src);
    walk_block(&if_stmt.then_block, text_len, src);
    if let Some(branch) = &if_stmt.else_branch {
        match branch {
            ElseBranch::If(nested) => walk_if_stmt(nested, text_len, src),
            ElseBranch::IfExpr(inner) => {
                walk_expr(&inner.cond, text_len, src);
                walk_block(&inner.then_block, text_len, src);
            }
            ElseBranch::Block(block) => walk_block(block, text_len, src),
        }
    }
}

fn walk_expr(expr: &Expr, text_len: u32, src: &str) {
    assert_span_ok(expr.span, text_len, src);
    match &expr.kind {
        ExprKind::Int
        | ExprKind::Float
        | ExprKind::Str
        | ExprKind::Char
        | ExprKind::Bool(_)
        | ExprKind::Null => {}
        ExprKind::Ident(ident) => walk_ident(ident, text_len, src),
        ExprKind::Unary { operand, .. } => walk_expr(operand, text_len, src),
        ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => {
            walk_expr(operand, text_len, src)
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, text_len, src);
            walk_expr(rhs, text_len, src);
        }
        ExprKind::Assign { target, value, .. } => {
            walk_expr(target, text_len, src);
            walk_expr(value, text_len, src);
        }
        ExprKind::Range { start, end, .. } => {
            walk_expr(start, text_len, src);
            walk_expr(end, text_len, src);
        }
        ExprKind::Call { callee, args } => {
            walk_expr(callee, text_len, src);
            for arg in args {
                walk_expr(arg, text_len, src);
            }
        }
        ExprKind::Member { base, member } => {
            walk_expr(base, text_len, src);
            walk_ident(member, text_len, src);
        }
        ExprKind::Index { base, index } => {
            walk_expr(base, text_len, src);
            walk_expr(index, text_len, src);
        }
        ExprKind::StructLit { name, fields } => {
            walk_ident(name, text_len, src);
            for field in fields {
                walk_ident(&field.name, text_len, src);
                walk_expr(&field.value, text_len, src);
            }
        }
        ExprKind::ArrayLit(elems) => {
            for elem in elems {
                walk_expr(elem, text_len, src);
            }
        }
        ExprKind::EnumVariant {
            name,
            variant,
            payload,
        } => {
            walk_ident(name, text_len, src);
            walk_ident(variant, text_len, src);
            if let Some(payload) = payload {
                walk_expr(payload, text_len, src);
            }
        }
        ExprKind::Group(inner) => walk_expr(inner, text_len, src),
        ExprKind::IfExpr(inner) => {
            walk_expr(&inner.cond, text_len, src);
            walk_block(&inner.then_block, text_len, src);
        }
        ExprKind::Block(block) => {
            for stmt in &block.stmts {
                walk_stmt(stmt, text_len, src);
            }
            if let Some(result) = &block.result {
                walk_expr(result, text_len, src);
            }
        }
        ExprKind::Tuple(elems) => {
            for elem in elems {
                walk_expr(elem, text_len, src);
            }
        }
        ExprKind::TupleFieldAccess { base, .. } => {
            walk_expr(base, text_len, src);
        }
        ExprKind::WhileExpr { cond, body, .. } => {
            walk_expr(cond, text_len, src);
            walk_block(body, text_len, src);
        }
        ExprKind::LoopExpr { body, .. } => {
            walk_block(body, text_len, src);
        }
        ExprKind::MatchExpr(m) => {
            assert_span_ok(m.span, text_len, src);
            walk_expr(&m.scrutinee, text_len, src);
            for arm in &m.arms {
                assert_span_ok(arm.pattern.span(), text_len, src);
                walk_expr(&arm.body, text_len, src);
            }
        }
    }
}

fn walk_ident(ident: &Ident, text_len: u32, src: &str) {
    assert_span_ok(ident.span, text_len, src);
}

fn assert_span_ok(span: Span, text_len: u32, src: &str) {
    assert!(span.start() <= span.end(), "inverted span for {src:?}");
    assert!(span.end() <= text_len, "span out of bounds for {src:?}");
}

// ---------------------------------------------------------------------------
// Spans
// ---------------------------------------------------------------------------

#[test]
fn spans_cover_exact_source_ranges() {
    // fn main() { return 42; }
    // 012345678901234567890123
    let src = "fn main() { return 42; }";
    let ast = parsed(src);
    let Item {
        kind: ItemKind::Fn(func),
        span,
    } = &ast.items()[0]
    else {
        panic!("expected a function item")
    };
    assert_eq!(span.range(), 0..24);
    assert_eq!(func.name.span.range(), 3..7);
    assert_eq!(func.body.span.range(), 10..24);
    let StmtKind::Return(Some(value)) = &func.body.stmts[0].kind else {
        panic!("expected a return statement")
    };
    assert_eq!(func.body.stmts[0].span.range(), 12..22);
    assert_eq!(value.span.range(), 19..21);
}

#[test]
fn expression_spans_cover_full_postfix_chains() {
    let src = "fn f() { let v = a.b[0](x); }";
    let ast = parsed(src);
    let ItemKind::Fn(func) = &ast.items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::Let(binding) = &func.body.stmts[0].kind else {
        panic!("expected a let statement")
    };
    // `a.b[0](x)` spans bytes 17..26.
    assert_eq!(binding.init.span.range(), 17..26);
}

#[test]
fn group_spans_include_parens() {
    let src = "fn f() { let v = (1 + 2); }";
    let ast = parsed(src);
    let ItemKind::Fn(func) = &ast.items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::Let(binding) = &func.body.stmts[0].kind else {
        panic!("expected a let statement")
    };
    let ExprKind::Group(inner) = &binding.init.kind else {
        panic!("expected a group")
    };
    // The group `(1 + 2)` spans bytes 17..24; the inner expression 18..23.
    assert_eq!(binding.init.span.range(), 17..24);
    assert_eq!(inner.span.range(), 18..23);
}

// ---------------------------------------------------------------------------
// Operator symbols
// ---------------------------------------------------------------------------

#[test]
fn operator_symbols_are_stable() {
    use mink::ast::{AssignOp, BinaryOp, UnaryOp};

    assert_eq!(UnaryOp::Neg.symbol(), "-");
    assert_eq!(UnaryOp::Not.symbol(), "!");
    assert_eq!(UnaryOp::BitNot.symbol(), "~");

    let binary_symbols = [
        (BinaryOp::Add, "+"),
        (BinaryOp::Sub, "-"),
        (BinaryOp::Mul, "*"),
        (BinaryOp::Div, "/"),
        (BinaryOp::Rem, "%"),
        (BinaryOp::Shl, "<<"),
        (BinaryOp::Shr, ">>"),
        (BinaryOp::Lt, "<"),
        (BinaryOp::Le, "<="),
        (BinaryOp::Gt, ">"),
        (BinaryOp::Ge, ">="),
        (BinaryOp::Eq, "=="),
        (BinaryOp::Ne, "!="),
        (BinaryOp::BitAnd, "&"),
        (BinaryOp::BitXor, "^"),
        (BinaryOp::BitOr, "|"),
        (BinaryOp::And, "&&"),
        (BinaryOp::Or, "||"),
    ];
    for (op, symbol) in binary_symbols {
        assert_eq!(op.symbol(), symbol);
    }

    let assign_symbols = [
        (AssignOp::Assign, "="),
        (AssignOp::AddAssign, "+="),
        (AssignOp::SubAssign, "-="),
        (AssignOp::MulAssign, "*="),
        (AssignOp::DivAssign, "/="),
        (AssignOp::RemAssign, "%="),
    ];
    for (op, symbol) in assign_symbols {
        assert_eq!(op.symbol(), symbol);
    }
}

// ---------------------------------------------------------------------------
// Depth and scale
// ---------------------------------------------------------------------------

#[test]
fn deeply_nested_parentheses_parse() {
    // Each nesting level recurses through the full expression chain (roughly
    // 30 frames), so parse on a thread with a generous stack. The depth is a
    // deliberate, documented bound: arbitrarily deep nesting is not a goal of
    // this milestone (see PARSER_IMPLEMENTATION.md).
    let depth = 300;
    let src = format!(
        "fn f() {{ let x = {}1{}; }}",
        "(".repeat(depth),
        ")".repeat(depth)
    );
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut map = SourceMap::new();
            let id = map.add("test.mink", &src);
            let file = map.get(id).expect("added file is present");
            let output = parse(file);
            assert!(output.is_valid());
            let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
                panic!("expected a function item")
            };
            let StmtKind::Let(binding) = &func.body.stmts[0].kind else {
                panic!("expected a let statement")
            };
            let mut current = &binding.init;
            let mut groups = 0;
            while let ExprKind::Group(inner) = &current.kind {
                groups += 1;
                current = inner;
            }
            assert!(matches!(&current.kind, ExprKind::Int));
            assert_eq!(groups, depth);
        })
        .expect("test thread spawns")
        .join()
        .expect("test thread did not panic");
}

#[test]
fn long_program_parses() {
    let mut program = String::new();
    for i in 0..500 {
        program.push_str(&format!(
            "fn f{i}(a, b) {{ let x = a + b * {i}; if x > 0 {{ return x; }} return 0; }}\n"
        ));
    }
    let output = parse_src(&program);
    assert!(output.parse_errors().is_empty());
    assert_eq!(output.ast().items().len(), 500);
    // Each function body lexes to 30 tokens (excluding Eof).
    assert_eq!(output.token_count(), 500 * 30);
}
