//! Hardening tests for the MINK parser (session 04).
//!
//! These tests stress the parser beyond the session-03 suite: a full
//! delimiter matrix, an exhaustive precedence/associativity matrix over the
//! frozen operator table, postfix combinations with exact span assertions,
//! syntactic assignment-target validation, statement/item boundary recovery,
//! recovery stress corpora, deterministic malformed-input fuzzing (never
//! panics, spans stay in bounds, error counts stay bounded), regressions that
//! excluded (future) syntax is never silently accepted, unicode byte-span
//! accuracy, and long-chain scale behavior.

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

/// Parses `src` and returns the parse-error kinds, in order.
fn error_kinds(src: &str) -> Vec<ParseErrorKind> {
    parse_src(src)
        .parse_errors()
        .iter()
        .map(|e| e.kind())
        .collect()
}

/// Parses `src` and returns `true` when it produces no parse errors.
fn parse_ok(src: &str) -> bool {
    parse_src(src).parse_errors().is_empty()
}

/// Parses `src` as the body of a function and returns its statements.
fn stmts(src: &str) -> Vec<Stmt> {
    let output = parse_src(&format!("fn f() {{ {src} }}"));
    assert!(
        output.lex_errors().is_empty() && output.parse_errors().is_empty(),
        "unexpected errors for {src:?}: {:?}",
        output.parse_errors()
    );
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    func.body.stmts.clone()
}

/// Parses `src` as an expression (`let v = <src>;` inside a function).
fn expr(src: &str) -> Expr {
    let statements = stmts(&format!("let v = {src};"));
    let StmtKind::Let(binding) = &statements[0].kind else {
        panic!("expected a let statement")
    };
    binding.init.clone()
}

// ---------------------------------------------------------------------------
// Tree-shape helpers
// ---------------------------------------------------------------------------

fn bin(e: &Expr, op: BinaryOp) -> (&Expr, &Expr) {
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

fn un(e: &Expr, op: UnaryOp) -> &Expr {
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

fn ident(e: &Expr, name: &str) {
    match &e.kind {
        ExprKind::Ident(Ident { name: actual, .. }) => assert_eq!(actual, name),
        other => panic!("expected identifier {name:?}, found {other:?}"),
    }
}

fn is_int(e: &Expr) {
    assert!(
        matches!(&e.kind, ExprKind::Int),
        "expected Int, found {:?}",
        e.kind
    );
}

fn call(e: &Expr) -> (&Expr, &[Expr]) {
    match &e.kind {
        ExprKind::Call { callee, args, .. } => (callee, args),
        other => panic!("expected a call, found {other:?}"),
    }
}

fn member(e: &Expr) -> (&Expr, &str) {
    match &e.kind {
        ExprKind::Member { base, member } => (base, &member.name),
        other => panic!("expected member access, found {other:?}"),
    }
}

fn index(e: &Expr) -> (&Expr, &Expr) {
    match &e.kind {
        ExprKind::Index { base, index } => (base, index),
        other => panic!("expected an index expression, found {other:?}"),
    }
}

fn assign(e: &Expr) -> (AssignOp, &Expr, &Expr) {
    match &e.kind {
        ExprKind::Assign { op, target, value } => (*op, target, value),
        other => panic!("expected an assignment, found {other:?}"),
    }
}

fn group(e: &Expr) -> &Expr {
    match &e.kind {
        ExprKind::Group(inner) => inner,
        other => panic!("expected a group, found {other:?}"),
    }
}

fn range(e: &Expr, inclusive: bool) -> (&Expr, &Expr) {
    match &e.kind {
        ExprKind::Range {
            inclusive: actual,
            start,
            end,
        } => {
            assert_eq!(*actual, inclusive, "unexpected inclusivity for {e:?}");
            (start, end)
        }
        other => panic!("expected a range, found {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Delimiter matrix
// ---------------------------------------------------------------------------

#[test]
fn stray_closing_delimiters_at_top_level_are_rejected() {
    for src in [")", "]", "}"] {
        assert_eq!(
            error_kinds(src),
            vec![ParseErrorKind::ExpectedItem],
            "for {src:?}"
        );
    }
}

#[test]
fn missing_closing_paren_in_call_is_unclosed() {
    assert_eq!(
        error_kinds("fn f() { g(1"),
        vec![ParseErrorKind::UnclosedParen]
    );
    assert_eq!(
        error_kinds("fn f() { g(1,"),
        vec![ParseErrorKind::UnclosedParen]
    );
    assert_eq!(
        error_kinds("fn f() { g(1, 2"),
        vec![ParseErrorKind::UnclosedParen]
    );
    assert_eq!(
        error_kinds("fn f() { g(1, 2,"),
        vec![ParseErrorKind::UnclosedParen]
    );
}

#[test]
fn missing_closing_bracket_is_unclosed() {
    assert_eq!(
        error_kinds("fn f() { a[0"),
        vec![ParseErrorKind::UnclosedBracket]
    );
}

#[test]
fn mismatched_closers_are_rejected() {
    // `]` where `)` was expected.
    assert_eq!(
        error_kinds("fn f() { let x = (1]; }"),
        vec![ParseErrorKind::ExpectedRParen]
    );
    // `)` where `]` was expected.
    assert_eq!(
        error_kinds("fn f() { let x = a[0); }"),
        vec![ParseErrorKind::ExpectedRBracket]
    );
    // `;` where `]` was expected, before the block closes.
    assert_eq!(
        error_kinds("fn f() { let x = a[0; }"),
        vec![ParseErrorKind::ExpectedRBracket]
    );
}

#[test]
fn delimiter_error_spans_point_at_offending_token() {
    // `fn f() { let x = (1]; }` — the `]` is at byte 19.
    let src = "fn f() { let x = (1]; }";
    let output = parse_src(src);
    let errors = output.parse_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind(), ParseErrorKind::ExpectedRParen);
    assert_eq!(errors[0].span().range(), 19..20);
}

#[test]
fn unclosed_bracket_error_spans_point_at_opener() {
    // `fn f() { a[0` — the `[` is at byte 10; at end of input the
    // unclosed-bracket diagnostic points at the opener.
    let src = "fn f() { a[0";
    let output = parse_src(src);
    let errors = output.parse_errors();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind(), ParseErrorKind::UnclosedBracket);
    assert_eq!(errors[0].span().range(), 10..11);
}

#[test]
fn nested_delimiters_parse_cleanly() {
    let statements = stmts("g((1)); a[0]; (a.b)[2]; f(f(a[0].b));");
    assert_eq!(statements.len(), 4);
}

#[test]
fn eof_inside_nested_delimiters_reports_the_innermost() {
    // The inner group `(1` is the innermost unclosed delimiter; the outer
    // call paren and the function body brace are not reported.
    assert_eq!(
        error_kinds("fn f() { g((1)"),
        vec![ParseErrorKind::UnclosedParen]
    );
    // The bracket is the innermost unclosed delimiter inside a group.
    assert_eq!(
        error_kinds("fn f() { (a[0"),
        vec![ParseErrorKind::UnclosedBracket]
    );
}

#[test]
fn multiple_malformed_delimiters_are_all_reported() {
    let src = "let x = 1; } fn f { }";
    assert_eq!(
        error_kinds(src),
        vec![ParseErrorKind::ExpectedItem, ParseErrorKind::ExpectedLParen]
    );
}

#[test]
fn stray_closer_inside_a_statement_is_a_statement_error() {
    // `let x = 1);` — the stray `)` terminates the expression, and the
    // missing `;` is reported at the `)`.
    assert_eq!(
        error_kinds("fn f() { let x = 1); }"),
        vec![ParseErrorKind::ExpectedSemicolon]
    );
}

#[test]
fn unexpected_opener_in_expr_position_is_rejected() {
    // Block expressions `{ 1 }` are now valid (session 28).
    // `{ 1 }` parses as a block expression returning 1.
    let errors = error_kinds("fn f() { let x = { 1 }; }");
    assert!(
        errors.is_empty(),
        "expected no errors for block expression, got {errors:?}"
    );
}

// ---------------------------------------------------------------------------
// Precedence matrix
// ---------------------------------------------------------------------------

#[test]
fn precedence_mix_additive_multiplicative() {
    let e = expr("a + b * c");
    let (lhs, rhs) = bin(&e, BinaryOp::Add);
    ident(lhs, "a");
    let (lhs, rhs) = bin(rhs, BinaryOp::Mul);
    ident(lhs, "b");
    ident(rhs, "c");

    let e = expr("a * b + c");
    let (lhs, rhs) = bin(&e, BinaryOp::Add);
    ident(rhs, "c");
    let (lhs, rhs) = bin(lhs, BinaryOp::Mul);
    ident(lhs, "a");
    ident(rhs, "b");
}

#[test]
fn precedence_mix_shift_additive() {
    let e = expr("a << b + c");
    let (lhs, rhs) = bin(&e, BinaryOp::Shl);
    ident(lhs, "a");
    let (lhs, rhs) = bin(rhs, BinaryOp::Add);
    ident(lhs, "b");
    ident(rhs, "c");

    let e = expr("a + b >> c");
    let (lhs, rhs) = bin(&e, BinaryOp::Shr);
    ident(rhs, "c");
    let (lhs, rhs) = bin(lhs, BinaryOp::Add);
    ident(lhs, "a");
    ident(rhs, "b");
}

#[test]
fn precedence_mix_equality_relational() {
    let e = expr("a == b < c");
    let (lhs, rhs) = bin(&e, BinaryOp::Eq);
    ident(lhs, "a");
    let (lhs, rhs) = bin(rhs, BinaryOp::Lt);
    ident(lhs, "b");
    ident(rhs, "c");

    // `a < b == c < d` groups as `(a < b) == (c < d)`.
    let e = expr("a < b == c < d");
    let (lhs, rhs) = bin(&e, BinaryOp::Eq);
    let (l, r) = bin(lhs, BinaryOp::Lt);
    ident(l, "a");
    ident(r, "b");
    let (l, r) = bin(rhs, BinaryOp::Lt);
    ident(l, "c");
    ident(r, "d");
}

#[test]
fn precedence_mix_logical_and_or() {
    // `a && b || c` groups as `(a && b) || c`.
    let e = expr("a && b || c");
    let (lhs, rhs) = bin(&e, BinaryOp::Or);
    ident(rhs, "c");
    let (lhs, rhs) = bin(lhs, BinaryOp::And);
    ident(lhs, "a");
    ident(rhs, "b");

    // `a || b && c || d` groups as `((a || (b && c)) || d)`.
    let e = expr("a || b && c || d");
    let (lhs, rhs) = bin(&e, BinaryOp::Or);
    ident(rhs, "d");
    let (lhs, rhs) = bin(lhs, BinaryOp::Or);
    ident(lhs, "a");
    let (lhs, rhs) = bin(rhs, BinaryOp::And);
    ident(lhs, "b");
    ident(rhs, "c");
}

#[test]
fn precedence_mix_bitwise_levels() {
    // `a | b ^ c & d` groups as `a | (b ^ (c & d))`.
    let e = expr("a | b ^ c & d");
    let (lhs, rhs) = bin(&e, BinaryOp::BitOr);
    ident(lhs, "a");
    let (lhs, rhs) = bin(rhs, BinaryOp::BitXor);
    ident(lhs, "b");
    let (lhs, rhs) = bin(rhs, BinaryOp::BitAnd);
    ident(lhs, "c");
    ident(rhs, "d");

    // `a ^ b | c` groups as `(a ^ b) | c`.
    let e = expr("a ^ b | c");
    let (lhs, rhs) = bin(&e, BinaryOp::BitOr);
    ident(rhs, "c");
    let (lhs, rhs) = bin(lhs, BinaryOp::BitXor);
    ident(lhs, "a");
    ident(rhs, "b");

    // `a & b ^ c` groups as `(a & b) ^ c`.
    let e = expr("a & b ^ c");
    let (lhs, rhs) = bin(&e, BinaryOp::BitXor);
    ident(rhs, "c");
    let (lhs, rhs) = bin(lhs, BinaryOp::BitAnd);
    ident(lhs, "a");
    ident(rhs, "b");
}

#[test]
fn precedence_mix_bitwise_equality() {
    // `a & b == c` groups as `a & (b == c)` — equality binds tighter.
    let e = expr("a & b == c");
    let (lhs, rhs) = bin(&e, BinaryOp::BitAnd);
    ident(lhs, "a");
    let (lhs, rhs) = bin(rhs, BinaryOp::Eq);
    ident(lhs, "b");
    ident(rhs, "c");

    // `a < b | c` groups as `(a < b) | c` — relational binds tighter.
    let e = expr("a < b | c");
    let (lhs, rhs) = bin(&e, BinaryOp::BitOr);
    ident(rhs, "c");
    let (lhs, rhs) = bin(lhs, BinaryOp::Lt);
    ident(lhs, "a");
    ident(rhs, "b");
}

#[test]
fn precedence_mix_equality_and_logical() {
    // `a + b == c && d` groups as `((a + b) == c) && d`.
    let e = expr("a + b == c && d");
    let (lhs, rhs) = bin(&e, BinaryOp::And);
    ident(rhs, "d");
    let (lhs, rhs) = bin(lhs, BinaryOp::Eq);
    ident(rhs, "c");
    let (lhs, rhs) = bin(lhs, BinaryOp::Add);
    ident(lhs, "a");
    ident(rhs, "b");
}

#[test]
fn precedence_mix_unary_binary() {
    // `!a == b` groups as `(!a) == b`.
    let e = expr("!a == b");
    let (lhs, rhs) = bin(&e, BinaryOp::Eq);
    ident(rhs, "b");
    ident(un(lhs, UnaryOp::Not), "a");

    // `-a + b` groups as `(-a) + b`.
    let e = expr("-a + b");
    let (lhs, rhs) = bin(&e, BinaryOp::Add);
    ident(rhs, "b");
    ident(un(lhs, UnaryOp::Neg), "a");

    // `a + -b` groups as `a + (-b)`.
    let e = expr("a + -b");
    let (lhs, rhs) = bin(&e, BinaryOp::Add);
    ident(lhs, "a");
    ident(un(rhs, UnaryOp::Neg), "b");

    // `~x << 1` groups as `(~x) << 1`.
    let e = expr("~x << 1");
    let (lhs, rhs) = bin(&e, BinaryOp::Shl);
    is_int(rhs);
    ident(un(lhs, UnaryOp::BitNot), "x");
}

#[test]
fn binary_levels_are_left_associative() {
    let cases: &[(&str, BinaryOp)] = &[
        ("a - b - c", BinaryOp::Sub),
        ("a / b / c", BinaryOp::Div),
        ("a << b << c", BinaryOp::Shl),
        ("a < b < c", BinaryOp::Lt),
        ("a == b == c", BinaryOp::Eq),
        ("a && b && c", BinaryOp::And),
        ("a | b | c", BinaryOp::BitOr),
    ];
    for (src, expected) in cases {
        let e = expr(src);
        let (lhs, rhs) = bin(&e, *expected);
        ident(rhs, "c");
        let (lhs, rhs) = bin(lhs, *expected);
        ident(lhs, "a");
        ident(rhs, "b");
    }
    // `a + b - c` groups as `(a + b) - c` — same level, mixed operators.
    let e = expr("a + b - c");
    let (lhs, rhs) = bin(&e, BinaryOp::Sub);
    ident(rhs, "c");
    let (lhs, rhs) = bin(lhs, BinaryOp::Add);
    ident(lhs, "a");
    ident(rhs, "b");

    // `a % b * c` groups as `(a % b) * c`.
    let e = expr("a % b * c");
    let (lhs, rhs) = bin(&e, BinaryOp::Mul);
    ident(rhs, "c");
    let (lhs, rhs) = bin(lhs, BinaryOp::Rem);
    ident(lhs, "a");
    ident(rhs, "b");
}

#[test]
fn assignment_and_ranges_are_right_associative() {
    let e = expr("a = b = c");
    let (_, target, value) = assign(&e);
    ident(target, "a");
    let (_, target, value) = assign(value);
    ident(target, "b");
    ident(value, "c");

    let e = expr("a .. b .. c");
    let (start, end) = range(&e, false);
    ident(start, "a");
    let (start, end) = range(end, false);
    ident(start, "b");
    ident(end, "c");
}

#[test]
fn range_binds_looser_than_binary_operators() {
    let e = expr("a .. b + c");
    let (start, end) = range(&e, false);
    ident(start, "a");
    let (lhs, rhs) = bin(end, BinaryOp::Add);
    ident(lhs, "b");
    ident(rhs, "c");

    let e = expr("a * b .. c");
    let (start, end) = range(&e, false);
    let (lhs, rhs) = bin(start, BinaryOp::Mul);
    ident(lhs, "a");
    ident(rhs, "b");
    ident(end, "c");
}

#[test]
fn assignment_binds_looser_than_range_but_range_is_not_a_place() {
    // `a .. b = c` parses as an assignment whose target is a range — which
    // is not a valid place, so the syntax-level restriction reports E-P04.
    assert_eq!(
        error_kinds("fn f() { a .. b = c; }"),
        vec![ParseErrorKind::ExpectedAssignmentTarget]
    );
}

#[test]
fn inclusive_ranges_nest_right() {
    let e = expr("a ..= b ..= c");
    let (start, end) = range(&e, true);
    ident(start, "a");
    let (start, end) = range(end, true);
    ident(start, "b");
    ident(end, "c");
}

// ---------------------------------------------------------------------------
// Postfix combinations
// ---------------------------------------------------------------------------

#[test]
fn postfix_combination_matrix() {
    let e = expr("foo()");
    let (callee, args) = call(&e);
    assert!(args.is_empty());
    ident(callee, "foo");

    let e = expr("foo(a)");
    let (callee, args) = call(&e);
    ident(callee, "foo");
    assert_eq!(args.len(), 1);
    ident(&args[0], "a");

    // `foo(a)(b)` is a call of a call.
    let e = expr("foo(a)(b)");
    let (callee, args) = call(&e);
    assert_eq!(args.len(), 1);
    ident(&args[0], "b");
    let (callee, args) = call(callee);
    ident(callee, "foo");
    assert_eq!(args.len(), 1);

    let e = expr("value.member");
    let (base, name) = member(&e);
    assert_eq!(name, "member");
    ident(base, "value");

    let e = expr("value[index]");
    let (base, idx) = index(&e);
    ident(base, "value");
    ident(idx, "index");

    // `value.member[index]` is an index on a member access.
    let e = expr("value.member[index]");
    let (base, idx) = index(&e);
    ident(idx, "index");
    let (base, name) = member(base);
    assert_eq!(name, "member");
    ident(base, "value");

    // `foo(a).member[index]` is an index on a member of a call.
    let e = expr("foo(a).member[index]");
    let (base, idx) = index(&e);
    ident(idx, "index");
    let (base, name) = member(base);
    assert_eq!(name, "member");
    let (callee, args) = call(base);
    ident(callee, "foo");
    assert_eq!(args.len(), 1);
}

#[test]
fn long_mixed_postfix_chain() {
    // `a.b.c[0](x).d` = member(d, call(index(member(member(a,b),c), 0), x))
    let e = expr("a.b.c[0](x).d");
    let (base, name) = member(&e);
    assert_eq!(name, "d");
    let (callee, args) = call(base);
    assert_eq!(args.len(), 1);
    ident(&args[0], "x");
    let (base, _) = index(callee);
    let (base, name) = member(base);
    assert_eq!(name, "c");
    let (base, name) = member(base);
    assert_eq!(name, "b");
    ident(base, "a");
}

#[test]
fn chained_calls_and_indexes() {
    let e = expr("f()()");
    let (callee, args) = call(&e);
    assert!(args.is_empty());
    let (callee, args) = call(callee);
    ident(callee, "f");
    assert!(args.is_empty());

    let e = expr("a[0][1]");
    let (base, idx) = index(&e);
    is_int(idx);
    let (base, idx) = index(base);
    ident(base, "a");
    is_int(idx);

    let e = expr("a.b[0](x).c");
    let (base, name) = member(&e);
    assert_eq!(name, "c");
    let (callee, args) = call(base);
    assert_eq!(args.len(), 1);
    let (base, _) = index(callee);
    let (base, name) = member(base);
    assert_eq!(name, "b");
    ident(base, "a");
}

#[test]
fn grouped_callees_are_allowed() {
    // `(a.b)(c)` — the callee is a group; this is syntactically valid even
    // though grouping is not a place (that restriction is assignment-only).
    let e = expr("(a.b)(c)");
    let (callee, args) = call(&e);
    assert_eq!(args.len(), 1);
    let ExprKind::Member { .. } = &group(callee).kind else {
        panic!("expected member access inside the group")
    };
}

#[test]
fn unary_and_postfix_combine() {
    let e = expr("-f(x)");
    let operand = un(&e, UnaryOp::Neg);
    let (callee, args) = call(operand);
    ident(callee, "f");
    assert_eq!(args.len(), 1);

    let e = expr("-a.b");
    let operand = un(&e, UnaryOp::Neg);
    let (base, name) = member(operand);
    assert_eq!(name, "b");
    ident(base, "a");

    let e = expr("a[-1]");
    let (base, idx) = index(&e);
    ident(base, "a");
    is_int(un(idx, UnaryOp::Neg));

    let e = expr("-f(x)[0].g");
    let operand = un(&e, UnaryOp::Neg);
    let (base, name) = member(operand);
    assert_eq!(name, "g");
    let (base, _) = index(base);
    let (callee, _) = call(base);
    ident(callee, "f");
}

#[test]
fn postfix_spans_cover_the_complete_expression() {
    // fn f() { let v = foo(a)(b); }
    //                 ^        ^
    //                 17       25  (foo(a)(b) = 17..26)
    let src = "fn f() { let v = foo(a)(b); }";
    let output = parse_src(src);
    assert!(output.is_valid());
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::Let(binding) = &func.body.stmts[0].kind else {
        panic!("expected a let statement")
    };
    assert_eq!(binding.init.span.range(), 17..26);
    let ExprKind::Call { callee, .. } = &binding.init.kind else {
        panic!("expected a call")
    };
    assert_eq!(callee.span.range(), 17..23); // the inner foo(a)

    // fn f() { let v = foo(a).member[index]; }
    //                 ^                  ^
    //                 17                 36  (full chain = 17..37)
    let src = "fn f() { let v = foo(a).member[index]; }";
    let output = parse_src(src);
    assert!(output.is_valid());
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::Let(binding) = &func.body.stmts[0].kind else {
        panic!("expected a let statement")
    };
    assert_eq!(binding.init.span.range(), 17..37);
    // The member name `member` spans 24..30 inside the chain.
    let ExprKind::Index { base, .. } = &binding.init.kind else {
        panic!("expected an index expression")
    };
    let ExprKind::Member { member, .. } = &base.kind else {
        panic!("expected a member access")
    };
    assert_eq!(member.span.range(), 24..30);
}

// ---------------------------------------------------------------------------
// Assignment-target validation (syntactic)
// ---------------------------------------------------------------------------

#[test]
fn valid_assignment_targets() {
    for src in ["a = b", "a.b = c", "a[0] = c", "a.b[0] = c", "a[0].b = c"] {
        let e = expr(src);
        let (_, target, _) = assign(&e);
        assert!(
            matches!(
                &target.kind,
                ExprKind::Ident(_) | ExprKind::Member { .. } | ExprKind::Index { .. }
            ),
            "for {src:?}"
        );
    }
}

#[test]
fn compound_assignment_targets_can_be_members_and_indexes() {
    let e = expr("a.b += 1");
    let (op, target, _) = assign(&e);
    assert_eq!(op, AssignOp::AddAssign);
    assert!(matches!(&target.kind, ExprKind::Member { .. }));

    let e = expr("a[0] *= 2");
    let (op, target, _) = assign(&e);
    assert_eq!(op, AssignOp::MulAssign);
    assert!(matches!(&target.kind, ExprKind::Index { .. }));

    let e = expr("a.b[0] -= 3");
    let (op, target, _) = assign(&e);
    assert_eq!(op, AssignOp::SubAssign);
    assert!(matches!(&target.kind, ExprKind::Index { .. }));
}

#[test]
fn invalid_assignment_targets_are_rejected() {
    let invalid = [
        "1 = 2",
        "a + b = c",
        "f() = c",
        "-a = c",
        "(a) = c",
        "\"str\" = c",
        "true = c",
        "a[0] + 1 = c",
        "a .. b = c",
    ];
    for src in invalid {
        let output = parse_src(&format!("fn f() {{ {src}; }}"));
        let kinds: Vec<_> = output.parse_errors().iter().map(|e| e.kind()).collect();
        assert_eq!(
            kinds,
            vec![ParseErrorKind::ExpectedAssignmentTarget],
            "for {src:?}"
        );
    }

    // `a = 1 + 2` is a *valid* assignment (the target is a place); it is
    // only the value that is an arithmetic expression.
    let e = expr("a = 1 + 2");
    let (_, target, value) = assign(&e);
    ident(target, "a");
    let (lhs, rhs) = bin(value, BinaryOp::Add);
    is_int(lhs);
    is_int(rhs);
}

#[test]
fn assignment_to_a_non_place_is_not_silently_accepted() {
    for src in [
        "1 = 2",
        "f() = c",
        "-a = c",
        "(a) = c",
        "a + b = c",
        "a .. b = c",
    ] {
        assert!(
            !parse_src(&format!("fn f() {{ {src}; }}")).is_valid(),
            "for {src:?}"
        );
    }
}

#[test]
fn chained_non_place_assignments_report_each_offender() {
    // `1 = 2 = 3` nests right; both `1` and `2` are non-places, so two
    // E-P04 diagnostics are reported, not one.
    assert_eq!(
        error_kinds("fn f() { 1 = 2 = 3; }"),
        vec![
            ParseErrorKind::ExpectedAssignmentTarget,
            ParseErrorKind::ExpectedAssignmentTarget,
        ]
    );
}

// ---------------------------------------------------------------------------
// Statement and item boundaries
// ---------------------------------------------------------------------------

#[test]
fn bad_statement_then_valid_statement() {
    let output = parse_src("fn f() { 1 + ; g(); }");
    assert_eq!(output.parse_errors().len(), 1);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.body.stmts.len(), 1);
    let StmtKind::Expr(e) = &func.body.stmts[0].kind else {
        panic!("expected an expression statement")
    };
    let (callee, _) = call(e);
    ident(callee, "g");
}

#[test]
fn bad_statement_inside_else_branch_then_valid() {
    let output = parse_src("fn f() { if a { } else { let x = ; f(); } }");
    assert_eq!(output.parse_errors().len(), 1);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::If(if_stmt) = &func.body.stmts[0].kind else {
        panic!("expected an if statement")
    };
    let Some(ElseBranch::Block(block)) = &if_stmt.else_branch else {
        panic!("expected an else block")
    };
    assert_eq!(block.stmts.len(), 1);
    let StmtKind::Expr(e) = &block.stmts[0].kind else {
        panic!("expected an expression statement")
    };
    let (callee, _) = call(e);
    ident(callee, "f");
}

#[test]
fn bad_item_then_valid_items() {
    // `trait` is an excluded top-level declaration; recovery must skip it
    // and keep parsing the valid items after it. (`enum` declarations
    // arrived with session 17 and are accepted.)
    let output = parse_src("trait T {} fn main() {} let x = 1;");
    assert_eq!(output.parse_errors().len(), 1);
    assert_eq!(output.ast().items().len(), 2);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.name.name, "main");
    let ItemKind::Let(binding) = &output.ast().items()[1].kind else {
        panic!("expected a let item")
    };
    assert_eq!(binding.name.name, "x");
}

#[test]
fn missing_function_body_does_not_consume_the_next_item() {
    // `fn f() -> int` without a body: the missing block is the error,
    // recovery skips to the next `fn`.
    let output = parse_src("fn f() -> int fn g() {}");
    assert_eq!(output.parse_errors().len(), 1);
    assert_eq!(output.ast().items().len(), 2);
    let ItemKind::Fn(func) = &output.ast().items()[1].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.name.name, "g");
}

#[test]
fn statement_boundary_recovery_keeps_the_rest_of_the_block() {
    let output = parse_src("fn f() { g(1 2 3); f(); }");
    assert_eq!(output.parse_errors().len(), 1);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.body.stmts.len(), 2);
}

#[test]
fn missing_semicolon_absorbs_the_following_tokens_but_stays_bounded() {
    // `let x = 1 let y = 2;` — one missing `;` absorbs the orphaned tokens;
    // exactly one diagnostic, and the statement itself is still produced.
    let output = parse_src("fn f() { let x = 1 let y = 2; }");
    assert_eq!(output.parse_errors().len(), 1);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.body.stmts.len(), 1);
}

// ---------------------------------------------------------------------------
// Recovery stress
// ---------------------------------------------------------------------------

#[test]
fn recovery_stress_corpus() {
    let cases: &[(&str, Vec<ParseErrorKind>)] = &[
        // A `{` in iterable position: now parsed as a block expression,
        // so the for body is missing.
        (
            "fn f() { for i in { break; } }",
            vec![ParseErrorKind::ExpectedBlock],
        ),
        // A `{` in condition position: now parsed as a block expression,
        // so the while body is missing.
        ("fn f() { while { } }", vec![ParseErrorKind::ExpectedBlock]),
        // Missing comma between params: `b` absorbed, `c` still parses.
        ("fn f(a b, c) {}", vec![ParseErrorKind::ExpectedComma]),
        // A `}` inside the parameter list: the param list aborts at the
        // statement boundary and the following `let` still parses as an item.
        (
            "fn f(a, b } let x = 1;",
            vec![ParseErrorKind::ExpectedComma],
        ),
        // Missing `;` after a return value absorbs the stray tokens.
        (
            "fn f() { return 1 2; }",
            vec![ParseErrorKind::ExpectedSemicolon],
        ),
        // An empty parameter position.
        ("fn f(,) {}", vec![ParseErrorKind::ExpectedIdentifier]),
        // Unterminated nested loop body at EOF: innermost brace reported.
        (
            "fn f() { loop { if a { } }",
            vec![ParseErrorKind::UnclosedBrace],
        ),
        // A `;` inside a group: the group aborts at the statement boundary.
        ("fn f() { (1; }", vec![ParseErrorKind::ExpectedRParen]),
        // Two independent errors across a function boundary.
        (
            "let x = ; fn f() { g(1; }",
            vec![
                ParseErrorKind::ExpectedExpression,
                ParseErrorKind::ExpectedComma,
            ],
        ),
    ];
    for (src, expected) in cases {
        assert_eq!(error_kinds(src), *expected, "for {src:?}");
    }
}

#[test]
fn recovered_params_and_args_keep_their_lists_useful() {
    // `fn f(a b, c) {}` — `b` is absorbed by the comma-recovery, `c` remains
    // a parameter.
    let output = parse_src("fn f(a b, c) {}");
    assert_eq!(output.parse_errors().len(), 1);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let names: Vec<&str> = func.params.iter().map(|p| p.name.name.as_str()).collect();
    assert_eq!(names, vec!["a", "c"]);

    // `g(1 2 3);` — `2` and `3` are absorbed, the call keeps one argument.
    let output = parse_src("fn f() { g(1 2 3); }");
    assert_eq!(output.parse_errors().len(), 1);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::Expr(e) = &func.body.stmts[0].kind else {
        panic!("expected an expression statement")
    };
    let (_, args) = call(e);
    assert_eq!(args.len(), 1);
}

#[test]
fn trailing_comma_after_recovered_comma_is_not_double_reported() {
    // `g(1 2,)` — one missing comma; the recovered `,` before `)` must not
    // trigger a second error.
    assert_eq!(
        error_kinds("fn f() { g(1 2,); }"),
        vec![ParseErrorKind::ExpectedComma]
    );
    assert_eq!(
        error_kinds("fn f(a b,) {}"),
        vec![ParseErrorKind::ExpectedComma]
    );
}

#[test]
fn error_count_is_bounded_per_root_cause() {
    // Twenty independent broken statements produce exactly twenty errors,
    // one per root cause — recovery does not cascade.
    let bad_stmts = format!("fn f() {{ {} }}", "let x = ; ".repeat(20));
    assert_eq!(error_kinds(&bad_stmts).len(), 20);

    // Same at item level.
    let bad_items = "let x = ; ".repeat(20);
    assert_eq!(error_kinds(&bad_items).len(), 20);
}

#[test]
fn recovery_preserves_later_control_flow() {
    let output = parse_src("fn f() { if a { } else if b { let x = ; } let y = 1; }");
    assert_eq!(output.parse_errors().len(), 1);
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    assert_eq!(func.body.stmts.len(), 2);
    let StmtKind::Let(binding) = &func.body.stmts[1].kind else {
        panic!("expected a let statement")
    };
    assert_eq!(binding.name.name, "y");
}

// ---------------------------------------------------------------------------
// Malformed-input corpora
// ---------------------------------------------------------------------------

#[test]
fn expanded_malformed_corpus_never_panics() {
    let corpus = [
        "(((((",
        ")))))",
        "[[[[[",
        "]]]]]",
        "{{{{{",
        "}}}}}",
        "({[",
        ")}]",
        "([)]",
        "{[(])}",
        "fn(",
        "fn{",
        "fn[",
        "fn)",
        "fn]",
        "fn}",
        "let(",
        "let{",
        "let[",
        "=",
        "==",
        "===",
        "+==",
        "a===b",
        "a==b=c",
        "a..",
        "..a",
        "..=a",
        "a..=",
        "..",
        "...",
        ".. ..",
        "fn f(a a",
        "fn f(a,,b)",
        "fn f(,)",
        "fn f(,)",
        "f(",
        "f(,",
        "f(,,)",
        "f(,)",
        "f(a a)",
        "a[",
        "a[]",
        "a[0][",
        "a[0][1",
        "a[",
        "a.b.",
        "a.",
        "f().",
        "a..b.",
        "x?",
        "a?:b",
        "?a",
        "a ? b",
        "fn let const if",
        "return return",
        "break continue",
        "+ - * /",
        "&& || &&",
        "& | ^",
        ";;;;;",
        ",,,,,",
        ":::",
        "::",
        "->",
        "=>",
        "fn f() { ",
        "fn f() { let",
        "fn f() { let x",
        "fn f() { let x = (",
        "fn f() { let x = ((",
        "fn f() { let x = [",
        "fn f() { let x = [0",
        "fn f() { if (",
        "fn f() { if x {",
        "fn f() { for ",
        "fn f() { for i",
        "fn f() { for i in ",
        "fn f(a, b) {",
        "fn f(a, b) { let",
        "fn f() { g((1",
        "fn f() { a[b(1",
        "fn f() { if a { } else if b { } else if c { } else { }",
        "mut x = 1",
        "let mut",
        "const",
        "1 = 2 = 3 = 4",
        "a = b = = c",
        "\u{feff}fn main() {}", // BOM: lexical error, parser must cope
        "fn f() { let s = \"héllo 世界", // unterminated unicode string
    ];
    for src in corpus {
        check_invariants(src);
    }
}

#[test]
fn pseudo_random_malformed_corpus_never_panics() {
    // A second deterministic pseudo-random corpus (different seed from the
    // session-03 suite) over arbitrary byte sequences decoded lossily.
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..1_000 {
        let len = (next() % 96) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        let src = String::from_utf8_lossy(&bytes);
        check_invariants(&src);
    }
}

/// Asserts the parser invariants for `src`: it never panics, every error span
/// and every AST node span is in bounds and non-inverted.
fn check_invariants(src: &str) {
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).expect("added file is present");
    let output = parse(file);
    let text_len = src.len() as u32;

    for error in output.parse_errors() {
        assert_span_ok(error.span(), text_len, src);
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
        TyKind::GenericParam(ident) => walk_ident(ident, text_len, "type"),
        TyKind::NamedApp { name, args } => {
            walk_ident(name, text_len, "type");
            for arg in args {
                walk_ty(arg, text_len);
            }
        }
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

/// Recursively walks an `if` statement, covering arbitrarily deep
/// `else if` chains so the corpus invariant checker validates every node's
/// span.
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
        ExprKind::Call { callee, args, .. } => {
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
// Excluded-syntax regression
// ---------------------------------------------------------------------------

#[test]
fn excluded_declarations_at_top_level_are_rejected() {
    // `enum` declarations are implemented (session 17) and therefore not
    // in this list.
    let excluded = [
        "type X = int;",
        "trait T {}",
        "impl T for U {}",
        "async fn f() {}",
        "unsafe fn f() {}",
        "match x {}",
    ];
    for src in excluded {
        assert_eq!(
            error_kinds(src),
            vec![ParseErrorKind::ExpectedItem],
            "for {src:?}"
        );
    }
}

#[test]
fn excluded_constructs_inside_functions_are_rejected() {
    let excluded: &[(&str, ParseErrorKind)] = &[
        // match statements arrived with session 18 and are accepted; a
        // pattern without `=>` is a dedicated E-P24.
        // Closure.
        ("let g = |x| x + 1;", ParseErrorKind::ExpectedExpression),
        // unsafe block.
        ("unsafe { }", ParseErrorKind::ExpectedExpression),
        // await expression.
        ("await x;", ParseErrorKind::ExpectedExpression),
    ];
    for (src, expected) in excluded {
        assert_eq!(
            error_kinds(&format!("fn f() {{ {src} }}")),
            vec![*expected],
            "for {src:?}"
        );
    }
}

#[test]
fn return_type_annotation_accepted() {
    // `-> Type` after the parameter list is now part of the grammar
    // (session 25 — function signature type annotations).
    // Valid return types parse without error.
    assert!(parse_ok("fn f() -> Int { }"));
    assert!(parse_ok("fn f() -> Bool { }"));
    assert!(parse_ok("fn f() -> Float { }"));
    assert!(parse_ok("fn f() -> Char { }"));
    assert!(parse_ok("fn f() -> Str { }"));
    assert!(parse_ok("fn f() -> Null { }"));
}

#[test]
fn malformed_return_type_is_rejected() {
    // `->` followed by a non-type token is a parse error.
    assert_eq!(
        error_kinds("fn f() -> { }"),
        vec![ParseErrorKind::ExpectedType]
    );
    assert_eq!(
        error_kinds("fn f() -> = { }"),
        vec![ParseErrorKind::ExpectedType]
    );
}

#[test]
fn excluded_tokens_and_operators_are_rejected() {
    let excluded: &[(&str, ParseErrorKind)] = &[
        // `?` optional handling: the expression ends at `?` and the missing
        // `;` is reported at the offending token.
        ("let z = g()?;", ParseErrorKind::ExpectedSemicolon),
        // Fat arrow (match-arm syntax).
        ("let z = a => b;", ParseErrorKind::ExpectedSemicolon),
        // Multi-segment paths: `::` forms enum variant paths (session 17),
        // so a third segment is still rejected.
        ("let p = std::mem::x;", ParseErrorKind::ExpectedSemicolon),
        // Open-ended ranges (both directions).
        ("let r = 0..;", ParseErrorKind::ExpectedExpression),
        ("let r = ..b;", ParseErrorKind::ExpectedExpression),
    ];
    for (src, expected) in excluded {
        assert_eq!(
            error_kinds(&format!("fn f() {{ {src} }}")),
            vec![*expected],
            "for {src:?}"
        );
    }
}

#[test]
fn excluded_keywords_are_never_silently_accepted() {
    // Keywords that remain rejected at both statement and item positions.
    let excluded_both = [
        "enum", "type", "trait", "impl", "match", "async", "await", "unsafe",
    ];
    for kw in excluded_both {
        let at_statement = format!("fn f() {{ {kw} x; }}");
        assert!(
            !parse_src(&at_statement).is_valid(),
            "keyword {kw:?} accepted at statement position"
        );
        let at_item = format!("{kw} x;");
        assert!(
            !parse_src(&at_item).is_valid(),
            "keyword {kw:?} accepted at item position"
        );
    }
    // mod, use, pub are now accepted at item position (session 34)
    // but remain rejected at statement position.
    let item_only = ["mod", "use", "pub"];
    for kw in item_only {
        let at_statement = format!("fn f() {{ {kw} x; }}");
        assert!(
            !parse_src(&at_statement).is_valid(),
            "keyword {kw:?} accepted at statement position"
        );
    }
}

#[test]
fn combined_excluded_program_reports_every_offender() {
    // `let x: int = 1` now parses (type annotations on bindings are
    // accepted since session 26); only the closure remains an error.
    let src = "fn main() { let x: int = 1; let g = |a| a; }";
    assert_eq!(error_kinds(src), vec![ParseErrorKind::ExpectedExpression]);
}

#[test]
fn keywords_cannot_be_member_names() {
    assert_eq!(
        error_kinds("fn f() { a.if; }"),
        vec![ParseErrorKind::ExpectedIdentifier]
    );
    assert_eq!(
        error_kinds("fn f() { a.match; }"),
        vec![ParseErrorKind::ExpectedIdentifier]
    );
}

// ---------------------------------------------------------------------------
// Unicode spans
// ---------------------------------------------------------------------------

#[test]
fn unicode_literals_keep_byte_exact_spans() {
    // fn f() { let s = "héllo 世界"; }
    // `"héllo 世界"` occupies bytes 17..32 (é is 2 bytes, 世/界 are 3 each).
    let src = "fn f() { let s = \"héllo 世界\"; }";
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).unwrap();
    let output = parse(file);
    assert!(output.is_valid());
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::Let(binding) = &func.body.stmts[0].kind else {
        panic!("expected a let statement")
    };
    assert_eq!(binding.init.span.range(), 17..32);
    assert_eq!(file.span_text(binding.init.span), Some("\"héllo 世界\""));

    // Multi-byte char: `'é'` is bytes 17..21.
    let src = "fn f() { let c = 'é'; }";
    let mut map = SourceMap::new();
    let id = map.add("test.mink", src);
    let file = map.get(id).unwrap();
    let output = parse(file);
    assert!(output.is_valid());
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::Let(binding) = &func.body.stmts[0].kind else {
        panic!("expected a let statement")
    };
    assert_eq!(binding.init.span.range(), 17..21);
}

#[test]
fn unicode_comments_do_not_shift_spans() {
    // fn f() { /* 世界 */ let a = 1; }
    // The comment occupies bytes 9..21; `let a = 1;` is bytes 22..32.
    let src = "fn f() { /* 世界 */ let a = 1; }";
    let output = parse_src(src);
    assert!(output.is_valid());
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::Let(binding) = &func.body.stmts[0].kind else {
        panic!("expected a let statement")
    };
    assert_eq!(binding.name.span.range(), 26..27);
    assert_eq!(binding.init.span.range(), 30..31);
    assert_eq!(func.body.stmts[0].span.range(), 22..32);
}

#[test]
fn multi_byte_literals_do_not_corrupt_following_spans() {
    // fn f() { let s = "é"; let t = "x"; }
    // `"é"` is bytes 17..21; `"x"` is bytes 31..34.
    let src = "fn f() { let s = \"é\"; let t = \"x\"; }";
    let output = parse_src(src);
    assert!(output.is_valid());
    let ItemKind::Fn(func) = &output.ast().items()[0].kind else {
        panic!("expected a function item")
    };
    let StmtKind::Let(t_binding) = &func.body.stmts[1].kind else {
        panic!("expected a let statement")
    };
    assert_eq!(t_binding.init.span.range(), 31..34);
}

// ---------------------------------------------------------------------------
// Scale and long chains
// ---------------------------------------------------------------------------

#[test]
fn long_binary_chains_parse_linearly_and_stay_left_associative() {
    let terms = 500;
    let src = (0..terms).map(|_| "a").collect::<Vec<_>>().join(" + ");
    let e = expr(&src);
    // The root is an addition; walking left `terms - 1` times reaches the
    // leftmost leaf, confirming `((a + a) + ... ) + a` (left-associative).
    let mut current = &e;
    for _ in 0..(terms - 1) {
        let (lhs, rhs) = bin(current, BinaryOp::Add);
        ident(rhs, "a");
        current = lhs;
    }
    ident(current, "a");
}

#[test]
fn deep_unary_chains_parse() {
    let depth = 200;
    let src = format!("{}a", "-".repeat(depth));
    let e = expr(&src);
    let mut current = &e;
    for _ in 0..depth {
        current = un(current, UnaryOp::Neg);
    }
    ident(current, "a");
}

#[test]
fn long_postfix_chains_parse() {
    let links = 200;
    let src = format!("a{}", ".b".repeat(links));
    let e = expr(&src);
    let mut current = &e;
    for _ in 0..links {
        let (base, name) = member(current);
        assert_eq!(name, "b");
        current = base;
    }
    ident(current, "a");
}

#[test]
fn long_argument_lists_parse() {
    let count = 200;
    let args = (0..count)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let e = expr(&format!("f({args})"));
    let (_, parsed_args) = call(&e);
    assert_eq!(parsed_args.len(), count);
}
