//! Integration tests for the HIR layer: lowering every supported AST
//! construct into typed, symbol-resolved nodes, preserving exact spans and
//! canonical types, eliminating syntax-only groups, representing control
//! flow, and failing structurally (never panicking) on malformed input.
//!
//! The design is documented in `docs/implementation/HIR_IMPLEMENTATION.md`.

use std::path::Path;

use mink::ast::{Ast, Block, Expr, ExprKind, FnItem, Ident, Item, ItemKind, Stmt, StmtKind};
use mink::hir::{
    self, HirElseBranch, HirErrorKind, HirExpr, HirExprKind, HirFn, HirIdent, HirItemKind,
    HirProgram, HirStmt, HirStmtKind,
};
use mink::parser;
use mink::semantics::{SemanticResult, SymbolKind};
use mink::source::{SourceId, SourceMap, Span};
use mink::typecheck::TypeResult;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses, semantically analyzes, type-checks, and lowers `src`, asserting
/// every front-end stage is clean and lowering succeeds.
fn lower_src(src: &str) -> (SourceMap, Ast, SemanticResult, TypeResult, HirProgram) {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("test.mink"), src);
    let file = sources.get(id).unwrap();
    let parsed = parser::parse(file);
    assert!(
        parsed.is_valid(),
        "test source must lex and parse cleanly\nlex errors: {:?}\nparse errors: {:?}",
        parsed.lex_errors(),
        parsed.parse_errors()
    );
    let (ast, _, _) = parsed.into_parts();
    let semantic = mink::semantics::analyze(&ast);
    assert!(
        !semantic.has_errors(),
        "semantic errors: {:?}",
        semantic.errors()
    );
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    assert!(!types.has_errors(), "type errors: {:?}", types.errors());
    let program = mink::hir::lower(&ast, &semantic, &types)
        .unwrap_or_else(|errors| panic!("clean front end must lower: {errors:?}"));
    (sources, ast, semantic, types, program)
}

/// The span of the `needle` text in the source registered as file id `0`.
fn text_span(src: &str, needle: &str) -> Span {
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not found"));
    Span::new(
        SourceId::new(0),
        start as u32..start as u32 + needle.len() as u32,
    )
}

/// The HIR function named `name`.
fn hir_fn<'p>(program: &'p HirProgram, name: &str) -> &'p HirFn {
    program
        .items
        .iter()
        .find_map(|item| match &item.kind {
            HirItemKind::Fn(f) if f.name.name == name => Some(f),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no HIR function named `{name}`"))
}

/// The `let` bindings among `stmts`, in order.
fn stmt_lets(stmts: &[HirStmt]) -> Vec<&mink::hir::HirLet> {
    stmts
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            HirStmtKind::Let(binding) => Some(binding),
            _ => None,
        })
        .collect()
}

/// The symbol id for the first symbol named `name` in the semantic result.
fn symbol_id(semantic: &SemanticResult, name: &str) -> mink::semantics::SymbolId {
    semantic
        .symbols()
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named `{name}`"))
        .id
}

/// Renders a type id through the HIR's own table.
fn type_name(program: &HirProgram, ty: mink::typecheck::TypeId) -> String {
    program.types.display(ty)
}

/// Asserts `expr` is a `Var` referencing the symbol named `name`.
fn expect_var<'p>(program: &'p HirProgram, expr: &'p HirExpr, name: &str) -> &'p HirIdent {
    match &expr.kind {
        HirExprKind::Var(ident) => {
            assert_eq!(ident.name, name, "var name");
            assert_eq!(
                program.types.display(ident.ty),
                program.types.display(expr.ty),
                "var type must match its expression type"
            );
            ident
        }
        other => panic!("expected a Var for `{name}`, found {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Literals
// ---------------------------------------------------------------------------

#[test]
fn literal_expressions_lower_with_types() {
    let src = "fn f() { let a = 1; let b = 2.5; let c = \"s\"; let d = 'x'; let e = true; let n = null; }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let lets = stmt_lets(&hir_fn(&program, "f").body.stmts);
    assert_eq!(lets.len(), 6);
    let expected: [(&str, HirExprKind, &str); 6] = [
        ("a", HirExprKind::Int, "Int"),
        ("b", HirExprKind::Float, "Float"),
        ("c", HirExprKind::Str, "Str"),
        ("d", HirExprKind::Char, "Char"),
        ("e", HirExprKind::Bool(true), "Bool"),
        ("n", HirExprKind::Null, "Null"),
    ];
    for (binding, (name, kind, ty)) in lets.iter().zip(expected) {
        assert_eq!(binding.name.name, name, "binding name");
        assert_eq!(binding.init.kind, kind, "initializer kind of `{name}`");
        assert_eq!(type_name(&program, binding.init.ty), ty);
        assert_eq!(type_name(&program, binding.ty), ty);
    }
}

// ---------------------------------------------------------------------------
// Identifiers and declarations
// ---------------------------------------------------------------------------

#[test]
fn var_references_preserve_symbol_and_type() {
    let src = "let a = 1; fn f() { let b = a; }";
    let (_sources, _ast, semantic, _types, program) = lower_src(src);
    let a_symbol = symbol_id(&semantic, "a");
    let body = &hir_fn(&program, "f").body;
    let lets = stmt_lets(&body.stmts);
    assert_eq!(lets.len(), 1);
    let var = expect_var(&program, &lets[0].init, "a");
    assert_eq!(var.symbol, a_symbol, "Var must reference the exact symbol");
    assert_eq!(type_name(&program, lets[0].ty), "Int");
}

#[test]
fn let_and_const_bindings_lower_with_mutability() {
    let src = "fn f() { let x = 1; let mut y = 2; const z = 3; }";
    let (_sources, _ast, semantic, _types, program) = lower_src(src);
    let stmts = &hir_fn(&program, "f").body.stmts;
    let lets = stmt_lets(stmts);
    assert_eq!(lets.len(), 2);
    assert_eq!(lets[0].name.name, "x");
    assert!(!lets[0].mutable);
    assert_eq!(lets[1].name.name, "y");
    assert!(lets[1].mutable);
    assert_eq!(lets[0].name.symbol, symbol_id(&semantic, "x"));
    assert_eq!(lets[1].name.symbol, symbol_id(&semantic, "y"));
    let consts = stmts
        .iter()
        .filter_map(|stmt| match &stmt.kind {
            HirStmtKind::Const(binding) => Some(binding),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(consts.len(), 1);
    assert_eq!(consts[0].name.name, "z");
    assert_eq!(consts[0].name.symbol, symbol_id(&semantic, "z"));
}

#[test]
fn module_items_lower_in_source_order() {
    let src = "fn f() {} let a = 1; const c = 2; fn g() {}";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let kinds = program
        .items
        .iter()
        .map(|item| match &item.kind {
            HirItemKind::Fn(f) => format!("fn:{}", f.name.name),
            HirItemKind::Let(b) => format!("let:{}", b.name.name),
            HirItemKind::Const(b) => format!("const:{}", b.name.name),
            HirItemKind::Struct(s) => format!("struct:{}", s.name.name),
            HirItemKind::Enum(e) => format!("enum:{}", e.name.name),
        })
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["fn:f", "let:a", "const:c", "fn:g"]);
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

#[test]
fn unary_and_binary_operators_lower() {
    let src = "fn f(a, b, i, ok) { let u = -a; let v = !ok; let w = ~i; let x = a + b; let y = ok && ok; let z = a == b; }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let lets = stmt_lets(&hir_fn(&program, "f").body.stmts);
    // `-a`, `!ok`, `~i`
    for (binding, op) in lets.iter().zip([
        mink::ast::UnaryOp::Neg,
        mink::ast::UnaryOp::Not,
        mink::ast::UnaryOp::BitNot,
    ]) {
        let HirExprKind::Unary {
            op: lowered,
            operand,
        } = &binding.init.kind
        else {
            panic!("expected unary in `{}`", binding.name.name);
        };
        assert_eq!(*lowered, op, "unary operator for `{}`", binding.name.name);
        assert!(matches!(operand.kind, HirExprKind::Var(_)));
    }
    // `a + b`, `ok && ok`, `a == b` (the let bindings after the three
    // unary ones)
    let expected_binaries = [
        ("x", mink::ast::BinaryOp::Add),
        ("y", mink::ast::BinaryOp::And),
        ("z", mink::ast::BinaryOp::Eq),
    ];
    for (binding, (name, op)) in lets.iter().skip(3).zip(expected_binaries) {
        let HirExprKind::Binary {
            op: lowered,
            lhs,
            rhs,
        } = &binding.init.kind
        else {
            panic!("expected binary in `{name}`");
        };
        assert_eq!(*lowered, op, "binary operator for `{name}`");
        assert!(matches!(lhs.kind, HirExprKind::Var(_)));
        assert!(matches!(rhs.kind, HirExprKind::Var(_)));
    }
}

#[test]
fn assignments_lower_with_operator_and_target() {
    let src = "fn f() { let mut x = 1; x = 2; x += 3; x -= 4; }";
    let (_sources, _ast, semantic, _types, program) = lower_src(src);
    let x_symbol = symbol_id(&semantic, "x");
    let exprs = hir_fn(&program, "f")
        .body
        .stmts
        .iter()
        .skip(1)
        .map(|stmt| match &stmt.kind {
            HirStmtKind::Expr(expr) => expr,
            other => panic!("expected expression statement, found {other:?}"),
        })
        .collect::<Vec<_>>();
    let expected_ops = [
        mink::ast::AssignOp::Assign,
        mink::ast::AssignOp::AddAssign,
        mink::ast::AssignOp::SubAssign,
    ];
    for (expr, op) in exprs.iter().zip(expected_ops) {
        let HirExprKind::Assign {
            op: lowered,
            target,
            value,
        } = &expr.kind
        else {
            panic!("expected assignment");
        };
        assert_eq!(*lowered, op, "assignment operator");
        let target_var = expect_var(&program, target, "x");
        assert_eq!(target_var.symbol, x_symbol, "target references x's symbol");
        assert!(matches!(value.kind, HirExprKind::Int), "assigned value");
    }
}

// ---------------------------------------------------------------------------
// Calls
// ---------------------------------------------------------------------------

#[test]
fn calls_lower_with_callee_args_and_result_type() {
    let src = "fn f(p) { return p; } fn g() { f(1); }";
    let (_sources, _ast, semantic, _types, program) = lower_src(src);
    let f_symbol = symbol_id(&semantic, "f");
    let g = hir_fn(&program, "g");
    let HirStmtKind::Expr(call_expr) = &g.body.stmts[0].kind else {
        panic!("expected expression statement");
    };
    let HirExprKind::Call { callee, args } = &call_expr.kind else {
        panic!("expected call");
    };
    let callee_var = expect_var(&program, callee, "f");
    assert_eq!(callee_var.symbol, f_symbol, "callee references f's symbol");
    assert_eq!(type_name(&program, callee_var.ty), "fn(Int) -> Int");
    assert_eq!(args.len(), 1);
    assert!(matches!(args[0].kind, HirExprKind::Int));
    // The call's type is the function's result, resolved to `Int`.
    assert_eq!(type_name(&program, call_expr.ty), "Int");
}

#[test]
fn call_result_type_propagates_into_declarations() {
    let src = "fn f() { return 1; } fn g() { let x = f(); }";
    let (_sources, _ast, semantic, _types, program) = lower_src(src);
    let lets = stmt_lets(&hir_fn(&program, "g").body.stmts);
    assert_eq!(lets.len(), 1);
    assert_eq!(lets[0].name.symbol, symbol_id(&semantic, "x"));
    assert_eq!(type_name(&program, lets[0].ty), "Int");
    assert!(matches!(lets[0].init.kind, HirExprKind::Call { .. }));
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

#[test]
fn functions_lower_with_params_and_body() {
    let src = "fn f(p) { return p; } fn g() { f(1); }";
    let (_sources, _ast, semantic, _types, program) = lower_src(src);
    let f = hir_fn(&program, "f");
    assert_eq!(f.name.symbol, symbol_id(&semantic, "f"));
    assert_eq!(type_name(&program, f.ty), "fn(Int) -> Int");
    assert_eq!(f.params.len(), 1);
    assert_eq!(f.params[0].name.symbol, symbol_id(&semantic, "p"));
    assert_eq!(type_name(&program, f.params[0].ty), "Int");
    let HirStmtKind::Return(Some(value)) = &f.body.stmts[0].kind else {
        panic!("expected a return with a value");
    };
    let var = expect_var(&program, value, "p");
    assert_eq!(var.symbol, symbol_id(&semantic, "p"));
    assert_eq!(type_name(&program, value.ty), "Int");
}

// ---------------------------------------------------------------------------
// Returns and control flow
// ---------------------------------------------------------------------------

#[test]
fn returns_lower_with_and_without_value() {
    let src = "fn f() { return 1; } fn g() { return; }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let f = hir_fn(&program, "f");
    let HirStmtKind::Return(Some(value)) = &f.body.stmts[0].kind else {
        panic!("expected typed return");
    };
    assert!(matches!(value.kind, HirExprKind::Int));
    assert_eq!(type_name(&program, value.ty), "Int");
    let g = hir_fn(&program, "g");
    assert_eq!(g.body.stmts[0].kind, HirStmtKind::Return(None));
}

#[test]
fn if_else_lowers_with_condition_and_branches() {
    let src = "fn f(c) { if c { let a = 1; } else if !c { let b = 2; } else { let d = 3; } }";
    let (_sources, _ast, semantic, _types, program) = lower_src(src);
    let f = hir_fn(&program, "f");
    let HirStmtKind::If(if_stmt) = &f.body.stmts[0].kind else {
        panic!("expected if statement");
    };
    let cond_var = expect_var(&program, &if_stmt.cond, "c");
    assert_eq!(cond_var.symbol, symbol_id(&semantic, "c"));
    assert_eq!(type_name(&program, if_stmt.cond.ty), "Bool");
    let then_lets = stmt_lets(&if_stmt.then_block.stmts);
    assert_eq!(then_lets.len(), 1);
    assert_eq!(then_lets[0].name.name, "a");
    let HirElseBranch::If(else_if) = if_stmt.else_branch.as_ref().unwrap() else {
        panic!("expected else-if branch");
    };
    assert!(matches!(else_if.cond.kind, HirExprKind::Unary { .. }));
    let HirElseBranch::Block(else_block) = else_if.else_branch.as_ref().unwrap() else {
        panic!("expected else block");
    };
    assert_eq!(stmt_lets(&else_block.stmts)[0].name.name, "d");
}

#[test]
fn while_loop_lowers_with_condition_and_body() {
    let src = "fn f() { let mut n = 3; while n > 0 { n = n - 1; } }";
    let (_sources, _ast, semantic, _types, program) = lower_src(src);
    let f = hir_fn(&program, "f");
    let HirStmtKind::While { cond, body } = &f.body.stmts[1].kind else {
        panic!("expected while statement");
    };
    let HirExprKind::Binary { op, lhs, rhs } = &cond.kind else {
        panic!("expected comparison condition");
    };
    assert_eq!(*op, mink::ast::BinaryOp::Gt);
    assert_eq!(
        expect_var(&program, lhs, "n").symbol,
        symbol_id(&semantic, "n")
    );
    assert!(matches!(rhs.kind, HirExprKind::Int));
    assert_eq!(body.stmts.len(), 1);
}

#[test]
fn for_loop_lowers_with_var_iterable_and_body() {
    let src = "fn f() { for i in 0..10 { i; } }";
    let (_sources, _ast, semantic, _types, program) = lower_src(src);
    let f = hir_fn(&program, "f");
    let HirStmtKind::For {
        var,
        iterable,
        body,
    } = &f.body.stmts[0].kind
    else {
        panic!("expected for statement");
    };
    assert_eq!(var.name, "i");
    assert_eq!(var.symbol, symbol_id(&semantic, "i"));
    // The loop variable's symbol is a for-variable.
    let symbol = semantic.symbols().get(var.symbol).unwrap();
    assert_eq!(symbol.kind, SymbolKind::ForVar);
    assert_eq!(type_name(&program, var.ty), "Int");
    assert_eq!(type_name(&program, iterable.ty), "Range<Int>");
    assert_eq!(body.stmts.len(), 1);
}

#[test]
fn loop_break_continue_lower() {
    let src = "fn f() { loop { break; continue; } }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let f = hir_fn(&program, "f");
    let HirStmtKind::Loop(body) = &f.body.stmts[0].kind else {
        panic!("expected loop statement");
    };
    assert_eq!(body.stmts[0].kind, HirStmtKind::Break);
    assert_eq!(body.stmts[1].kind, HirStmtKind::Continue);
}

#[test]
fn nested_control_flow_lowers() {
    let src = "fn f() { let mut n = 3; while n > 0 { for i in 0..n { if i == 0 { continue; } } n = n - 1; } }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let f = hir_fn(&program, "f");
    let HirStmtKind::While {
        body: while_body, ..
    } = &f.body.stmts[1].kind
    else {
        panic!("expected while");
    };
    let HirStmtKind::For { body: for_body, .. } = &while_body.stmts[0].kind else {
        panic!("expected for inside while");
    };
    let HirStmtKind::If(if_stmt) = &for_body.stmts[0].kind else {
        panic!("expected if inside for");
    };
    let HirStmtKind::Continue = &if_stmt.then_block.stmts[0].kind else {
        panic!("expected continue inside if");
    };
    let HirStmtKind::Expr(assign) = &while_body.stmts[1].kind else {
        panic!("expected assignment after the for loop");
    };
    assert!(matches!(assign.kind, HirExprKind::Assign { .. }));
}

// ---------------------------------------------------------------------------
// Ranges
// ---------------------------------------------------------------------------

#[test]
fn ranges_lower_with_inclusive_flag() {
    let src = "fn f() { let a = 0 .. 5; let b = 0 ..= 5; }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let lets = stmt_lets(&hir_fn(&program, "f").body.stmts);
    let HirExprKind::Range {
        inclusive,
        start,
        end,
    } = &lets[0].init.kind
    else {
        panic!("expected range");
    };
    assert!(!inclusive);
    assert!(matches!(start.kind, HirExprKind::Int));
    assert!(matches!(end.kind, HirExprKind::Int));
    assert_eq!(type_name(&program, lets[0].ty), "Range<Int>");
    let HirExprKind::Range { inclusive, .. } = &lets[1].init.kind else {
        panic!("expected range");
    };
    assert!(inclusive);
}

// ---------------------------------------------------------------------------
// Spans
// ---------------------------------------------------------------------------

#[test]
fn expression_spans_are_preserved() {
    let src = "fn f() { let x = 1 + 2; }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let lets = stmt_lets(&hir_fn(&program, "f").body.stmts);
    let init = &lets[0].init;
    assert_eq!(init.span, text_span(src, "1 + 2"));
    let HirExprKind::Binary { lhs, rhs, .. } = &init.kind else {
        panic!("expected binary");
    };
    assert_eq!(lhs.span, text_span(src, "1"));
    assert_eq!(rhs.span, text_span(src, "2"));
}

#[test]
fn group_nodes_are_eliminated() {
    // `(1 + 2)` lowers to its inner binary node, keeping the parentheses'
    // span; no group node remains.
    let src = "fn f() { let x = (1 + 2) * 3; }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let lets = stmt_lets(&hir_fn(&program, "f").body.stmts);
    let init = &lets[0].init;
    assert_eq!(init.span, text_span(src, "(1 + 2) * 3"));
    let HirExprKind::Binary { op, lhs, rhs } = &init.kind else {
        panic!("expected binary multiplication at the top");
    };
    assert_eq!(*op, mink::ast::BinaryOp::Mul);
    assert!(matches!(rhs.kind, HirExprKind::Int));
    let HirExprKind::Binary { op: inner_op, .. } = &lhs.kind else {
        panic!("expected the grouped addition, not a group node");
    };
    assert_eq!(*inner_op, mink::ast::BinaryOp::Add);
    assert_eq!(lhs.span, text_span(src, "(1 + 2)"));
}

#[test]
fn statement_and_block_spans_are_preserved() {
    let src = "fn f() { if true { let x = 1; } }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let f = hir_fn(&program, "f");
    assert_eq!(f.span, text_span(src, "fn f() { if true { let x = 1; } }"));
    assert_eq!(f.body.span, text_span(src, "{ if true { let x = 1; } }"));
    let HirStmtKind::If(if_stmt) = &f.body.stmts[0].kind else {
        panic!("expected if");
    };
    assert_eq!(if_stmt.span, text_span(src, "if true { let x = 1; }"));
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[test]
fn expression_types_match_the_type_checker() {
    let src = "fn f() { let x = 1 + 2 * 3; let y = x > 1; }";
    let (_sources, _ast, _semantic, types, program) = lower_src(src);
    let lets = stmt_lets(&hir_fn(&program, "f").body.stmts);
    let init = &lets[0].init;
    let checker_ty = types.expr_type_exact(text_span(src, "1 + 2 * 3")).unwrap();
    assert_eq!(
        type_name(&program, init.ty),
        types.types().display(checker_ty),
        "HIR type must equal the type checker's recorded type"
    );
    assert_eq!(type_name(&program, init.ty), "Int");
    assert_eq!(type_name(&program, lets[1].ty), "Bool");
}

#[test]
fn hir_program_owns_a_usable_type_table() {
    let src = "fn f() { let x = 0 .. 5; }";
    let (_sources, _ast, semantic, types, program) = lower_src(src);
    let lets = stmt_lets(&hir_fn(&program, "f").body.stmts);
    // The HIR's cloned table renders and canonicalizes types on its own,
    // without the original type result.
    assert_eq!(type_name(&program, lets[0].ty), "Range<Int>");
    let x_ty = types.symbol_type(symbol_id(&semantic, "x")).unwrap();
    assert_eq!(
        program.types.display(program.types.canonical(lets[0].ty)),
        types.types().display(x_ty)
    );
}

// ---------------------------------------------------------------------------
// Member / index
// ---------------------------------------------------------------------------

#[test]
fn member_and_index_nodes_lower() {
    let src = "struct P { f: Int } fn f() { let o = P { f: 1 }; let a = [1, 2]; o.f; a[0]; }";
    let (_sources, _ast, _semantic, _types, program) = lower_src(src);
    let f = hir_fn(&program, "f");
    let HirStmtKind::Expr(member) = &f.body.stmts[2].kind else {
        panic!("expected member expression statement");
    };
    let HirExprKind::Member { base, member } = &member.kind else {
        panic!("expected member access");
    };
    assert_eq!(expect_var(&program, base, "o").name, "o");
    assert_eq!(member.name, "f");
    // The member token `f` of `o.f` (the `f` in `fn f` is a different one).
    let member_start = src.find("o.f").unwrap() + 2;
    assert_eq!(
        member.span,
        Span::new(
            SourceId::new(0),
            member_start as u32..member_start as u32 + 1
        )
    );
    let HirStmtKind::Expr(index) = &f.body.stmts[3].kind else {
        panic!("expected index expression statement");
    };
    let HirExprKind::Index { base, index } = &index.kind else {
        panic!("expected index access");
    };
    assert_eq!(expect_var(&program, base, "a").name, "a");
    assert!(matches!(index.kind, HirExprKind::Int));
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn empty_program_lowers() {
    let (_sources, _ast, _semantic, _types, program) = lower_src("");
    assert!(program.items.is_empty());
    // The cloned type table still exists.
    assert!(!program.types.is_empty());
}

#[test]
fn many_functions_lower() {
    let mut src = String::from("const base = 1;");
    for i in 0..200 {
        src.push_str(&format!("fn f{i}(p) {{ let v = p + base; return v; }}"));
    }
    let (_sources, _ast, _semantic, _types, program) = lower_src(&src);
    assert_eq!(program.items.len(), 201);
    // Every function lowers with a real, non-fallback type and a resolved
    // name; the const item is preserved alongside them.
    let fns = program
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            HirItemKind::Fn(f) => Some(f),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fns.len(), 200);
    // Every function carries a real `Fn` type, never a fallback error type.
    assert!(
        fns.iter()
            .all(|f| f.name.name.starts_with('f') && !program.types.is_error(f.ty))
    );
}

// ---------------------------------------------------------------------------
// Malformed / adversarial input
// ---------------------------------------------------------------------------

/// A hand-built AST with an unresolved identifier reference must fail with
/// a structured lowering error, never a panic.
#[test]
fn unresolved_reference_is_a_lowering_error() {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("bad.mink"), "");
    let file_id = sources.get(id).unwrap().id();
    let mut pos = 0u32;
    let mut next_span = move || {
        let span = Span::new(file_id, pos..pos + 1);
        pos += 1;
        span
    };
    let unresolved_ident_span = next_span();
    let unresolved = Expr {
        kind: ExprKind::Ident(Ident {
            name: "u".to_string(),
            span: unresolved_ident_span,
        }),
        span: next_span(),
    };
    let ast = Ast::new(vec![Item {
        kind: ItemKind::Fn(FnItem {
            name: Ident {
                name: "f".to_string(),
                span: next_span(),
            },
            params: Vec::new(),
            return_ty: None,
            body: Block {
                stmts: vec![Stmt {
                    kind: StmtKind::Expr(unresolved.clone()),
                    span: next_span(),
                }],
                span: next_span(),
            },
        }),
        span: next_span(),
    }]);
    let semantic = mink::semantics::analyze(&ast);
    let types = mink::typecheck::check(&ast, &semantic, &sources);
    let errors = hir::lower(&ast, &semantic, &types).unwrap_err();
    assert_eq!(errors.len(), 1, "errors: {errors:?}");
    assert_eq!(errors[0].kind(), HirErrorKind::UnresolvedSymbol);
    assert_eq!(errors[0].code(), "E-H01");
    // The error points at the identifier token, not the whole expression.
    assert_eq!(errors[0].span(), unresolved_ident_span);
}
