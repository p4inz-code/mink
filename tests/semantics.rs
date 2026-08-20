//! Integration tests for semantic analysis: scope construction, symbol
//! collection, name resolution, duplicate detection, shadowing, mutability,
//! control-flow context, semantic diagnostics, and the semantic-result API.
//!
//! The semantic rules under test are documented in
//! `docs/language/CORE_LANGUAGE.md` §24 and
//! `docs/implementation/SEMANTIC_ANALYSIS_IMPLEMENTATION.md`.

use std::path::{Path, PathBuf};

use mink::ast::{
    AssignOp, Ast, Block, ElseBranch, Expr, ExprKind, FnItem, Ident, IfStmt, Item, ItemKind,
    LetItem, Param, Pattern, Stmt, StmtKind,
};
use mink::driver::CheckError;
use mink::parser;
use mink::semantics::{
    ScopeKind, SemanticError, SemanticErrorKind, SemanticResult, Symbol, SymbolId, SymbolKind,
};
use mink::source::{SourceId, SourceMap, Span};

/// The number of predeclared runtime intrinsics (the semantic analyzer
/// binds one symbol per entry of the runtime intrinsic table before any
/// source declaration). Computed from the table so adding an intrinsic
/// never requires recounting this file.
const INTRINSICS: usize = mink::runtime::intrinsics::ALL.len();

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses and semantically analyzes `src`, asserting that it lexes and
/// parses cleanly (semantic tests start from valid syntax).
fn analyze(src: &str) -> (SourceMap, Ast, SemanticResult) {
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
    let result = semantics_analyze(&ast);
    (sources, ast, result)
}

fn semantics_analyze(ast: &Ast) -> SemanticResult {
    mink::semantics::analyze(ast)
}

/// Collects every identifier occurrence in the AST. Declarations (names
/// being bound) are tagged `true`; references are tagged `false`; both are
/// in source order.
fn collect_idents(ast: &Ast) -> Vec<(&str, Span, bool)> {
    let mut out = Vec::new();
    for item in &ast.items {
        match &item.kind {
            ItemKind::Fn(f) => {
                out.push((f.name.name.as_str(), f.name.span, true));
                for param in &f.params {
                    out.push((param.name.name.as_str(), param.name.span, true));
                }
                block_idents(&f.body, &mut out);
            }
            ItemKind::Let(binding) => {
                out.push((binding.name.name.as_str(), binding.name.span, true));
                expr_idents(&binding.init, &mut out);
            }
            ItemKind::Const(binding) => {
                out.push((binding.name.name.as_str(), binding.name.span, true));
                expr_idents(&binding.init, &mut out);
            }
            // Struct and enum names live in the type namespace, not the
            // value namespace: they are not collected as identifiers.
            ItemKind::Struct(_) | ItemKind::Enum(_) => {}
        }
    }
    out
}

fn block_idents<'a>(block: &'a Block, out: &mut Vec<(&'a str, Span, bool)>) {
    for stmt in &block.stmts {
        stmt_idents(stmt, out);
    }
}

fn stmt_idents<'a>(stmt: &'a Stmt, out: &mut Vec<(&'a str, Span, bool)>) {
    match &stmt.kind {
        StmtKind::Let(binding) => {
            out.push((binding.name.name.as_str(), binding.name.span, true));
            expr_idents(&binding.init, out);
        }
        StmtKind::Const(binding) => {
            out.push((binding.name.name.as_str(), binding.name.span, true));
            expr_idents(&binding.init, out);
        }
        StmtKind::Return(Some(value)) => expr_idents(value, out),
        StmtKind::Return(None) | StmtKind::Break(_) | StmtKind::Continue => {}
        StmtKind::If(stmt) => if_idents(stmt, out),
        StmtKind::While { cond, body } => {
            expr_idents(cond, out);
            block_idents(body, out);
        }
        StmtKind::For {
            name,
            iterable,
            body,
        } => {
            out.push((name.name.as_str(), name.span, true));
            expr_idents(iterable, out);
            block_idents(body, out);
        }
        StmtKind::Loop(body) => block_idents(body, out),
        StmtKind::Match(stmt) => {
            expr_idents(&stmt.scrutinee, out);
            for arm in &stmt.arms {
                if let Pattern::Binding(name) = &arm.pattern {
                    out.push((name.name.as_str(), name.span, false));
                }
                block_idents(&arm.body, out);
            }
        }
        StmtKind::Expr(expr) => expr_idents(expr, out),
    }
}

fn if_idents<'a>(stmt: &'a IfStmt, out: &mut Vec<(&'a str, Span, bool)>) {
    expr_idents(&stmt.cond, out);
    block_idents(&stmt.then_block, out);
    match &stmt.else_branch {
        Some(ElseBranch::If(nested)) => if_idents(nested, out),
        Some(ElseBranch::IfExpr(inner)) => {
            expr_idents(&inner.cond, out);
            block_idents(&inner.then_block, out);
        }
        Some(ElseBranch::Block(block)) => block_idents(block, out),
        None => {}
    }
}

fn expr_idents<'a>(expr: &'a Expr, out: &mut Vec<(&'a str, Span, bool)>) {
    match &expr.kind {
        ExprKind::Int
        | ExprKind::Float
        | ExprKind::Str
        | ExprKind::Char
        | ExprKind::Bool(_)
        | ExprKind::Null => {}
        ExprKind::Ident(ident) => out.push((ident.name.as_str(), ident.span, false)),
        ExprKind::Unary { operand, .. } => expr_idents(operand, out),
        ExprKind::Borrow { operand, .. } | ExprKind::Deref { operand } => expr_idents(operand, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            expr_idents(lhs, out);
            expr_idents(rhs, out);
        }
        ExprKind::Assign { target, value, .. } => {
            expr_idents(target, out);
            expr_idents(value, out);
        }
        ExprKind::Range { start, end, .. } => {
            expr_idents(start, out);
            expr_idents(end, out);
        }
        ExprKind::Call { callee, args } => {
            expr_idents(callee, out);
            for arg in args {
                expr_idents(arg, out);
            }
        }
        ExprKind::Member { base, .. } => expr_idents(base, out),
        ExprKind::Index { base, index } => {
            expr_idents(base, out);
            expr_idents(index, out);
        }
        ExprKind::StructLit { name, fields } => {
            for field in fields {
                expr_idents(&field.value, out);
            }
            let _ = name;
        }
        ExprKind::ArrayLit(elems) => {
            for elem in elems {
                expr_idents(elem, out);
            }
        }
        // Enum variant references carry type/variant names, never value
        // names: they are not collected as identifiers.
        ExprKind::EnumVariant { .. } => {}
        ExprKind::Group(inner) => expr_idents(inner, out),
        ExprKind::IfExpr(inner) => {
            expr_idents(&inner.cond, out);
        }
        ExprKind::Block(block) => {
            for stmt in &block.stmts {
                stmt_idents(stmt, out);
            }
            if let Some(result) = &block.result {
                expr_idents(result, out);
            }
        }
        ExprKind::Tuple(elems) => {
            for elem in elems {
                expr_idents(elem, out);
            }
        }
        ExprKind::TupleFieldAccess { base, .. } => expr_idents(base, out),
        ExprKind::WhileExpr { cond, body, .. } => {
            expr_idents(cond, out);
            block_idents(body, out);
        }
        ExprKind::LoopExpr { body, .. } => block_idents(body, out),
    }
}

/// All declaration spans of `name` in the AST, in source order.
fn decl_spans(ast: &Ast, name: &str) -> Vec<Span> {
    collect_idents(ast)
        .into_iter()
        .filter(|(n, _, is_decl)| *is_decl && *n == name)
        .map(|(_, span, _)| span)
        .collect()
}

/// All reference spans of `name` in the AST, in source order.
fn ref_spans(ast: &Ast, name: &str) -> Vec<Span> {
    collect_idents(ast)
        .into_iter()
        .filter(|(n, _, is_decl)| !*is_decl && *n == name)
        .map(|(_, span, _)| span)
        .collect()
}

/// The first declaration span of `name`.
fn decl_span(ast: &Ast, name: &str) -> Span {
    let spans = decl_spans(ast, name);
    assert!(!spans.is_empty(), "no declaration of `{name}` found");
    spans[0]
}

/// The first reference span of `name`.
fn first_ref(ast: &Ast, name: &str) -> Span {
    let spans = ref_spans(ast, name);
    assert!(!spans.is_empty(), "no reference to `{name}` found");
    spans[0]
}

/// The symbol whose declaration is at `span`.
fn symbol_at(result: &SemanticResult, span: Span) -> Symbol {
    result
        .symbols()
        .iter()
        .find(|s| s.span == span)
        .cloned()
        .expect("a symbol declared at this span")
}

/// The first symbol named `name`.
fn symbol(result: &SemanticResult, name: &str) -> Symbol {
    result
        .symbols()
        .iter()
        .find(|s| s.name == name)
        .cloned()
        .expect("a symbol named `{name}`")
}

/// Spans of all errors of `kind`.
fn error_spans(result: &SemanticResult, kind: SemanticErrorKind) -> Vec<Span> {
    result
        .errors()
        .iter()
        .filter(|e| e.kind() == kind)
        .map(SemanticError::span)
        .collect()
}

/// The span of the `needle` text, assuming it appears exactly once. The
/// source file registered by [`analyze`] always has id `0`.
fn text_span(src: &str, needle: &str) -> Span {
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not found"));
    let file = SourceId::new(0);
    Span::new(file, start as u32..start as u32 + needle.len() as u32)
}

/// Writes `content` to a uniquely named temp file for driver tests.
fn temp_source(name: &str, content: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("mink_sem_test_{}_{name}", std::process::id()));
    std::fs::write(&path, content).unwrap();
    path
}

// ---------------------------------------------------------------------------
// Valid programs
// ---------------------------------------------------------------------------

#[test]
fn empty_program_analyzes() {
    let (_sources, _ast, result) = analyze("");
    assert!(!result.has_errors());
    // Only the predeclared runtime intrinsics are present.
    assert_eq!(result.symbols().len(), INTRINSICS);
    assert!(result.resolutions().is_empty());
}

#[test]
fn module_declarations_analyze() {
    let (_sources, _ast, result) = analyze("let x = 1; const y = 2; fn f() {}");
    assert!(!result.has_errors());
    // The three declarations plus the predeclared intrinsics.
    assert_eq!(result.symbols().len(), 3 + INTRINSICS);
    // The module scope holds all three declarations (after the intrinsics).
    let module = result
        .scopes()
        .iter()
        .find(|s| s.kind == ScopeKind::Module)
        .unwrap();
    assert_eq!(module.symbols().len(), 3 + INTRINSICS);
}

#[test]
fn declaration_and_use_resolve() {
    let src = "fn f() { let x = 1; x; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let x = symbol(&result, "x");
    assert_eq!(result.resolve(first_ref(&ast, "x")), Some(x.id));
}

#[test]
fn nested_block_lookup_resolves_outer() {
    // The frozen grammar has no bare `{ ... }` statements; nested scopes
    // come from control-flow bodies like `if`.
    let src = "fn f() { let x = 1; if true { x; } }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let x = symbol(&result, "x");
    assert_eq!(result.resolve(first_ref(&ast, "x")), Some(x.id));
}

#[test]
fn outer_scope_lookup_from_function() {
    let src = "let y = 1; fn f() { y; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let y = symbol(&result, "y");
    assert_eq!(y.kind, SymbolKind::Let { mutable: false });
    // The module symbol is reachable from inside the function.
    assert_eq!(result.resolve(first_ref(&ast, "y")), Some(y.id));
}

#[test]
fn functions_are_callable_in_any_order() {
    let src = "fn a() {} fn b() { a(); }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let a = symbol(&result, "a");
    assert_eq!(a.kind, SymbolKind::Fn);
    assert_eq!(result.resolve(first_ref(&ast, "a")), Some(a.id));
}

#[test]
fn parameter_resolution() {
    let src = "fn f(p) { p; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let p = symbol(&result, "p");
    assert_eq!(p.kind, SymbolKind::Param);
    assert_eq!(result.resolve(first_ref(&ast, "p")), Some(p.id));
}

#[test]
fn local_variable_initializer_resolves_earlier_local() {
    let src = "fn f() { let a = 1; let b = a; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let a = symbol(&result, "a");
    // `b`'s initializer references `a`; the reference resolves to `a`.
    assert_eq!(result.resolve(first_ref(&ast, "a")), Some(a.id));
}

#[test]
fn valid_mutable_assignment() {
    let src = "fn f() { let mut x = 1; x = 2; x += 1; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let x = symbol(&result, "x");
    assert_eq!(x.kind, SymbolKind::Let { mutable: true });
    // Both assignment targets resolve to `x`.
    let refs = ref_spans(&ast, "x");
    assert_eq!(refs.len(), 2);
    for span in refs {
        assert_eq!(result.resolve(span), Some(x.id));
    }
}

#[test]
fn valid_control_flow_context() {
    let src = "fn f() { for i in 0..10 { while i > 0 { loop { break; continue; } } } }";
    let (_sources, _ast, result) = analyze(src);
    assert!(!result.has_errors());
}

#[test]
fn valid_return_context() {
    let src = "fn f() { return; return 1; if true { return; } }";
    let (_sources, _ast, result) = analyze(src);
    assert!(!result.has_errors());
}

#[test]
fn nested_shadowing_is_allowed() {
    let src = "fn f() { let x = 1; if true { let x = 2; x; } }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let decls = decl_spans(&ast, "x");
    assert_eq!(decls.len(), 2);
    let outer = symbol_at(&result, decls[0]);
    let inner = symbol_at(&result, decls[1]);
    assert_ne!(outer.id, inner.id);
    // The reference inside the nested block resolves to the inner
    // declaration.
    assert_eq!(result.resolve(first_ref(&ast, "x")), Some(inner.id));
}

#[test]
fn multiple_independent_scopes() {
    let src = "fn f() { if true { let a = 1; } else { let a = 2; } let b = 3; }";
    let (_sources, _ast, result) = analyze(src);
    assert!(!result.has_errors());
    // f, the two sibling `a` bindings, and `b`, plus the intrinsics.
    assert_eq!(result.symbols().len(), 4 + INTRINSICS);
}

#[test]
fn module_scope_is_order_independent() {
    let src = "fn f() { g(); } let b = a; const a = 1; fn g() { b; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    // Uses before declaration resolve at module scope.
    let a = symbol(&result, "a");
    let b = symbol(&result, "b");
    let g = symbol(&result, "g");
    assert_eq!(result.resolve(first_ref(&ast, "a")), Some(a.id));
    assert_eq!(result.resolve(first_ref(&ast, "b")), Some(b.id));
    assert_eq!(result.resolve(first_ref(&ast, "g")), Some(g.id));
}

#[test]
fn module_binding_visible_in_its_own_initializer() {
    // Module scope is order-independent: a binding is collected before its
    // initializer is analyzed, so `let x = x;` at module scope resolves the
    // initializer reference to the binding itself. (In block scopes the
    // initializer is analyzed before binding, so the same shape refers to an
    // outer `x` or is unresolved — see `let_initializer_does_not_see_itself`.)
    let src = "let x = x;";
    let (_sources, _ast, result) = analyze(src);
    assert!(!result.has_errors());
    assert_eq!(result.symbols().len(), 1 + INTRINSICS); // x plus the intrinsics
    // The initializer reference resolves to the binding itself.
    assert_eq!(result.resolutions().len(), 1);
    let x = symbol(&result, "x");
    assert_eq!(result.resolutions()[0].1, x.id);
}

#[test]
fn for_loop_variable_resolves_in_body() {
    let src = "fn f() { for i in 0..10 { i; } }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let i = symbol(&result, "i");
    assert_eq!(i.kind, SymbolKind::ForVar);
    assert_eq!(result.resolve(first_ref(&ast, "i")), Some(i.id));
}

#[test]
fn for_iterable_cannot_reference_its_own_variable() {
    // The loop variable is not visible in its own iterable expression.
    let src = "fn f() { for x in x { } }";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::UnresolvedName);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], first_ref(&ast, "x"));
}

#[test]
fn for_iterable_sees_outer_scope() {
    let src = "let i = 5; fn f() { for i in i { } }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let module_i = symbol(&result, "i");
    assert_eq!(module_i.kind, SymbolKind::Let { mutable: false });
    // Only the iterable references `i`; it resolves to the module binding.
    let refs = ref_spans(&ast, "i");
    assert_eq!(refs.len(), 1);
    assert_eq!(result.resolve(refs[0]), Some(module_i.id));
}

#[test]
fn member_and_index_assignment_checks_base_writability() {
    // Writing a field/element through a binding requires the binding to be
    // mutable (E-S03), exactly like writing the binding directly; the root
    // base of the chain is what must be writable.
    let src = "struct P { f: Int } fn f() { let mut o = P { f: 1 }; let mut arr = [1, 2]; o.f = 2; arr[0] = 3; }";
    let (_sources, _ast, result) = analyze(src);
    assert!(!result.has_errors());

    let src =
        "struct P { f: Int } fn f() { let o = P { f: 1 }; let arr = [1, 2]; o.f = 2; arr[0] = 3; }";
    let (_sources, _ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::AssignmentToImmutable);
    assert_eq!(spans.len(), 2);
}

#[test]
fn member_names_are_not_resolved_as_scope_names() {
    let src = "fn f() { let o = 1; o.field; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    // `field` is a member selector, not a scope name: it never appears in
    // resolutions and is not reported unresolved. Only `o` resolves.
    assert!(ref_spans(&ast, "field").is_empty());
    assert_eq!(result.resolutions().len(), 1);
}

#[test]
fn index_base_and_index_are_resolved() {
    let src = "fn f() { let a = 1; let i = 0; a[i]; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let a = symbol(&result, "a");
    let i = symbol(&result, "i");
    assert_eq!(result.resolve(first_ref(&ast, "a")), Some(a.id));
    assert_eq!(result.resolve(first_ref(&ast, "i")), Some(i.id));
}

#[test]
fn empty_function_and_empty_blocks() {
    let src = "fn f() { if true {} else {} while false {} for x in 0..1 {} loop {} }";
    let (_sources, _ast, result) = analyze(src);
    assert!(!result.has_errors());
}

#[test]
fn local_can_shadow_module_function() {
    let src = "fn f() { let f = 1; f; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let decls = decl_spans(&ast, "f");
    assert_eq!(decls.len(), 2);
    let local = symbol_at(&result, decls[1]);
    assert_eq!(local.kind, SymbolKind::Let { mutable: false });
    // The reference resolves to the local binding, not the function.
    assert_eq!(result.resolve(first_ref(&ast, "f")), Some(local.id));
}

#[test]
fn function_local_shadows_module_binding() {
    let src = "let x = 1; fn f() { let x = 2; x; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let decls = decl_spans(&ast, "x");
    assert_eq!(decls.len(), 2);
    let local = symbol_at(&result, decls[1]);
    assert_eq!(result.resolve(first_ref(&ast, "x")), Some(local.id));
}

#[test]
fn mutable_shadow_can_be_assigned() {
    let src = "fn f() { let mut x = 1; if true { let mut x = 2; x = 3; } x = 4; }";
    let (_sources, ast, result) = analyze(src);
    assert!(!result.has_errors());
    let decls = decl_spans(&ast, "x");
    let outer = symbol_at(&result, decls[0]);
    let inner = symbol_at(&result, decls[1]);
    let refs = ref_spans(&ast, "x");
    // First target is the inner binding, second is the outer.
    assert_eq!(result.resolve(refs[0]), Some(inner.id));
    assert_eq!(result.resolve(refs[1]), Some(outer.id));
}

#[test]
fn let_initializer_does_not_see_itself() {
    // A binding is not visible in its own initializer; with no outer `x`
    // this is unresolved rather than recursive.
    let src = "fn f() { let x = x; }";
    let (_sources, _ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::UnresolvedName);
    assert_eq!(spans.len(), 1);
}

// ---------------------------------------------------------------------------
// Invalid programs
// ---------------------------------------------------------------------------

#[test]
fn unresolved_identifier_is_reported() {
    let src = "fn f() { missing; }";
    let (_sources, ast, result) = analyze(src);
    assert!(result.has_errors());
    let spans = error_spans(&result, SemanticErrorKind::UnresolvedName);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], first_ref(&ast, "missing"));
    let error = &result.errors()[0];
    assert_eq!(error.name(), "missing");
    assert_eq!(error.code(), "E-S01");
    assert_eq!(error.original(), None);
}

#[test]
fn duplicate_module_bindings_are_rejected() {
    let src = "let x = 1; let x = 2;";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::DuplicateDefinition);
    assert_eq!(spans.len(), 1);
    let decls = decl_spans(&ast, "x");
    assert_eq!(decls.len(), 2);
    // The error points at the duplicate and records the original.
    assert_eq!(spans[0], decls[1]);
    let error = &result.errors()[0];
    assert_eq!(error.code(), "E-S02");
    assert_eq!(error.original(), Some(decls[0]));
}

#[test]
fn duplicate_functions_are_rejected() {
    let src = "fn f() {} fn f() {}";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::DuplicateDefinition);
    assert_eq!(spans.len(), 1);
    let decls = decl_spans(&ast, "f");
    assert_eq!(spans[0], decls[1]);
    assert_eq!(result.errors()[0].original(), Some(decls[0]));
}

#[test]
fn duplicate_enums_are_rejected() {
    // Enums share one type namespace with each other (E-S15); the error
    // points at the duplicate and records the original.
    let src = "enum E { A } enum E { B }";
    let (_sources, _ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::DuplicateEnum);
    assert_eq!(spans.len(), 1);
    assert_eq!(result.errors()[0].code(), "E-S15");
    // The duplicate error points at the second declaration's *name*; the
    // original is the first declaration's name. Enum names are
    // type-namespace symbols, so recover the name positions from the
    // source text.
    let first_name_start = src.find("E { A }").unwrap() as u32;
    let second_name_start = src.find("E { B }").unwrap() as u32;
    assert_eq!(spans[0].start(), second_name_start);
    assert_eq!(
        result.errors()[0].original().unwrap().start(),
        first_name_start
    );
}

#[test]
fn enum_and_struct_share_one_type_namespace() {
    // Struct and enum names share one type namespace: declaring a struct
    // after a same-named enum duplicates the type name (reported with the
    // later declaration's kind — E-S08 for the struct), and vice versa
    // (E-S15 for the enum).
    let src = "enum E { A } struct E { x: Int }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::DuplicateStruct).len(),
        1
    );
    assert_eq!(result.errors()[0].code(), "E-S08");

    let src = "struct E { x: Int } enum E { A }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::DuplicateEnum).len(),
        1
    );
    assert_eq!(result.errors()[0].code(), "E-S15");
}

#[test]
fn duplicate_variants_are_rejected() {
    // Variants are scoped to their enum (E-S16): duplicates within one
    // enum are rejected, but the same variant name in another enum is fine.
    let src = "enum E { A, A } enum F { A }";
    let (_sources, _ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::DuplicateVariant);
    assert_eq!(spans.len(), 1);
    assert_eq!(result.errors()[0].code(), "E-S16");
    // The duplicate points at the second `A`; the original is the first.
    // `text_span` finds the first `A`, whose span matches the recorded
    // original variant identifier span.
    assert_eq!(result.errors()[0].original(), Some(text_span(src, "A")));
}

#[test]
fn function_and_binding_collision_is_rejected() {
    // Functions and bindings share one namespace per scope.
    let src = "fn f() {} let f = 1;";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::DuplicateDefinition).len(),
        1
    );
}

#[test]
fn duplicate_in_block_scope_is_rejected() {
    let src = "fn f() { let a = 1; let a = 2; }";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::DuplicateDefinition);
    assert_eq!(spans.len(), 1);
    let decls = decl_spans(&ast, "a");
    assert_eq!(spans[0], decls[1]);
    assert_eq!(result.errors()[0].original(), Some(decls[0]));
}

#[test]
fn duplicate_parameters_are_rejected() {
    let src = "fn f(a, a) {}";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::DuplicateDefinition);
    assert_eq!(spans.len(), 1);
    let decls = decl_spans(&ast, "a");
    assert_eq!(decls.len(), 2);
    assert_eq!(spans[0], decls[1]);
    assert_eq!(result.errors()[0].original(), Some(decls[0]));
}

#[test]
fn parameter_local_collision_is_rejected() {
    // The function body block is the function's declaration scope, so a
    // parameter and a body binding with the same name collide.
    let src = "fn f(p) { let p = 1; }";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::DuplicateDefinition);
    assert_eq!(spans.len(), 1);
    let decls = decl_spans(&ast, "p");
    assert_eq!(decls.len(), 2);
    assert_eq!(spans[0], decls[1]);
    assert_eq!(result.errors()[0].original(), Some(decls[0]));
}

#[test]
fn immutable_binding_assignment_is_rejected() {
    let src = "fn f() { let x = 1; x = 2; }";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::AssignmentToImmutable);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], first_ref(&ast, "x"));
    assert_eq!(result.errors()[0].name(), "x");
    assert_eq!(result.errors()[0].code(), "E-S03");
}

#[test]
fn const_assignment_is_rejected() {
    let src = "const x = 1; fn f() { x = 2; }";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::AssignmentToConstant);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], first_ref(&ast, "x"));
    assert_eq!(result.errors()[0].code(), "E-S04");
}

#[test]
fn parameter_assignment_is_rejected() {
    let src = "fn f(p) { p = 1; }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::AssignmentToImmutable).len(),
        1
    );
}

#[test]
fn for_variable_assignment_is_rejected() {
    let src = "fn f() { for i in 0..1 { i = 1; } }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::AssignmentToImmutable).len(),
        1
    );
}

#[test]
fn function_name_assignment_is_rejected() {
    let src = "fn f() {} fn g() { f = 1; }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::AssignmentToImmutable).len(),
        1
    );
}

#[test]
fn compound_assignment_to_immutable_is_rejected() {
    let src = "fn f() { let x = 1; x += 1; }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::AssignmentToImmutable).len(),
        1
    );
}

#[test]
fn break_outside_loop_is_rejected() {
    let src = "fn f() { break; }";
    let (_sources, _ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::BreakOutsideLoop);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], text_span(src, "break;"));
    assert_eq!(result.errors()[0].code(), "E-S05");
}

#[test]
fn continue_outside_loop_is_rejected() {
    let src = "fn f() { continue; }";
    let (_sources, _ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::ContinueOutsideLoop);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], text_span(src, "continue;"));
    assert_eq!(result.errors()[0].code(), "E-S06");
}

#[test]
fn break_inside_if_but_not_loop_is_rejected() {
    let src = "fn f() { if true { break; } }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::BreakOutsideLoop).len(),
        1
    );
}

#[test]
fn break_valid_in_loop_but_rejected_after_it() {
    let src = "fn f() { loop { continue; } break; }";
    let (_sources, _ast, result) = analyze(src);
    // Only the `break` after the loop is rejected; the `continue` inside is
    // fine, and `break;` occurs exactly once in this source.
    let spans = error_spans(&result, SemanticErrorKind::BreakOutsideLoop);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], text_span(src, "break;"));
}

#[test]
fn use_before_declaration_in_block_is_unresolved() {
    let src = "fn f() { x; let x = 1; }";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::UnresolvedName);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], first_ref(&ast, "x"));
}

#[test]
fn block_declaration_not_visible_outside_block() {
    let src = "fn f() { if true { let x = 1; } x; }";
    let (_sources, ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::UnresolvedName);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0], first_ref(&ast, "x"));
}

// ---------------------------------------------------------------------------
// Recovery: independent errors are all reported, cascades are avoided
// ---------------------------------------------------------------------------

#[test]
fn multiple_unresolved_names_are_all_reported() {
    let src = "fn f() { alpha; beta; }";
    let (_sources, _ast, result) = analyze(src);
    let spans = error_spans(&result, SemanticErrorKind::UnresolvedName);
    assert_eq!(spans.len(), 2);
}

#[test]
fn duplicate_does_not_cascade_into_unresolved() {
    let src = "fn f() { let x = 1; let x = 2; x; }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::DuplicateDefinition).len(),
        1
    );
    // The reference still resolves to the first declaration.
    assert!(error_spans(&result, SemanticErrorKind::UnresolvedName).is_empty());
}

#[test]
fn duplicate_plus_unresolved_combination() {
    let src = "fn f() { let y = 1; let y = 2; unknown; }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::DuplicateDefinition).len(),
        1
    );
    assert_eq!(
        error_spans(&result, SemanticErrorKind::UnresolvedName).len(),
        1
    );
}

#[test]
fn mutability_plus_unresolved_combination() {
    let src = "fn f() { let x = 1; x = 2; unknown = 3; }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::AssignmentToImmutable).len(),
        1
    );
    assert_eq!(
        error_spans(&result, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    // An unresolved assignment target reports only unresolved, not also a
    // mutability error.
    let errors: Vec<_> = result.errors().iter().map(SemanticError::kind).collect();
    assert_eq!(
        errors,
        vec![
            SemanticErrorKind::AssignmentToImmutable,
            SemanticErrorKind::UnresolvedName
        ]
    );
}

#[test]
fn control_flow_plus_resolution_combination() {
    let src = "fn f() { break; unknown; }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::BreakOutsideLoop).len(),
        1
    );
    assert_eq!(
        error_spans(&result, SemanticErrorKind::UnresolvedName).len(),
        1
    );
}

#[test]
fn errors_in_nested_scopes() {
    let src = "fn f() { if true { missing_a; } missing_b; }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::UnresolvedName).len(),
        2
    );
}

#[test]
fn analysis_continues_after_errors() {
    let src = "fn f() { bad; let ok = 1; ok; }";
    let (_sources, ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::UnresolvedName).len(),
        1
    );
    // Declarations and references after the error are still analyzed.
    let ok = symbol(&result, "ok");
    assert_eq!(result.resolve(first_ref(&ast, "ok")), Some(ok.id));
}

#[test]
fn binding_after_duplicate_still_resolves_to_first() {
    let src = "let x = 1; let x = 2; fn f() { x; }";
    let (_sources, ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::DuplicateDefinition).len(),
        1
    );
    let decls = decl_spans(&ast, "x");
    let first = symbol_at(&result, decls[0]);
    assert_eq!(result.resolve(first_ref(&ast, "x")), Some(first.id));
}

#[test]
fn errors_do_not_stop_module_analysis() {
    let src = "let a = missing1; fn f() { let b = missing2; } let c = 1;";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::UnresolvedName).len(),
        2
    );
    // All three module declarations are still collected (plus intrinsics).
    assert_eq!(result.symbols().len(), 4 + INTRINSICS); // a, f, b, c + intrinsics
}

// ---------------------------------------------------------------------------
// Symbol table and scope behavior
// ---------------------------------------------------------------------------

#[test]
fn symbol_ids_are_stable_unique_and_round_trip() {
    let (_sources, _ast, result) = analyze("let a = 1; let b = 2; fn f() { let c = 3; }");
    let ids: Vec<SymbolId> = result.symbols().iter().map(|s| s.id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "symbol ids must be unique");
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(id.raw() as usize, i, "ids are sequential");
        assert_eq!(result.symbols().get(*id).map(|s| s.id), Some(*id));
    }
}

#[test]
fn symbol_declaration_spans_match_ast() {
    let src = "let x = 1; fn f(p) { let y = 2; }";
    let (_sources, ast, result) = analyze(src);
    for sym in result.symbols().iter() {
        if sym.kind == SymbolKind::Intrinsic {
            continue; // predeclared runtime intrinsics have no source span
        }
        assert_eq!(
            sym.span,
            decl_span(&ast, &sym.name),
            "span for `{}`",
            sym.name
        );
    }
}

#[test]
fn scope_nesting_and_kinds() {
    let src = "let m = 1; fn f() { let x = 1; if true { let y = 2; } }";
    let (_sources, _ast, result) = analyze(src);
    let module = result
        .scopes()
        .iter()
        .find(|s| s.kind == ScopeKind::Module)
        .unwrap();
    assert!(module.parent.is_none());
    let m = symbol(&result, "m");
    assert_eq!(m.scope, module.id);

    let function = result
        .scopes()
        .iter()
        .find(|s| s.kind == ScopeKind::Function)
        .unwrap();
    assert_eq!(function.parent, Some(module.id));
    let x = symbol(&result, "x");
    assert_eq!(x.scope, function.id);

    let block = result
        .scopes()
        .iter()
        .find(|s| s.kind == ScopeKind::Block)
        .unwrap();
    assert_eq!(block.parent, Some(function.id));
    let y = symbol(&result, "y");
    assert_eq!(y.scope, block.id);
}

#[test]
fn scope_declarations_are_listed_per_scope() {
    let src = "let m = 1; fn f(p) { let x = 1; }";
    let (_sources, _ast, result) = analyze(src);
    let module = result
        .scopes()
        .iter()
        .find(|s| s.kind == ScopeKind::Module)
        .unwrap();
    let function = result
        .scopes()
        .iter()
        .find(|s| s.kind == ScopeKind::Function)
        .unwrap();
    let m = symbol(&result, "m");
    let f = symbol(&result, "f");
    let p = symbol(&result, "p");
    let x = symbol(&result, "x");
    // The module scope lists the six predeclared intrinsics before the
    // source declarations.
    let declared: Vec<SymbolId> = module
        .symbols()
        .iter()
        .copied()
        .filter(|id| result.symbols().get(*id).map(|s| s.kind) != Some(SymbolKind::Intrinsic))
        .collect();
    assert_eq!(declared, vec![m.id, f.id]);
    assert_eq!(function.symbols(), &[p.id, x.id]);
    // Direct lookup within a scope finds only its own declarations.
    assert_eq!(module.lookup("m"), Some(m.id));
    assert_eq!(module.lookup("x"), None);
    assert_eq!(function.lookup("p"), Some(p.id));
    assert_eq!(function.lookup("m"), None);
}

#[test]
fn symbol_kinds_are_recorded() {
    let src = "const c = 1; let l = 2; let mut m = 3; fn f(p) { for v in 0..1 { } }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(symbol(&result, "c").kind, SymbolKind::Const);
    assert_eq!(
        symbol(&result, "l").kind,
        SymbolKind::Let { mutable: false }
    );
    assert_eq!(symbol(&result, "m").kind, SymbolKind::Let { mutable: true });
    assert_eq!(symbol(&result, "f").kind, SymbolKind::Fn);
    assert_eq!(symbol(&result, "p").kind, SymbolKind::Param);
    assert_eq!(symbol(&result, "v").kind, SymbolKind::ForVar);
    assert!(symbol(&result, "m").kind.is_mutable());
    assert!(!symbol(&result, "l").kind.is_mutable());
    assert!(!symbol(&result, "c").kind.is_mutable());
}

#[test]
fn resolutions_are_ordered_and_keyed_by_span() {
    let src = "fn f() { let a = 1; let b = a; b; }";
    let (_sources, ast, result) = analyze(src);
    let a = symbol(&result, "a");
    let b = symbol(&result, "b");
    let a_ref = first_ref(&ast, "a");
    let b_refs = ref_spans(&ast, "b");
    assert_eq!(b_refs.len(), 1);
    assert_eq!(result.resolve(a_ref), Some(a.id));
    assert_eq!(result.resolve(b_refs[0]), Some(b.id));
    // References are exposed in span order, one per identifier token.
    assert_eq!(result.resolutions().len(), 2);
    assert!(result.resolutions()[0].0.start() < result.resolutions()[1].0.start());
}

#[test]
fn semantic_error_kind_display_is_stable() {
    let src = "fn f() { missing; let x = 1; x = 2; }";
    let (_sources, _ast, result) = analyze(src);
    let messages: Vec<String> = result.errors().iter().map(|e| e.to_string()).collect();
    assert_eq!(messages[0], "cannot find name `missing` in this scope");
    assert_eq!(messages[1], "cannot assign to `x`: it is not mutable");
}

// ---------------------------------------------------------------------------
// Driver integration
// ---------------------------------------------------------------------------

#[test]
fn driver_check_exposes_semantic_result_for_valid_source() {
    let path = temp_source(
        "driver_valid.mink",
        "let base = 1; fn f() { let x = base; x; }\n",
    );
    let mut sources = SourceMap::new();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(report.errors.is_empty());
    let semantic = report.semantic.expect("semantics ran for valid source");
    assert!(!semantic.has_errors());
    assert_eq!(semantic.symbols().len(), 3 + INTRINSICS); // base, f, x + intrinsics
    // The reference to `base` inside `f` resolves.
    assert!(!semantic.resolutions().is_empty());
}

#[test]
fn driver_check_reports_semantic_errors() {
    let path = temp_source("driver_sem_errors.mink", "fn f() { missing; }\n");
    let mut sources = SourceMap::new();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(!report.errors.is_empty());
    assert!(
        report
            .errors
            .iter()
            .all(|e| matches!(e, CheckError::Semantic(_)))
    );
    assert_eq!(report.errors[0].code(), "E-S01");
    assert!(report.semantic.is_some());
}

#[test]
fn driver_check_skips_semantics_when_parsing_fails() {
    let path = temp_source("driver_parse_error.mink", "fn f() { let x = ; }\n");
    let mut sources = SourceMap::new();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(!report.errors.is_empty());
    assert!(
        report
            .errors
            .iter()
            .all(|e| matches!(e, CheckError::Parse(_)))
    );
    // No semantic or type analysis: no diagnostics, no results.
    assert!(report.semantic.is_none());
    assert!(report.types.is_none());
}

#[test]
fn driver_check_skips_semantics_on_lexical_errors() {
    let path = temp_source("driver_lex_error.mink", "let x = \"unterminated\n");
    let mut sources = SourceMap::new();
    let report = mink::driver::check(&mut sources, &path).unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(report.semantic.is_none());
    assert!(report.types.is_none());
    assert!(
        report
            .errors
            .iter()
            .any(|e| matches!(e, CheckError::Lex(_)))
    );
}

// ---------------------------------------------------------------------------
// Robustness: unusual but structurally valid ASTs must not panic
// ---------------------------------------------------------------------------

#[test]
fn deeply_nested_blocks_do_not_panic() {
    let mut src = String::from("fn f() {");
    for _ in 0..100 {
        src.push_str("if true {");
    }
    src.push_str("loop { break; }");
    for _ in 0..100 {
        src.push('}');
    }
    src.push('}');
    let (_sources, _ast, result) = analyze(&src);
    assert!(!result.has_errors());
}

#[test]
fn many_declarations_and_references_scale() {
    let mut src = String::from("const base = 1;");
    for i in 0..200 {
        src.push_str(&format!("fn f{i}() {{ let v = base; v; }}"));
    }
    let (_sources, _ast, result) = analyze(&src);
    assert!(!result.has_errors());
    // const + 200 functions + 200 locals + the intrinsics.
    assert_eq!(result.symbols().len(), 401 + INTRINSICS);
    // 200 uses of `base` + 200 uses of each `v`.
    assert_eq!(result.resolutions().len(), 400);
}

#[test]
fn long_expression_chain_does_not_panic() {
    let mut expr = String::from("a");
    for _ in 0..300 {
        expr.push_str(" + a");
    }
    let src = format!("let a = 1; fn f() {{ let s = {expr}; }}");
    let (_sources, _ast, result) = analyze(&src);
    assert!(!result.has_errors());
}

#[test]
fn unresolved_in_deep_expression_reports_once() {
    let src = "fn f() { let s = 1 + 2 * missing - 3; }";
    let (_sources, _ast, result) = analyze(src);
    assert_eq!(
        error_spans(&result, SemanticErrorKind::UnresolvedName).len(),
        1
    );
}

/// A manually constructed AST where the assignment target is a literal —
/// the parser rejects this shape syntactically (`E-P04`), but the analyzer
/// must tolerate it without panicking.
#[test]
fn analyzer_tolerates_literal_assignment_target() {
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("weird.mink"), "");
    let file_id = sources.get(id).unwrap().id();
    let span = Span::new(file_id, 0..0);
    let assign = Expr {
        kind: ExprKind::Assign {
            op: AssignOp::Assign,
            target: Box::new(Expr {
                kind: ExprKind::Int,
                span,
            }),
            value: Box::new(Expr {
                kind: ExprKind::Int,
                span,
            }),
        },
        span,
    };
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
                result: None,
                stmts: vec![Stmt {
                    kind: StmtKind::Expr(assign),
                    span,
                }],
                span,
            },
        }),
        span,
    }]);
    let result = semantics_analyze(&ast);
    // The literal target is analyzed as a plain expression: no errors.
    assert!(!result.has_errors());
}

/// An empty AST produces a module scope and no errors.
#[test]
fn empty_ast_analyzes() {
    let result = semantics_analyze(&Ast::new(Vec::new()));
    assert!(!result.has_errors());
    let module = result
        .scopes()
        .iter()
        .find(|s| s.kind == ScopeKind::Module)
        .unwrap();
    assert_eq!(module.id.raw() as usize, 0);
    assert_eq!(module.parent, None);
    assert_eq!(result.scopes().len(), 1);
}

/// Assigning through a `Group` target (also rejected by the parser as a
/// place) resolves the inner identifier without panicking.
#[test]
fn analyzer_tolerates_group_assignment_target() {
    // The parser rejects `(x) = 2` as a non-place target, so that shape
    // cannot reach the analyzer through `analyze`. Build it directly: a
    // Group target is analyzed as a plain expression.
    let mut sources = SourceMap::new();
    let id = sources.add(Path::new("weird2.mink"), "fn f() { let x = 1; }");
    let file_id = sources.get(id).unwrap().id();
    let span = Span::new(file_id, 13..14); // position of `x` in the body text
    let target = Expr {
        kind: ExprKind::Group(Box::new(Expr {
            kind: ExprKind::Ident(Ident {
                name: "x".to_string(),
                span,
            }),
            span,
        })),
        span,
    };
    let assign = Expr {
        kind: ExprKind::Assign {
            op: AssignOp::Assign,
            target: Box::new(target),
            value: Box::new(Expr {
                kind: ExprKind::Int,
                span: Span::new(file_id, 0..0),
            }),
        },
        span,
    };
    let x_binding = Stmt {
        kind: StmtKind::Let(LetItem {
            name: Ident {
                name: "x".to_string(),
                span,
            },
            mutable: false,
            ty: None,
            init: Expr {
                kind: ExprKind::Int,
                span,
            },
        }),
        span,
    };
    let ast = Ast::new(vec![Item {
        kind: ItemKind::Fn(FnItem {
            name: Ident {
                name: "f".to_string(),
                span: Span::new(file_id, 0..1),
            },
            params: Vec::new(),
            return_ty: None,
            body: Block {
                result: None,
                stmts: vec![
                    x_binding,
                    Stmt {
                        kind: StmtKind::Expr(assign),
                        span,
                    },
                ],
                span: Span::new(file_id, 0..0),
            },
        }),
        span: Span::new(file_id, 0..0),
    }]);
    let result = semantics_analyze(&ast);
    // `x` inside the group resolves to the binding (no mutability check is
    // applied to a non-identifier target); no errors are expected.
    assert!(!result.has_errors());
    assert!(result.resolve(span).is_some());
}
