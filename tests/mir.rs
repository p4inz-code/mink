//! Integration tests for the MIR layer: lowering the HIR into explicit
//! control flow — functions, locals, statements, terminators, and loops —
//! preserving spans and types, lowering `break`/`continue` to jumps, and
//! validating structural integrity. Malformed/mutated programs are rejected
//! with structured errors, never panics.
//!
//! The design is documented in `docs/implementation/MIR_IMPLEMENTATION.md`.

use std::path::Path;

use mink::hir::{HirItemKind, HirProgram};
use mink::mir::{
    self, BlockId, LocalId, MirConstantKind, MirErrorKind, MirFn, MirItemKind, MirOperand,
    MirOperandKind, MirProgram, MirRvalueKind, MirStmtKind, MirTargetKind, MirTerminator,
};
use mink::parser;
use mink::semantics::SymbolId;
use mink::source::{SourceId, SourceMap, Span};
use mink::typecheck::TypeId;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses, semantically analyzes, type-checks, lowers to HIR, lowers to MIR,
/// and validates `src`, asserting every stage is clean.
fn lower_mir(src: &str) -> (HirProgram, MirProgram) {
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
    let hir = mink::hir::lower(&ast, &semantic, &types)
        .unwrap_or_else(|errors| panic!("clean front end must lower: {errors:?}"));
    let mir = mir::lower(&hir).unwrap_or_else(|errors| panic!("clean HIR must lower: {errors:?}"));
    if let Err(errors) = mir::validate(&mir) {
        panic!("lowering a clean program must produce valid MIR: {errors:?}");
    }
    (hir, mir)
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

/// The symbol of the module-level item named `name`.
fn hir_symbol(hir: &HirProgram, name: &str) -> SymbolId {
    hir.items
        .iter()
        .find_map(|item| match &item.kind {
            HirItemKind::Fn(f) if f.name.name == name => Some(f.name.symbol),
            HirItemKind::Let(b) if b.name.name == name => Some(b.name.symbol),
            HirItemKind::Const(b) if b.name.name == name => Some(b.name.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no module item named `{name}`"))
}

/// Renders a type id through the MIR's own table.
fn type_name(mir: &MirProgram, ty: TypeId) -> String {
    mir.types.display(ty)
}

/// The jump target of `term`, if it is a jump.
fn jump_target(term: &MirTerminator) -> Option<BlockId> {
    match term {
        MirTerminator::Jump { target, .. } => Some(*target),
        _ => None,
    }
}

/// The (then, else) targets of `term`, if it is a branch.
fn branch_targets(term: &MirTerminator) -> Option<(BlockId, BlockId)> {
    match term {
        MirTerminator::Branch {
            then_block,
            else_block,
            ..
        } => Some((*then_block, *else_block)),
        _ => None,
    }
}

/// The returned value of `term`, if it is a return.
fn return_value(term: &MirTerminator) -> Option<&Option<MirOperand>> {
    match term {
        MirTerminator::Return { value, .. } => Some(value),
        _ => None,
    }
}

/// The local-id target of an assignment statement, if any.
fn assign_target(stmt: &mink::mir::MirStmt) -> Option<LocalId> {
    match &stmt.kind {
        MirStmtKind::Assign {
            target:
                mink::mir::MirTarget {
                    kind: MirTargetKind::Local(id),
                    ..
                },
            ..
        } => Some(*id),
        _ => None,
    }
}

/// The statements of the entry block.
fn entry_stmts(f: &MirFn) -> &[mink::mir::MirStmt] {
    &f.blocks[0].stmts
}

/// Asserts `f`'s blocks are ordered by id with the entry block first.
fn assert_block_ordering(f: &MirFn) {
    for (index, block) in f.blocks.iter().enumerate() {
        assert_eq!(block.id.raw() as usize, index, "block ordering");
    }
    assert_eq!(f.blocks[0].id, f.entry());
}

// ---------------------------------------------------------------------------
// Program structure
// ---------------------------------------------------------------------------

#[test]
fn empty_program_lowers() {
    let (_hir, mir) = lower_mir("");
    assert!(mir.items.is_empty());
    // The cloned type table still exists.
    assert!(!mir.types.is_empty());
}

#[test]
fn module_items_lower_in_source_order() {
    let (hir, mir) = lower_mir("fn f() {} let a = 1; const c = 2; fn g() {}");
    let kinds = mir
        .items
        .iter()
        .map(|item| match &item.kind {
            MirItemKind::Fn(f) => format!("fn:{}", f.name.name),
            MirItemKind::Let(b) => format!("let:{}", b.name.name),
            MirItemKind::Const(b) => format!("const:{}", b.name.name),
        })
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["fn:f", "let:a", "const:c", "fn:g"]);
    // The let's initializer is a constant operand; the const's too.
    let MirItemKind::Let(a) = &mir.items[1].kind else {
        unreachable!()
    };
    assert!(matches!(a.value.kind, MirOperandKind::Constant(_)));
    assert_eq!(a.name.symbol, hir_symbol(&hir, "a"));
    let MirItemKind::Const(c) = &mir.items[2].kind else {
        unreachable!()
    };
    assert!(!c.mutable);
    assert_eq!(c.name.symbol, hir_symbol(&hir, "c"));
}

#[test]
fn lowering_is_deterministic() {
    let src = "fn main() { for i in 0..10 { while i > 0 { if i == 3 { break; } continue; } } }";
    let (_hir, first) = lower_mir(src);
    let (_hir, second) = lower_mir(src);
    assert_eq!(first, second);
}

// ---------------------------------------------------------------------------
// Functions, locals, and statements
// ---------------------------------------------------------------------------

#[test]
fn function_lowers_with_params_locals_and_entry() {
    // The call site pins the parameter types to `Int`.
    let (hir, mir) = lower_mir("fn f(a, b) { let x = 1; return; } fn g() { f(1, 2); }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.name.symbol, hir_symbol(&hir, "f"));
    assert_eq!(f.params, [LocalId::new(0), LocalId::new(1)]);
    assert_eq!(f.locals.len(), 3);
    assert_eq!(f.locals[0].name, "a");
    // Parameters carry their declaration's symbol.
    assert!(f.locals[0].symbol.is_some());
    assert!(!f.locals[0].mutable);
    assert_eq!(type_name(&mir, f.locals[0].ty), "Int");
    assert_eq!(f.locals[1].name, "b");
    assert_eq!(f.locals[2].name, "x");
    assert!(!f.locals[2].mutable);
    assert_block_ordering(f);
    assert_eq!(f.blocks.len(), 1);
    let term = &f.blocks[0].terminator;
    assert_eq!(return_value(term), Some(&None));
    // The entry block carries the body's span.
    assert_eq!(
        f.blocks[0].span,
        text_span(
            "fn f(a, b) { let x = 1; return; }",
            "{ let x = 1; return; }"
        )
    );
}

#[test]
fn function_span_and_type_are_preserved() {
    let src = "fn f() { return 1; }";
    let (hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    assert_eq!(f.span, text_span(src, src));
    assert_eq!(type_name(&mir, f.ty), "fn() -> Int");
    assert_eq!(f.name.symbol, hir_symbol(&hir, "f"));
}

#[test]
fn empty_function_falls_off_with_bare_return() {
    let (_hir, mir) = lower_mir("fn f() {}");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 1);
    assert_eq!(return_value(&f.blocks[0].terminator), Some(&None));
}

#[test]
fn returns_lower_with_and_without_value() {
    let src = "fn f() { return 1; } fn g() { return; }";
    let (hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    let term = &f.blocks[0].terminator;
    let value = return_value(term).and_then(|v| v.as_ref()).unwrap();
    assert!(matches!(value.kind, MirOperandKind::Constant(_)));
    assert_eq!(type_name(&mir, value.ty), "Int");
    // The return terminator keeps the statement's span (including the
    // statement's trailing semicolon).
    assert_eq!(term_span(term), text_span(src, "return 1;"));
    assert_eq!(f.name.symbol, hir_symbol(&hir, "f"));
    let g = mir_fn(&mir, "g");
    assert_eq!(return_value(&g.blocks[0].terminator), Some(&None));
}

fn term_span(term: &MirTerminator) -> Span {
    match term {
        MirTerminator::Return { span, .. }
        | MirTerminator::Jump { span, .. }
        | MirTerminator::Branch { span, .. } => *span,
    }
}

#[test]
fn let_bindings_lower_to_locals_and_assignments() {
    let src = "fn f() { let x = 1; let mut y = 2; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    assert_eq!(f.locals.len(), 2);
    assert_eq!(f.locals[0].name, "x");
    assert!(f.locals[0].symbol.is_some());
    assert!(!f.locals[0].mutable);
    assert_eq!(f.locals[1].name, "y");
    assert!(f.locals[1].symbol.is_some());
    assert!(f.locals[1].mutable);
    let stmts = entry_stmts(f);
    assert_eq!(stmts.len(), 2);
    assert_eq!(assign_target(&stmts[0]), Some(LocalId::new(0)));
    assert_eq!(assign_target(&stmts[1]), Some(LocalId::new(1)));
    for (stmt, span) in stmts.iter().zip([
        text_span(src, "let x = 1;"),
        text_span(src, "let mut y = 2;"),
    ]) {
        assert_eq!(stmt.span, span, "statement span");
    }
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    assert!(matches!(
        rvalue.kind,
        MirRvalueKind::Use(MirOperand {
            kind: MirOperandKind::Constant(_),
            ..
        })
    ));
}

#[test]
fn arithmetic_lowers_to_binary_rvalue_with_temporaries() {
    let src = "fn f() { let x = 1 + 2; }";
    let (hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    // Locals: x, then the temporary holding `1 + 2`.
    assert_eq!(f.locals.len(), 2);
    let stmts = entry_stmts(f);
    assert_eq!(stmts.len(), 2);
    let MirStmtKind::Assign { target, rvalue } = &stmts[0].kind;
    assert_eq!(target.kind, MirTargetKind::Local(LocalId::new(1)));
    let MirRvalueKind::Binary { op, lhs, rhs } = &rvalue.kind else {
        panic!("expected a binary rvalue");
    };
    assert_eq!(*op, mink::ast::BinaryOp::Add);
    assert!(matches!(lhs.kind, MirOperandKind::Constant(_)));
    assert!(matches!(rhs.kind, MirOperandKind::Constant(_)));
    assert_eq!(type_name(&mir, rvalue.ty), "Int");
    // The binding copies the temporary.
    let MirStmtKind::Assign { target, rvalue } = &stmts[1].kind;
    assert_eq!(target.kind, MirTargetKind::Local(LocalId::new(0)));
    assert!(matches!(
        rvalue.kind,
        MirRvalueKind::Use(MirOperand {
            kind: MirOperandKind::Local(_),
            ..
        })
    ));
    assert_eq!(f.name.symbol, hir_symbol(&hir, "f"));
}

#[test]
fn unary_operations_lower() {
    let src = "fn f(ok) { let a = -1; let b = !ok; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    // Statements: temp = -1; a = temp; temp = !ok; b = temp.
    assert_eq!(f.locals.len(), 5);
    let stmts = entry_stmts(f);
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    let MirRvalueKind::Unary { op, operand } = &rvalue.kind else {
        panic!("expected a unary rvalue");
    };
    assert_eq!(*op, mink::ast::UnaryOp::Neg);
    assert!(matches!(operand.kind, MirOperandKind::Constant(_)));
    let MirStmtKind::Assign { rvalue, .. } = &stmts[2].kind;
    let MirRvalueKind::Unary { op, operand } = &rvalue.kind else {
        panic!("expected a unary rvalue");
    };
    assert_eq!(*op, mink::ast::UnaryOp::Not);
    assert_eq!(operand.kind, MirOperandKind::Local(LocalId::new(0)));
}

#[test]
fn calls_lower_to_call_rvalue_with_static_callee() {
    let src = "fn f(p) { return p; } fn g() { f(1); }";
    let (hir, mir) = lower_mir(src);
    let g = mir_fn(&mir, "g");
    let stmts = entry_stmts(g);
    assert_eq!(stmts.len(), 1);
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    let MirRvalueKind::Call { callee, args } = &rvalue.kind else {
        panic!("expected a call rvalue");
    };
    assert_eq!(callee.kind, MirOperandKind::Static(hir_symbol(&hir, "f")));
    assert_eq!(args.len(), 1);
    assert!(matches!(args[0].kind, MirOperandKind::Constant(_)));
    assert_eq!(type_name(&mir, rvalue.ty), "Int");
}

#[test]
fn call_result_used_in_declaration() {
    let src = "fn f() { return 1; } fn g() { let x = f(); }";
    let (_hir, mir) = lower_mir(src);
    let g = mir_fn(&mir, "g");
    let stmts = entry_stmts(g);
    assert_eq!(stmts.len(), 2);
    // x copies the call's temporary.
    let MirStmtKind::Assign { target, rvalue } = &stmts[1].kind;
    assert_eq!(target.kind, MirTargetKind::Local(LocalId::new(0)));
    assert!(matches!(
        rvalue.kind,
        MirRvalueKind::Use(MirOperand {
            kind: MirOperandKind::Local(_),
            ..
        })
    ));
    assert_eq!(type_name(&mir, g.locals[0].ty), "Int");
    assert!(g.locals[0].symbol.is_some());
}

#[test]
fn plain_and_compound_assignments_lower() {
    let src = "fn f() { let mut x = 1; x = 2; x += 3; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    let stmts = entry_stmts(f);
    assert_eq!(stmts.len(), 4);
    // x = 2: a plain store.
    let MirStmtKind::Assign { target, rvalue } = &stmts[1].kind;
    assert_eq!(target.kind, MirTargetKind::Local(LocalId::new(0)));
    assert!(matches!(
        rvalue.kind,
        MirRvalueKind::Use(MirOperand {
            kind: MirOperandKind::Constant(_),
            ..
        })
    ));
    // x += 3 desugars to temp = x + 3; x = temp.
    let MirStmtKind::Assign { rvalue, .. } = &stmts[2].kind;
    let MirRvalueKind::Binary { op, lhs, rhs } = &rvalue.kind else {
        panic!("expected the compound-assignment binary");
    };
    assert_eq!(*op, mink::ast::BinaryOp::Add);
    assert_eq!(lhs.kind, MirOperandKind::Local(LocalId::new(0)));
    assert!(matches!(rhs.kind, MirOperandKind::Constant(_)));
    let MirStmtKind::Assign { target, .. } = &stmts[3].kind;
    assert_eq!(target.kind, MirTargetKind::Local(LocalId::new(0)));
}

#[test]
fn ranges_lower_with_inclusive_flag() {
    let src = "fn f() { let a = 0 .. 5; let b = 0 ..= 5; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    let stmts = entry_stmts(f);
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    let MirRvalueKind::Range {
        inclusive,
        start,
        end,
    } = &rvalue.kind
    else {
        panic!("expected a range rvalue");
    };
    assert!(!inclusive);
    assert!(matches!(start.kind, MirOperandKind::Constant(_)));
    assert!(matches!(end.kind, MirOperandKind::Constant(_)));
    assert_eq!(type_name(&mir, rvalue.ty), "Range<Int>");
    let MirStmtKind::Assign { rvalue, .. } = &stmts[2].kind;
    let MirRvalueKind::Range { inclusive, .. } = &rvalue.kind else {
        unreachable!()
    };
    assert!(inclusive);
}

#[test]
fn member_and_index_expressions_lower() {
    // The struct and array literals materialize into temporaries first
    // (statements 0–3), then the member/index expression statements.
    let src = "struct P { f: Int } fn f() { let o = P { f: 1 }; let a = [1, 2]; o.f; a[0]; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    let stmts = entry_stmts(f);
    let MirStmtKind::Assign { rvalue, .. } = &stmts[4].kind;
    let MirRvalueKind::Member { base, member } = &rvalue.kind else {
        panic!("expected a member rvalue");
    };
    assert_eq!(base.kind, MirOperandKind::Local(LocalId::new(0)));
    assert_eq!(member.name, "f");
    let MirStmtKind::Assign { rvalue, .. } = &stmts[5].kind;
    let MirRvalueKind::Index { base, index } = &rvalue.kind else {
        panic!("expected an index rvalue");
    };
    assert_eq!(base.kind, MirOperandKind::Local(LocalId::new(2)));
    assert!(matches!(index.kind, MirOperandKind::Constant(_)));
}

#[test]
fn module_item_references_lower_to_static_operands() {
    let src = "const base = 1; fn f() { return; } fn g() { let x = base; f(); return; }";
    let (hir, mir) = lower_mir(src);
    let g = mir_fn(&mir, "g");
    let stmts = entry_stmts(g);
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    let MirRvalueKind::Use(operand) = &rvalue.kind else {
        panic!("expected a use rvalue");
    };
    assert_eq!(
        operand.kind,
        MirOperandKind::Static(hir_symbol(&hir, "base"))
    );
    let MirStmtKind::Assign { rvalue, .. } = &stmts[1].kind;
    let MirRvalueKind::Call { callee, .. } = &rvalue.kind else {
        unreachable!()
    };
    assert_eq!(callee.kind, MirOperandKind::Static(hir_symbol(&hir, "f")));
}

#[test]
fn module_mutable_let_assignment_lowers_to_static_target() {
    let src = "let mut x = 1; fn f() { x = 2; return; }";
    let (hir, mir) = lower_mir(src);
    let MirItemKind::Let(x) = &mir.items[0].kind else {
        unreachable!()
    };
    assert!(x.mutable);
    assert_eq!(x.name.symbol, hir_symbol(&hir, "x"));
    let f = mir_fn(&mir, "f");
    let MirStmtKind::Assign { target, .. } = &entry_stmts(f)[0].kind;
    assert_eq!(target.kind, MirTargetKind::Static(hir_symbol(&hir, "x")));
}

// ---------------------------------------------------------------------------
// Control flow
// ---------------------------------------------------------------------------

#[test]
fn if_else_lowers_to_branch_and_join() {
    let src = "fn f(c) { if c { let a = 1; } else { let b = 2; } return; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 4);
    assert_block_ordering(f);
    let (then_b, else_b) = branch_targets(&f.blocks[0].terminator).unwrap();
    assert_eq!((then_b, else_b), (BlockId::new(1), BlockId::new(2)));
    let MirTerminator::Branch { cond, .. } = &f.blocks[0].terminator else {
        unreachable!()
    };
    assert_eq!(cond.kind, MirOperandKind::Local(LocalId::new(0)));
    // Then block: `let a = 1`, then join.
    assert_eq!(f.blocks[1].stmts.len(), 1);
    assert_eq!(assign_target(&f.blocks[1].stmts[0]), Some(LocalId::new(1)));
    assert_eq!(jump_target(&f.blocks[1].terminator), Some(BlockId::new(3)));
    // Else block: `let b = 2`, then join.
    assert_eq!(f.blocks[2].stmts.len(), 1);
    assert_eq!(assign_target(&f.blocks[2].stmts[0]), Some(LocalId::new(2)));
    assert_eq!(jump_target(&f.blocks[2].terminator), Some(BlockId::new(3)));
    // Join block: the statement after the if.
    assert_eq!(return_value(&f.blocks[3].terminator), Some(&None));
}

#[test]
fn if_without_else_joins_via_else_block() {
    let src = "fn f(c) { if c { let a = 1; } return; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 4);
    let (then_b, else_b) = branch_targets(&f.blocks[0].terminator).unwrap();
    assert_eq!((then_b, else_b), (BlockId::new(1), BlockId::new(2)));
    // Both arms join at the shared continuation block 3.
    assert_eq!(jump_target(&f.blocks[1].terminator), Some(BlockId::new(3)));
    assert_eq!(jump_target(&f.blocks[2].terminator), Some(BlockId::new(3)));
    // The join block holds the following `return;`.
    assert_eq!(return_value(&f.blocks[3].terminator), Some(&None));
}

#[test]
fn divergent_if_produces_no_dead_join_block() {
    let src = "fn f(c) { if c { return 1; } else { return 2; } }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 3);
    let (then_b, else_b) = branch_targets(&f.blocks[0].terminator).unwrap();
    assert_eq!((then_b, else_b), (BlockId::new(1), BlockId::new(2)));
    let then_value = return_value(&f.blocks[1].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    assert!(matches!(then_value.kind, MirOperandKind::Constant(_)));
    let else_value = return_value(&f.blocks[2].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    assert!(matches!(else_value.kind, MirOperandKind::Constant(_)));
}

#[test]
fn else_if_chain_lowers_to_nested_branches() {
    let src = "fn f(c) { if c { } else if !c { let b = 1; } else { let d = 2; } return; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 6);
    let (then_b, else_b) = branch_targets(&f.blocks[0].terminator).unwrap();
    assert_eq!((then_b, else_b), (BlockId::new(1), BlockId::new(2)));
    // The outer then block jumps to the join.
    assert_eq!(jump_target(&f.blocks[1].terminator), Some(BlockId::new(5)));
    // The nested else-if lives in block 2: its condition is a unary.
    let MirTerminator::Branch { cond, .. } = &f.blocks[2].terminator else {
        panic!("expected a nested branch");
    };
    let MirOperandKind::Local(cond_local) = cond.kind else {
        panic!("expected the nested condition to be a local");
    };
    let MirStmtKind::Assign { rvalue, .. } = &f.blocks[2].stmts[0].kind;
    let MirRvalueKind::Unary { op, .. } = &rvalue.kind else {
        panic!("expected a unary rvalue for `!c`");
    };
    assert_eq!(*op, mink::ast::UnaryOp::Not);
    assert_eq!(cond_local, LocalId::new(1));
    // The two leaf arms both join at block 5.
    assert_eq!(jump_target(&f.blocks[3].terminator), Some(BlockId::new(5)));
    assert_eq!(jump_target(&f.blocks[4].terminator), Some(BlockId::new(5)));
    assert_eq!(return_value(&f.blocks[5].terminator), Some(&None));
}

#[test]
fn while_loop_lowers_to_header_body_and_exit() {
    let src = "fn f() { let mut n = 3; while n > 0 { n = n - 1; } return; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 4);
    assert_block_ordering(f);
    // Entry: initialize n, jump into the header.
    assert_eq!(jump_target(&f.blocks[0].terminator), Some(BlockId::new(1)));
    // Header: test the condition, branch to body or exit.
    let MirTerminator::Branch { cond, .. } = &f.blocks[1].terminator else {
        panic!("expected a branch in the header");
    };
    assert!(matches!(cond.kind, MirOperandKind::Local(_)));
    assert_eq!(
        branch_targets(&f.blocks[1].terminator),
        Some((BlockId::new(2), BlockId::new(3)))
    );
    assert_eq!(
        f.blocks[1].span,
        text_span(src, "while n > 0 { n = n - 1; }")
    );
    // Body: `n = n - 1`, then loop back to the header.
    assert_eq!(f.blocks[2].stmts.len(), 2);
    assert_eq!(jump_target(&f.blocks[2].terminator), Some(BlockId::new(1)));
    // Exit: the statement after the loop.
    assert_eq!(return_value(&f.blocks[3].terminator), Some(&None));
}

#[test]
fn continue_targets_the_loop_header() {
    let src =
        "fn f() { let mut i = 0; while i < 5 { i = i + 1; if i == 2 { continue; } } return; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    // The `continue` inside the body must jump to the header (block 1),
    // skipping the rest of the body.
    let (then_b, _else_b) = branch_targets(&f.blocks[2].terminator).unwrap();
    assert_eq!(
        jump_target(&f.blocks[then_b.raw() as usize].terminator),
        Some(BlockId::new(1)),
        "`continue` must jump to the loop header"
    );
    // The loop exit holds the statement after the loop (a bare return) and
    // is the final block.
    let exit = f.blocks.last().unwrap();
    assert_eq!(return_value(&exit.terminator), Some(&None));
}

#[test]
fn for_loop_lowers_to_range_iteration() {
    let src = "fn f() { for i in 0..10 { i; } }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    // Locals: i (mutable loop variable), the iterable, the done flag. The
    // loop variable is a `for` variable, not a module item; its symbol is
    // present but only meaningful within this function.
    assert_eq!(f.locals.len(), 3);
    assert_eq!(f.locals[0].name, "i");
    assert!(f.locals[0].symbol.is_some());
    assert!(f.locals[0].mutable);
    assert_eq!(type_name(&mir, f.locals[0].ty), "Int");
    // Blocks: entry, init, header, body, exit.
    assert_eq!(f.blocks.len(), 5);
    assert_block_ordering(f);
    // Init: build the range value and jump to the header.
    let MirStmtKind::Assign { rvalue, .. } = &f.blocks[1].stmts[0].kind;
    let MirRvalueKind::Range {
        inclusive,
        start,
        end,
    } = &rvalue.kind
    else {
        panic!("expected a range construction");
    };
    assert!(!inclusive);
    assert!(matches!(start.kind, MirOperandKind::Constant(_)));
    assert!(matches!(end.kind, MirOperandKind::Constant(_)));
    assert_eq!(jump_target(&f.blocks[1].terminator), Some(BlockId::new(2)));
    // Header: completion test, branch to exit (block 4) or body (block 3).
    let MirStmtKind::Assign { rvalue, .. } = &f.blocks[2].stmts[0].kind;
    assert!(matches!(rvalue.kind, MirRvalueKind::RangeFinished { .. }));
    assert_eq!(
        branch_targets(&f.blocks[2].terminator),
        Some((BlockId::new(4), BlockId::new(3)))
    );
    // Body: fetch the next element, then loop back.
    let MirStmtKind::Assign { target, rvalue } = &f.blocks[3].stmts[0].kind;
    assert_eq!(target.kind, MirTargetKind::Local(LocalId::new(0)));
    let MirRvalueKind::RangeNext { .. } = &rvalue.kind else {
        panic!("expected a range-next rvalue");
    };
    assert_eq!(type_name(&mir, rvalue.ty), "Int");
    assert_eq!(jump_target(&f.blocks[3].terminator), Some(BlockId::new(2)));
    // Exit: fall-off-the-end bare return.
    assert_eq!(return_value(&f.blocks[4].terminator), Some(&None));
}

#[test]
fn inclusive_for_loop_preserves_inclusive_range() {
    let src = "fn f() { for i in 0..=5 { } }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    let MirStmtKind::Assign { rvalue, .. } = &f.blocks[1].stmts[0].kind;
    let MirRvalueKind::Range { inclusive, .. } = &rvalue.kind else {
        unreachable!()
    };
    assert!(inclusive);
}

#[test]
fn for_over_range_value_uses_the_local() {
    let src = "fn f() { let r = 0 .. 10; for i in r { i; } return; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    // The iterable is copied from the range-typed local `r` (local 0: the
    // range construction lives in a temporary declared alongside `let r`).
    let MirStmtKind::Assign { rvalue, .. } = &f.blocks[1].stmts[0].kind;
    let MirRvalueKind::Use(operand) = &rvalue.kind else {
        panic!("expected the range value to be copied");
    };
    assert_eq!(operand.kind, MirOperandKind::Local(LocalId::new(0)));
    assert_eq!(type_name(&mir, operand.ty), "Range<Int>");
}

#[test]
fn loop_lowers_with_break_to_exit() {
    let src = "fn f() { loop { break; } }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 3);
    // Entry jumps into the loop; the body `break`s to the exit; the exit
    // falls off the end.
    assert_eq!(jump_target(&f.blocks[0].terminator), Some(BlockId::new(1)));
    assert_eq!(jump_target(&f.blocks[1].terminator), Some(BlockId::new(2)));
    assert_eq!(return_value(&f.blocks[2].terminator), Some(&None));
}

#[test]
fn nested_control_flow_lowers_and_validates() {
    let src = concat!(
        "fn main() { ",
        "for i in 0..10 { ",
        "while i > 0 { ",
        "loop { ",
        "if i == 3 { break; } ",
        "continue; ",
        "} ",
        "} ",
        "} ",
        "return; ",
        "}",
    );
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "main");
    // Deeply nested loops produce a healthy number of blocks, all ordered.
    assert!(f.blocks.len() > 10, "blocks: {}", f.blocks.len());
    assert_block_ordering(f);
    // Every block ends with a terminator by construction; validation passes
    // (already asserted by the helper).
    assert!(mir::validate(&mir).is_ok());
    // The break/continue inside the innermost loop resolve to real blocks.
    let mut break_targets = 0;
    for block in &f.blocks {
        match &block.terminator {
            MirTerminator::Jump { target, .. } => {
                assert!(target.raw() < f.blocks.len() as u32, "dangling jump");
                break_targets += 1;
            }
            MirTerminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                assert!(then_block.raw() < f.blocks.len() as u32);
                assert!(else_block.raw() < f.blocks.len() as u32);
            }
            MirTerminator::Return { .. } => {}
        }
    }
    assert!(break_targets >= 5, "jumps: {break_targets}");
}

#[test]
fn statements_after_return_still_lower() {
    // Dead code after a terminator must lower into an unreachable block
    // without corrupting the CFG.
    let src = "fn f() { return; let x = 1; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    assert!(f.blocks.len() >= 2);
    assert_block_ordering(f);
    let term = &f.blocks[0].terminator;
    assert_eq!(return_value(term), Some(&None));
    // The dead `let` lives in its own block with a valid terminator.
    assert_eq!(f.blocks[1].stmts.len(), 1);
    assert!(mir::validate(&mir).is_ok());
}

// ---------------------------------------------------------------------------
// Spans and types
// ---------------------------------------------------------------------------

#[test]
fn control_flow_spans_are_preserved() {
    let src = "fn f(c) { if c { } return; }";
    let (_hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "f");
    let MirTerminator::Branch { span, .. } = &f.blocks[0].terminator else {
        unreachable!()
    };
    assert_eq!(*span, text_span(src, "if c { }"));
    // Both arms join at block 3, which holds the following `return;`.
    let MirTerminator::Return { span, .. } = &f.blocks[3].terminator else {
        unreachable!()
    };
    assert_eq!(*span, text_span(src, "return;"));
}

#[test]
fn expression_types_match_the_type_checker() {
    // The call site pins the parameters and result to `Int`.
    let src = "fn add(a, b) { return a + b; } fn main() { add(1, 2); }";
    let (hir, mir) = lower_mir(src);
    let f = mir_fn(&mir, "add");
    let stmts = entry_stmts(f);
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    assert_eq!(type_name(&mir, rvalue.ty), "Int");
    assert_eq!(type_name(&mir, f.ty), "fn(Int, Int) -> Int");
    // The return value references the temporary.
    let value = return_value(&f.blocks[0].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    assert_eq!(type_name(&mir, value.ty), "Int");
    assert_eq!(f.name.symbol, hir_symbol(&hir, "add"));
}

// ---------------------------------------------------------------------------
// Robustness
// ---------------------------------------------------------------------------

#[test]
fn many_functions_lower() {
    let mut src = String::from("const base = 1;");
    for i in 0..200 {
        src.push_str(&format!("fn f{i}(p) {{ let v = p + base; return v; }}"));
    }
    let (_hir, mir) = lower_mir(&src);
    assert_eq!(mir.items.len(), 201);
    let fns = mir
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            MirItemKind::Fn(f) => Some(f),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fns.len(), 200);
    // Every function has a properly ordered CFG and at least one block.
    for f in &fns {
        assert!(f.name.name.starts_with('f'));
        assert!(!f.blocks.is_empty());
        assert_block_ordering(f);
    }
}

// ---------------------------------------------------------------------------
// Malformed / mutated input
// ---------------------------------------------------------------------------

/// A hand-corrupted MIR program (dangling jump target) must fail validation
/// with a structured error, never a panic.
#[test]
fn dangling_block_reference_is_a_validation_error() {
    let (_hir, mir) = lower_mir("fn f() { return; }");
    let mut corrupted = mir.clone();
    let MirItemKind::Fn(f) = &mut corrupted.items[0].kind else {
        unreachable!()
    };
    f.blocks[0].terminator = MirTerminator::Jump {
        target: BlockId::new(99),
        span: Span::new(SourceId::new(0), 0..0),
    };
    let errors = mir::validate(&corrupted).unwrap_err();
    assert_eq!(errors.len(), 1, "errors: {errors:?}");
    assert_eq!(errors[0].kind(), MirErrorKind::InvalidBlockReference);
    assert_eq!(errors[0].code(), "E-M07");
    assert!(errors[0].detail().unwrap().contains("99"));
}

/// A hand-corrupted MIR program (dangling local target) must fail
/// validation with a structured error, never a panic.
#[test]
fn dangling_local_reference_is_a_validation_error() {
    let (_hir, mir) = lower_mir("fn f(p) { return; }");
    let mut corrupted = mir.clone();
    let MirItemKind::Fn(f) = &mut corrupted.items[0].kind else {
        unreachable!()
    };
    // The function's own type is a real id in the program's table.
    let ty = f.ty;
    f.blocks[0].stmts.push(mink::mir::MirStmt {
        kind: MirStmtKind::Assign {
            target: mink::mir::MirTarget {
                kind: MirTargetKind::Local(LocalId::new(7)),
                span: Span::new(SourceId::new(0), 0..0),
                ty,
            },
            rvalue: mink::mir::MirRvalue {
                kind: MirRvalueKind::Use(MirOperand {
                    kind: MirOperandKind::Constant(mink::mir::MirConstant {
                        kind: MirConstantKind::Int,
                        span: Span::new(SourceId::new(0), 0..0),
                        ty,
                    }),
                    span: Span::new(SourceId::new(0), 0..0),
                    ty,
                }),
                span: Span::new(SourceId::new(0), 0..0),
                ty,
            },
        },
        span: Span::new(SourceId::new(0), 0..0),
    });
    let errors = mir::validate(&corrupted).unwrap_err();
    assert_eq!(errors.len(), 1, "errors: {errors:?}");
    assert_eq!(errors[0].kind(), MirErrorKind::InvalidLocalReference);
    assert_eq!(errors[0].code(), "E-M08");
}
