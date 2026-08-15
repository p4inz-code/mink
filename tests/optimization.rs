//! Integration tests for MIR optimization: constant folding, copy
//! propagation, CFG simplification, unreachable-block elimination, and
//! dead-code elimination — all behavior-preserving, deterministic, and
//! structurally validated before and after every pass. Malformed/mutated
//! programs are rejected with structured errors, never panics.
//!
//! The design is documented in `docs/implementation/OPTIMIZATION_IMPLEMENTATION.md`.

use std::path::Path;

use mink::hir::HirProgram;
use mink::mir::{
    self, BlockId, LocalId, MirConstantKind, MirFn, MirItemKind, MirOperand, MirOperandKind,
    MirProgram, MirRvalue, MirRvalueKind, MirStmtKind, MirTargetKind, MirTerminator,
};
use mink::parser;
use mink::source::{SourceId, SourceMap, Span};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parses, semantically analyzes, type-checks, lowers to HIR, lowers to MIR,
/// validates, and optimizes `src`, asserting every stage is clean.
fn optimize_mir(src: &str) -> (HirProgram, MirProgram) {
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
    let lowered =
        mir::lower(&hir).unwrap_or_else(|errors| panic!("clean HIR must lower: {errors:?}"));
    // The unoptimized program must be valid.
    if let Err(errors) = mir::validate(&lowered) {
        panic!("lowering a clean program must produce valid MIR: {errors:?}");
    }
    let optimized = mir::optimize(&lowered)
        .unwrap_or_else(|errors| panic!("optimizing a clean program must succeed: {errors:?}"));
    if let Err(errors) = mir::validate(&optimized) {
        panic!("optimized MIR must remain valid: {errors:?}");
    }
    (hir, optimized)
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

/// Renders a type id through the MIR's own table.
fn type_name(mir: &MirProgram, ty: mink::typecheck::TypeId) -> String {
    mir.types.display(ty)
}

/// The returned value of `term`, if it is a return.
fn return_value(term: &MirTerminator) -> Option<&Option<MirOperand>> {
    match term {
        MirTerminator::Return { value, .. } => Some(value),
        _ => None,
    }
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

/// Asserts `f`'s blocks are ordered by id with the entry block first.
fn assert_block_ordering(f: &MirFn) {
    for (index, block) in f.blocks.iter().enumerate() {
        assert_eq!(block.id.raw() as usize, index, "block ordering");
    }
    assert_eq!(f.blocks[0].id, f.entry());
}

// ---------------------------------------------------------------------------
// Constant folding
// ---------------------------------------------------------------------------

#[test]
fn boolean_algebra_folds_to_constants() {
    // `true && false` folds to `false`; the copy statement and the temporary
    // are then dead and removed, leaving a constant return.
    let (_hir, mir) = optimize_mir("fn f() { return true && false; }");
    let f = mir_fn(&mir, "f");
    let term = &f.blocks[0].terminator;
    let value = return_value(term).and_then(|v| v.as_ref()).unwrap();
    let MirOperandKind::Constant(constant) = &value.kind else {
        panic!("expected a folded constant, found {:?}", value.kind);
    };
    assert_eq!(constant.kind, MirConstantKind::Bool(false));
    assert_eq!(type_name(&mir, value.ty), "Bool");
}

#[test]
fn logical_not_folds() {
    let (_hir, mir) = optimize_mir("fn f() { return !true; }");
    let f = mir_fn(&mir, "f");
    let value = return_value(&f.blocks[0].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    let MirOperandKind::Constant(constant) = &value.kind else {
        panic!("expected a folded constant");
    };
    assert_eq!(constant.kind, MirConstantKind::Bool(false));
}

#[test]
fn logical_or_and_equality_fold() {
    let (_hir, mir) = optimize_mir(concat!(
        "fn f() { return true || false; } ",
        "fn g() { return true == false; } ",
        "fn h() { return true != false; }",
    ));
    let bools = ["f", "g", "h"]
        .iter()
        .map(|name| {
            let f = mir_fn(&mir, name);
            let value = return_value(&f.blocks[0].terminator)
                .and_then(|v| v.as_ref())
                .unwrap();
            let MirOperandKind::Constant(constant) = &value.kind else {
                panic!("expected a folded constant in {name}");
            };
            match constant.kind {
                MirConstantKind::Bool(b) => b,
                _ => panic!("expected a bool constant in {name}"),
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(bools, [true, false, true]);
}

#[test]
fn folded_constant_keeps_expression_span() {
    let src = "fn f() { return true && false; }";
    let (_hir, mir) = optimize_mir(src);
    let f = mir_fn(&mir, "f");
    let value = return_value(&f.blocks[0].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    let MirOperandKind::Constant(constant) = &value.kind else {
        panic!("expected a folded constant");
    };
    // The folded constant preserves the span of the whole folded expression.
    assert_eq!(constant.span, text_span(src, "true && false"));
    assert_eq!(value.span, text_span(src, "true && false"));
}

#[test]
fn non_foldable_operators_are_preserved() {
    // Arithmetic over literals is not folded (MIR constants carry no
    // decoded values): the binary rvalue and its temporary survive.
    let (_hir, mir) = optimize_mir("fn f() { let x = 1 + 2; return x; }");
    let f = mir_fn(&mir, "f");
    let stmts = &f.blocks[0].stmts;
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    assert!(matches!(
        rvalue.kind,
        MirRvalueKind::Binary {
            op: mink::ast::BinaryOp::Add,
            ..
        }
    ));
}

#[test]
fn folding_does_not_cross_non_constant_operands() {
    // `true && p` must NOT fold (p is not a constant; folding could change
    // evaluation of a side-effecting operand).
    let (_hir, mir) = optimize_mir("fn f(p) { return true && p; }");
    let f = mir_fn(&mir, "f");
    let MirTerminator::Return { value, .. } = &f.blocks[0].terminator else {
        unreachable!()
    };
    let value = value.as_ref().unwrap();
    let MirOperandKind::Local(_) = value.kind else {
        panic!("the non-constant operand must survive as a local load");
    };
}

// ---------------------------------------------------------------------------
// Copy propagation / redundant moves
// ---------------------------------------------------------------------------

#[test]
fn copy_chains_are_propagated() {
    // `let y = x` where x holds a constant: the copy is propagated and the
    // now-dead copy statement is eliminated.
    let (_hir, mir) = optimize_mir("fn f() { let x = true; let y = x; return y; }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks[0].stmts.len(), 0, "the copies must be eliminated");
    let value = return_value(&f.blocks[0].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    let MirOperandKind::Constant(constant) = &value.kind else {
        panic!("expected a propagated constant");
    };
    assert_eq!(constant.kind, MirConstantKind::Bool(true));
}

#[test]
fn propagated_constant_keeps_read_span() {
    let src = "fn f() { let x = true; let y = x; return y; }";
    let (_hir, mir) = optimize_mir(src);
    let f = mir_fn(&mir, "f");
    let value = return_value(&f.blocks[0].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    // The propagated constant keeps the span of the read it replaced: the
    // `y` inside `return y` (the first `y` would match `let y`).
    let ret = text_span(src, "return y");
    let read = Span::new(ret.file(), ret.start() + 7..ret.start() + 8);
    assert_eq!(value.span, read);
}

#[test]
fn copy_into_unused_local_is_eliminated() {
    // `let q = p` where q is never read: a pure dead store, removed.
    let (_hir, mir) = optimize_mir("fn f(p) { let q = p; return; }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks[0].stmts.len(), 0);
    assert_eq!(return_value(&f.blocks[0].terminator), Some(&None));
}

#[test]
fn reassignment_kills_stale_copy() {
    // y copies x (true), then x is reassigned: `return y` must still be
    // `true`, never the reassigned value.
    let (_hir, mir) = optimize_mir("fn f() { let mut x = true; let y = x; x = false; return y; }");
    let f = mir_fn(&mir, "f");
    let value = return_value(&f.blocks[0].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    let MirOperandKind::Constant(constant) = &value.kind else {
        panic!("expected a constant return");
    };
    assert_eq!(constant.kind, MirConstantKind::Bool(true));
}

// ---------------------------------------------------------------------------
// CFG simplification and unreachable blocks
// ---------------------------------------------------------------------------

#[test]
fn constant_true_branch_becomes_jump() {
    let (_hir, mir) = optimize_mir("fn f() { if true { return 1; } else { return 2; } }");
    let f = mir_fn(&mir, "f");
    // Entry: jump to the then-arm; the else-arm (return 2) is unreachable and
    // eliminated. Blocks: entry + then-arm.
    assert_eq!(f.blocks.len(), 2);
    assert_block_ordering(f);
    assert_eq!(jump_target(&f.blocks[0].terminator), Some(BlockId::new(1)));
    let value = return_value(&f.blocks[1].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    let MirOperandKind::Constant(_) = value.kind else {
        panic!("expected the then-arm's return constant");
    };
}

#[test]
fn constant_false_branch_takes_else() {
    let (_hir, mir) = optimize_mir("fn f() { if false { return 1; } else { return 2; } }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 2);
    assert_eq!(jump_target(&f.blocks[0].terminator), Some(BlockId::new(1)));
    let value = return_value(&f.blocks[1].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    let MirOperandKind::Constant(_) = value.kind else {
        panic!("expected the else-arm's return constant");
    };
}

#[test]
fn folded_condition_removes_dead_paths() {
    // `let c = true && true;` folds to a constant, the branch folds to a
    // jump, and the dead else path is eliminated.
    let (_hir, mir) =
        optimize_mir("fn f() { let c = true && true; if c { return 1; } else { return 2; } }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 2);
    assert_block_ordering(f);
    assert_eq!(jump_target(&f.blocks[0].terminator), Some(BlockId::new(1)));
}

#[test]
fn non_constant_conditions_are_preserved() {
    let src = "fn f(c) { if c { return 1; } else { return 2; } }";
    let (_hir, mir) = optimize_mir(src);
    let f = mir_fn(&mir, "f");
    // The branch must survive: `c` is a parameter, not a constant.
    let (then_b, else_b) = branch_targets(&f.blocks[0].terminator).unwrap();
    assert_eq!((then_b, else_b), (BlockId::new(1), BlockId::new(2)));
    assert_eq!(f.blocks.len(), 3);
}

#[test]
fn unreachable_block_after_return_is_removed() {
    // The statement after `return` lives in its own unreachable block which
    // is eliminated.
    let (_hir, mir) = optimize_mir("fn f() { return; let x = 1; }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 1);
    assert_eq!(return_value(&f.blocks[0].terminator), Some(&None));
}

#[test]
fn dead_else_chain_is_removed() {
    // `if true {} else if true {} else {}` — after folding, the entire else
    // chain is unreachable and eliminated, and the empty then-arm is
    // threaded through, leaving entry → join → return.
    let (_hir, mir) = optimize_mir("fn f() { if true { } else if true { } else { } return; }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks.len(), 2);
    assert_block_ordering(f);
    assert_eq!(jump_target(&f.blocks[0].terminator), Some(BlockId::new(1)));
    assert_eq!(return_value(&f.blocks[1].terminator), Some(&None));
}

// ---------------------------------------------------------------------------
// Dead-code elimination
// ---------------------------------------------------------------------------

#[test]
fn unused_binding_is_eliminated() {
    let (_hir, mir) = optimize_mir("fn f() { let x = 1; return; }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks[0].stmts.len(), 0);
    // The local slot is retained (ids stay stable), but the store is gone.
    assert_eq!(f.locals.len(), 1);
    assert_eq!(f.locals[0].name, "x");
}

#[test]
fn calls_are_never_eliminated() {
    // A call with an unused result must survive: it may have side effects.
    let src = "fn g() { return; } fn f() { g(); return; }";
    let (_hir, mir) = optimize_mir(src);
    let f = mir_fn(&mir, "f");
    let stmts = &f.blocks[0].stmts;
    assert_eq!(stmts.len(), 1, "the call statement must survive");
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    assert!(matches!(rvalue.kind, MirRvalueKind::Call { .. }));
}

#[test]
fn range_iteration_machinery_is_never_eliminated() {
    // The loop variable is never read, but RangeNext/RangeFinished carry the
    // loop's observable iteration state: they must survive.
    let (_hir, mir) = optimize_mir("fn f() { for i in 0..10 { } }");
    let f = mir_fn(&mir, "f");
    let mut next = 0;
    let mut finished = 0;
    for block in &f.blocks {
        for stmt in &block.stmts {
            match &stmt.kind {
                MirStmtKind::Assign { rvalue, .. } => match rvalue.kind {
                    MirRvalueKind::RangeNext { .. } => next += 1,
                    MirRvalueKind::RangeFinished { .. } => finished += 1,
                    _ => {}
                },
            }
        }
    }
    assert_eq!(next, 1, "the RangeNext statement must survive");
    assert_eq!(finished, 1, "the RangeFinished statement must survive");
}

#[test]
fn static_assignment_is_never_eliminated() {
    // Writing module storage is observable: the Static-target store must
    // survive even though its value is never read locally.
    let (_hir, mir) = optimize_mir("let mut x = 1; fn f() { x = 2; return; }");
    let f = mir_fn(&mir, "f");
    let stmts = &f.blocks[0].stmts;
    assert_eq!(stmts.len(), 1);
    let MirStmtKind::Assign { target, .. } = &stmts[0].kind;
    assert!(matches!(target.kind, MirTargetKind::Static(_)));
}

#[test]
fn member_and_index_loads_are_never_eliminated() {
    // Member/index rvalues have no defined semantics yet: they are
    // conservatively treated as effectful and must survive.
    let (_hir, mir) = optimize_mir("fn f() { let o = 1; let i = 1; o.f; i[0]; return; }");
    let f = mir_fn(&mir, "f");
    let mut member = 0;
    let mut index = 0;
    for stmt in &f.blocks[0].stmts {
        let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
        match rvalue.kind {
            MirRvalueKind::Member { .. } => member += 1,
            MirRvalueKind::Index { .. } => index += 1,
            _ => {}
        }
    }
    assert_eq!(member, 1);
    assert_eq!(index, 1);
}

#[test]
fn used_bindings_are_kept() {
    let (_hir, mir) = optimize_mir("fn f(p) { let x = p; return x; }");
    let f = mir_fn(&mir, "f");
    // `let x = p` is a redundant move: the read of x is propagated to the
    // parameter and the dead copy is eliminated.
    assert_eq!(f.blocks[0].stmts.len(), 0);
    let value = return_value(&f.blocks[0].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    assert_eq!(value.kind, MirOperandKind::Local(LocalId::new(0)));
}

#[test]
fn compound_assignment_whose_result_is_unread_is_dead() {
    // `x += 1` where x is never read afterwards: the whole computation is
    // dead and is eliminated. This is safe because writing a local slot that
    // is never read is unobservable.
    let (_hir, mir) = optimize_mir("fn f() { let mut x = 1; x += 1; return; }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks[0].stmts.len(), 0);
    assert_eq!(return_value(&f.blocks[0].terminator), Some(&None));
}

#[test]
fn compound_assignment_whose_result_is_read_survives() {
    // The compound-assignment result flows into the return: the binary and
    // the store must survive. The optimizer may propagate the copy so the
    // return reads the binary-result temp instead of `x` itself.
    let (_hir, mir) = optimize_mir("fn f() { let mut x = 1; x += 1; return x; }");
    let f = mir_fn(&mir, "f");
    let binary_survives = f.blocks[0].stmts.iter().any(|s| {
        matches!(
            s.kind,
            MirStmtKind::Assign {
                rvalue: MirRvalue {
                    kind: MirRvalueKind::Binary { .. },
                    ..
                },
                ..
            }
        )
    });
    assert!(binary_survives, "the `+` must survive optimization");
    let value = return_value(&f.blocks[0].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    assert!(matches!(value.kind, MirOperandKind::Local(_)));
}

// ---------------------------------------------------------------------------
// Statics
// ---------------------------------------------------------------------------

#[test]
fn static_initializers_are_folded() {
    let (_hir, mir) = optimize_mir("const c = true && false;");
    let MirItemKind::Const(c) = &mir.items[0].kind else {
        unreachable!()
    };
    // The initializer folds to a constant; no statements remain.
    assert!(c.stmts.is_empty());
    let MirOperandKind::Constant(constant) = &c.value.kind else {
        panic!("expected a folded constant initializer");
    };
    assert_eq!(constant.kind, MirConstantKind::Bool(false));
}

#[test]
fn static_values_are_preserved() {
    // A static's final value is a read: it must never be eliminated.
    let (_hir, mir) = optimize_mir("let base = 1;");
    let MirItemKind::Let(base) = &mir.items[0].kind else {
        unreachable!()
    };
    let MirOperandKind::Constant(_) = base.value.kind else {
        panic!("expected a constant static value");
    };
}

// ---------------------------------------------------------------------------
// Loops and control flow survive
// ---------------------------------------------------------------------------

#[test]
fn loops_are_not_broken_by_optimization() {
    let (_hir, mir) = optimize_mir(concat!(
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
    ));
    let f = mir_fn(&mir, "main");
    assert!(f.blocks.len() > 5, "loops must survive: {}", f.blocks.len());
    assert_block_ordering(f);
    // Every terminator target must remain in bounds.
    for block in &f.blocks {
        match &block.terminator {
            MirTerminator::Jump { target, .. } => {
                assert!(target.raw() < f.blocks.len() as u32);
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
    assert!(mir::validate(&mir).is_ok());
}

#[test]
fn infinite_loop_survives() {
    // `loop {}` lowers to a self-loop; it must not be mistaken for dead code.
    let (_hir, mir) = optimize_mir("fn f() { loop { } }");
    let f = mir_fn(&mir, "f");
    let mut saw_self_loop = false;
    for block in &f.blocks {
        if let MirTerminator::Jump { target, .. } = &block.terminator {
            if *target == block.id {
                saw_self_loop = true;
            }
        }
    }
    assert!(saw_self_loop, "the infinite loop must survive");
    assert!(mir::validate(&mir).is_ok());
}

#[test]
fn while_with_constant_condition_simplifies_safely() {
    // `while true { break; }` folds the header branch to a jump into the
    // body; the body's `break` jumps to the exit, and the empty header and
    // body are threaded through, leaving entry → exit.
    let (_hir, mir) = optimize_mir("fn f() { while true { break; } return; }");
    let f = mir_fn(&mir, "f");
    assert_block_ordering(f);
    assert_eq!(f.blocks.len(), 2);
    assert_eq!(jump_target(&f.blocks[0].terminator), Some(BlockId::new(1)));
    assert_eq!(return_value(&f.blocks[1].terminator), Some(&None));
    assert!(mir::validate(&mir).is_ok());
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn optimization_is_deterministic() {
    let src = concat!(
        "fn main() { ",
        "let a = true && false; ",
        "let mut x = 1; ",
        "if a { x = 2; } else { x = 3; } ",
        "for i in 0..10 { while i > 0 { loop { break; } } } ",
        "return; ",
        "}",
    );
    let (_, first) = optimize_mir(src);
    let (_, second) = optimize_mir(src);
    assert_eq!(first, second);
}

#[test]
fn optimization_reaches_a_fixpoint() {
    let src = concat!(
        "fn main() { ",
        "let c = !(true && false) || false; ",
        "if c { return 1; } else { return 2; } ",
        "}",
    );
    let (_, once) = optimize_mir(src);
    let (_, twice) = optimize_mir(src);
    let optimized_again = mir::optimize(&once).unwrap();
    assert_eq!(once, twice);
    assert_eq!(once, optimized_again, "a fixpoint is reached in one pass");
}

// ---------------------------------------------------------------------------
// Malformed / adversarial input
// ---------------------------------------------------------------------------

/// A hand-corrupted MIR program must fail optimization with structured
/// errors, never a panic.
#[test]
fn malformed_input_returns_structured_errors() {
    let (_hir, mir) = optimize_mir("fn f() { return; }");
    let mut corrupted = mir.clone();
    let MirItemKind::Fn(f) = &mut corrupted.items[0].kind else {
        unreachable!()
    };
    f.blocks[0].terminator = MirTerminator::Jump {
        target: BlockId::new(99),
        span: Span::new(SourceId::new(0), 0..0),
    };
    let errors = mir::optimize(&corrupted).unwrap_err();
    assert!(!errors.is_empty(), "a corrupted program must be rejected");
    assert_eq!(
        errors[0].code(),
        "E-M07",
        "dangling block reference must be reported as E-M07"
    );
}

#[test]
fn dangling_local_reference_is_rejected() {
    let (_hir, mir) = optimize_mir("fn f() { return; }");
    let mut corrupted = mir.clone();
    let MirItemKind::Fn(f) = &mut corrupted.items[0].kind else {
        unreachable!()
    };
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
    let errors = mir::optimize(&corrupted).unwrap_err();
    assert_eq!(errors[0].code(), "E-M08");
}

#[test]
fn many_functions_optimize_deterministically() {
    let mut src = String::from("const base = true;");
    for i in 0..100 {
        src.push_str(&format!(
            "fn f{i}(p) {{ if p {{ return true && false; }} return base; }}"
        ));
    }
    let (_, first) = optimize_mir(&src);
    let (_, second) = optimize_mir(&src);
    assert_eq!(first, second);
    assert!(mir::validate(&first).is_ok());
    // Every function's entry block is intact.
    for i in 0..100 {
        let f = mir_fn(&first, &format!("f{i}"));
        assert!(!f.blocks.is_empty());
        assert_block_ordering(f);
    }
}

#[test]
fn module_item_references_survive() {
    // Statics referenced from functions must never be treated as dead.
    let (_hir, mir) = optimize_mir("const base = true; fn f() { let x = base; return x; }");
    let f = mir_fn(&mir, "f");
    let stmts = &f.blocks[0].stmts;
    assert_eq!(stmts.len(), 1);
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    let MirRvalueKind::Use(operand) = &rvalue.kind else {
        panic!("expected a use rvalue");
    };
    assert!(matches!(operand.kind, MirOperandKind::Static(_)));
    // The static itself is preserved.
    let MirItemKind::Const(c) = &mir.items[0].kind else {
        unreachable!()
    };
    assert_eq!(c.name.name, "base");
}

#[test]
fn parameter_uses_are_preserved() {
    let (_hir, mir) = optimize_mir("fn f(p) { return p; }");
    let f = mir_fn(&mir, "f");
    assert_eq!(f.blocks[0].stmts.len(), 0);
    let value = return_value(&f.blocks[0].terminator)
        .and_then(|v| v.as_ref())
        .unwrap();
    assert_eq!(value.kind, MirOperandKind::Local(LocalId::new(0)));
    // The parameter local is still declared.
    assert_eq!(f.locals[0].name, "p");
}

#[test]
fn empty_program_optimizes() {
    let (_hir, mir) = optimize_mir("");
    assert!(mir.items.is_empty());
}

#[test]
fn symbols_and_types_are_preserved() {
    let (_hir, mir) = optimize_mir(
        "fn add(a, b) { return a + b; } fn main() { let r = add(true && true, false); return r; }",
    );
    // The `add` function keeps its name symbol and fn type.
    let add = mir_fn(&mir, "add");
    assert_eq!(type_name(&mir, add.ty), "fn(Bool, Bool) -> Bool");
    let main = mir_fn(&mir, "main");
    // The `true && true` argument folds; `false` stays a literal.
    let stmts = &main.blocks[0].stmts;
    let MirStmtKind::Assign { rvalue, .. } = &stmts[0].kind;
    let MirRvalueKind::Call { args, .. } = &rvalue.kind else {
        panic!("expected a call rvalue");
    };
    let MirOperandKind::Constant(first) = &args[0].kind else {
        panic!("the folded argument must be a constant");
    };
    assert_eq!(first.kind, MirConstantKind::Bool(true));
    let MirOperandKind::Constant(second) = &args[1].kind else {
        panic!("the literal argument must be a constant");
    };
    assert_eq!(second.kind, MirConstantKind::Bool(false));
}
