//! MIR optimization passes.
//!
//! Optimization runs on a structurally validated [`MirProgram`] and rewrites
//! it in place through a small, composable pipeline of passes. Every pass is
//! **behavior-preserving**: it only removes or rewrites nodes whose
//! elimination is provably unobservable, and it never changes the semantics
//! the front end established (see `docs/implementation/
//! OPTIMIZATION_IMPLEMENTATION.md`).
//!
//! The passes, in pipeline order:
//!
//! - [`ConstFold`] — folds the boolean algebra over `Bool` constants (the
//!   only constants whose *values* MIR carries; see §2 of the optimization
//!   doc for why folding is deliberately limited to that);
//! - [`CopyProp`] — replaces reads of a local that provably holds a copy of
//!   another value with that value, eliminating redundant moves;
//! - [`CfgSimplify`] — folds branches on constant conditions into jumps,
//!   collapses branches whose targets coincide, and threads jumps through
//!   empty blocks;
//! - [`UnreachableElim`] — removes blocks that are unreachable from the
//!   entry block and renumbers the survivors;
//! - [`DeadCodeElim`] — removes stores to locals that are never read, when
//!   the stored rvalue is provably side-effect-free.
//!
//! [`optimize`] runs the pipeline to a fixpoint, validating the program
//! **before the first pass and after every single pass**. A pass that
//! breaks a structural invariant — or malformed input — is reported as
//! [`MirError`]s (never a panic). All passes are deterministic: iteration
//! follows block and statement order, and no pass's output depends on hash
//! iteration order.

use crate::ast::{BinaryOp, UnaryOp};
use crate::source::Span;
use crate::typecheck::TypeId;

use super::error::MirError;
use super::validate;
use super::{
    BlockId, LocalId, MirBlock, MirConstant, MirConstantKind, MirFn, MirItemKind, MirOperand,
    MirOperandKind, MirProgram, MirRvalue, MirRvalueKind, MirStatic, MirStmt, MirStmtKind,
    MirTargetKind, MirTerminator,
};

/// The maximum number of fixpoint rounds the pipeline runs. Every pass is
/// monotone (it only removes nodes or rewrites them to smaller forms), so a
/// fixpoint is reached quickly; the cap is a defensive bound that guarantees
/// termination on any input.
const MAX_ROUNDS: usize = 64;

/// A single optimization pass over a [`MirProgram`].
///
/// Passes are composable and deterministic: they transform the program in
/// place and report whether anything changed, so a pipeline driver can run
/// them to a fixpoint. Implementations must be behavior-preserving and must
/// leave the program structurally valid (see [`validate`]).
pub trait MirPass {
    /// The pass's stable name, used in reports and tooling.
    fn name(&self) -> &'static str;

    /// Runs this pass over `program`, returning whether it changed anything.
    fn run(&mut self, program: &mut MirProgram) -> bool;
}

/// Runs the standard optimization pipeline over `program` to a fixpoint.
///
/// The input is validated first — malformed MIR is rejected with its
/// structural errors instead of being optimized — and the program is
/// validated **after every pass**, so a pass that breaks an invariant is
/// reported as [`MirError`]s rather than panicking. On success the returned
/// program is structurally valid and semantically equivalent to the input.
pub fn optimize(program: &MirProgram) -> Result<MirProgram, Vec<MirError>> {
    // Refuse to optimize malformed MIR: structured errors, never a panic.
    validate(program)?;
    let mut program = program.clone();
    let mut passes: Vec<Box<dyn MirPass>> = vec![
        Box::new(ConstFold),
        Box::new(CopyProp),
        Box::new(CfgSimplify),
        Box::new(UnreachableElim),
        Box::new(DeadCodeElim),
    ];
    for _ in 0..MAX_ROUNDS {
        let mut changed = false;
        for pass in &mut passes {
            changed |= pass.run(&mut program);
            // Every pass must preserve structural integrity; a pass that
            // breaks it is an internal compiler error, reported never
            // panicked.
            validate(&program)?;
        }
        if !changed {
            break;
        }
    }
    Ok(program)
}

// ---------------------------------------------------------------------------
// Constant folding
// ---------------------------------------------------------------------------

/// Folds the boolean algebra over `Bool` constants.
///
/// `!true → false`, `true && false → false`, `false || true → true`,
/// `true == false → false`, `true != false → true`. The folded result
/// replaces the rvalue with a `Use` of the resulting constant, preserving
/// the rvalue's span and type.
///
/// MIR deliberately does not decode literal values (`Int`, `Float`, `Str`,
/// `Char` carry only their kind and span), so arithmetic and other
/// value-dependent folds are not performed here; `Bool(bool)` is the only
/// constant whose value MIR carries. See
/// `docs/implementation/OPTIMIZATION_IMPLEMENTATION.md` §2.
pub struct ConstFold;

impl MirPass for ConstFold {
    fn name(&self) -> &'static str {
        "const-fold"
    }

    fn run(&mut self, program: &mut MirProgram) -> bool {
        let mut changed = false;
        for item in &mut program.items {
            match &mut item.kind {
                MirItemKind::Fn(f) => {
                    for block in &mut f.blocks {
                        for stmt in &mut block.stmts {
                            changed |= fold_stmt(stmt);
                        }
                    }
                }
                MirItemKind::Let(stat) | MirItemKind::Const(stat) => {
                    for stmt in &mut stat.stmts {
                        changed |= fold_stmt(stmt);
                    }
                }
            }
        }
        changed
    }
}

/// Folds `stmt`'s rvalue if it is a foldable boolean operation.
fn fold_stmt(stmt: &mut MirStmt) -> bool {
    match &mut stmt.kind {
        MirStmtKind::Assign { rvalue, .. } => fold_rvalue(rvalue),
    }
}

/// Folds one rvalue, returning whether it changed.
fn fold_rvalue(rvalue: &mut MirRvalue) -> bool {
    match &mut rvalue.kind {
        MirRvalueKind::Unary {
            op: UnaryOp::Not,
            operand,
        } => {
            if let Some(value) = bool_value(operand) {
                rvalue.kind = MirRvalueKind::Use(bool_operand(!value, rvalue.span, rvalue.ty));
                return true;
            }
            false
        }
        MirRvalueKind::Binary { op, lhs, rhs } => {
            if let (Some(l), Some(r)) = (bool_value(lhs), bool_value(rhs)) {
                let value = match op {
                    BinaryOp::And => l && r,
                    BinaryOp::Or => l || r,
                    BinaryOp::Eq => l == r,
                    BinaryOp::Ne => l != r,
                    // Other operators never type-check on `Bool`; a fold
                    // would fabricate semantics the front end rejected.
                    _ => return false,
                };
                rvalue.kind = MirRvalueKind::Use(bool_operand(value, rvalue.span, rvalue.ty));
                return true;
            }
            false
        }
        _ => false,
    }
}

/// The value of `operand` when it is a `Bool` constant.
fn bool_value(operand: &MirOperand) -> Option<bool> {
    match &operand.kind {
        MirOperandKind::Constant(constant) => match constant.kind {
            MirConstantKind::Bool(value) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

/// A `Bool` constant operand with the given span and type.
fn bool_operand(value: bool, span: Span, ty: TypeId) -> MirOperand {
    MirOperand {
        kind: MirOperandKind::Constant(MirConstant {
            kind: MirConstantKind::Bool(value),
            span,
            ty,
        }),
        span,
        ty,
    }
}

// ---------------------------------------------------------------------------
// Copy propagation (redundant move/copy elimination)
// ---------------------------------------------------------------------------

/// Replaces reads of a local that provably holds a copy of another value
/// with that value.
///
/// Within each block, a statement `t = Use(x)` where `x` is a local or a
/// constant records that `t` holds `x`'s value *until either is reassigned*.
/// Later reads of `t` in the same block are rewritten to read `x` directly
/// (preserving the read's span and type), after which the copy statement
/// itself usually becomes dead and is removed by [`DeadCodeElim`].
///
/// Soundness is maintained with conservative kills: reassigning a local
/// invalidates every recorded copy *of* that local (their value is the old
/// one), reassigning the target invalidates its own record, and a
/// `Member`/`Index`/`Static` target — whose place semantics or reachability
/// the optimizer cannot prove — clears all records. Calls do not clear
/// local records because this language has no pointers or references: a
/// callee cannot observe or modify another function's locals.
pub struct CopyProp;

impl MirPass for CopyProp {
    fn name(&self) -> &'static str {
        "copy-propagation"
    }

    fn run(&mut self, program: &mut MirProgram) -> bool {
        let mut changed = false;
        for item in &mut program.items {
            match &mut item.kind {
                MirItemKind::Fn(f) => {
                    for block in &mut f.blocks {
                        changed |= propagate_block(&mut block.stmts, &mut block.terminator);
                    }
                }
                MirItemKind::Let(stat) | MirItemKind::Const(stat) => {
                    changed |= propagate_static(stat);
                }
            }
        }
        changed
    }
}

/// Performs copy propagation over one block's statements and terminator.
fn propagate_block(stmts: &mut [MirStmt], terminator: &mut MirTerminator) -> bool {
    // `known[i]` is the value local `i` currently provably holds, if any.
    let mut known: Vec<Option<MirOperand>> = Vec::new();
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        changed |= propagate_stmt(stmt, &mut known);
    }
    changed |= rewrite_terminator(terminator, &known);
    changed
}

/// Performs copy propagation over a module static's statements and final
/// value operand.
fn propagate_static(stat: &mut MirStatic) -> bool {
    let mut known: Vec<Option<MirOperand>> = Vec::new();
    let mut changed = false;
    for stmt in stat.stmts.iter_mut() {
        changed |= propagate_stmt(stmt, &mut known);
    }
    changed |= rewrite_operand(&mut stat.value, &known);
    changed
}

/// Applies the copy-propagation rules to one statement.
fn propagate_stmt(stmt: &mut MirStmt, known: &mut Vec<Option<MirOperand>>) -> bool {
    let MirStmtKind::Assign { target, rvalue } = &mut stmt.kind;
    let mut changed = false;
    let target_local = match &target.kind {
        MirTargetKind::Local(id) => Some(*id),
        // An opaque write: the target's place semantics are not defined, so
        // no recorded copy can be trusted afterwards.
        MirTargetKind::Static(_) | MirTargetKind::Member { .. } | MirTargetKind::Index { .. } => {
            known.iter_mut().for_each(|slot| *slot = None);
            None
        }
    };
    if let Some(id) = target_local {
        // The target is being (re)defined: any copy *of* it is stale (it
        // now holds a new value), and its own record is replaced below.
        kill_copies_of(known, id);
    }
    // Rewrite reads in the rvalue.
    changed |= rewrite_rvalue(rvalue, known);
    // Rewrite reads in member/index target bases and indices.
    if let MirTargetKind::Member { base, .. } = &mut target.kind {
        changed |= rewrite_operand(base, known);
    } else if let MirTargetKind::Index { base, index } = &mut target.kind {
        changed |= rewrite_operand(base, known);
        changed |= rewrite_operand(index, known);
    }
    // Record the new copy when this statement is one.
    if let Some(id) = target_local {
        let copy = match &rvalue.kind {
            MirRvalueKind::Use(operand)
                if matches!(
                    operand.kind,
                    MirOperandKind::Local(_) | MirOperandKind::Constant(_)
                ) =>
            {
                Some(operand.clone())
            }
            _ => None,
        };
        let slot = known.get_mut(id.raw() as usize);
        match slot {
            Some(slot) => *slot = copy,
            None => {
                // Out-of-bounds local id (defensive; validation rejects
                // these): extend the table so later reads stay guarded.
                let missing = id.raw() as usize + 1 - known.len();
                known.resize(known.len() + missing, None);
                if let Some(slot) = known.get_mut(id.raw() as usize) {
                    *slot = copy;
                }
            }
        }
    }
    changed
}

/// Removes every recorded copy whose source is `local` (the local was
/// reassigned, so copies of its old value are no longer expressible as
/// reads of it).
fn kill_copies_of(known: &mut [Option<MirOperand>], local: LocalId) {
    for slot in known.iter_mut() {
        if matches!(
            slot,
            Some(MirOperand {
                kind: MirOperandKind::Local(id),
                ..
            }) if *id == local
        ) {
            *slot = None;
        }
    }
}

/// Rewrites reads inside `rvalue` using `known`, returning whether anything
/// changed.
fn rewrite_rvalue(rvalue: &mut MirRvalue, known: &[Option<MirOperand>]) -> bool {
    let mut changed = false;
    match &mut rvalue.kind {
        MirRvalueKind::Use(op)
        | MirRvalueKind::Unary { operand: op, .. }
        | MirRvalueKind::RangeNext { range: op }
        | MirRvalueKind::RangeFinished { range: op }
        | MirRvalueKind::Member { base: op, .. } => changed |= rewrite_operand(op, known),
        MirRvalueKind::Binary { lhs, rhs, .. } => {
            changed |= rewrite_operand(lhs, known);
            changed |= rewrite_operand(rhs, known);
        }
        MirRvalueKind::Call { callee, args } => {
            changed |= rewrite_operand(callee, known);
            for arg in args {
                changed |= rewrite_operand(arg, known);
            }
        }
        MirRvalueKind::Range { start, end, .. } => {
            changed |= rewrite_operand(start, known);
            changed |= rewrite_operand(end, known);
        }
        MirRvalueKind::Index { base, index } => {
            changed |= rewrite_operand(base, known);
            changed |= rewrite_operand(index, known);
        }
    }
    changed
}

/// Rewrites the reads in a terminator (return value, branch condition).
fn rewrite_terminator(terminator: &mut MirTerminator, known: &[Option<MirOperand>]) -> bool {
    match terminator {
        MirTerminator::Return {
            value: Some(value), ..
        } => rewrite_operand(value, known),
        MirTerminator::Branch { cond, .. } => rewrite_operand(cond, known),
        MirTerminator::Jump { .. } | MirTerminator::Return { value: None, .. } => false,
    }
}

/// Rewrites one operand read through `known`, preserving the read's span and
/// type. A local read is replaced with the recorded copy when one exists and
/// is not the same local.
fn rewrite_operand(operand: &mut MirOperand, known: &[Option<MirOperand>]) -> bool {
    let MirOperandKind::Local(id) = &operand.kind else {
        return false;
    };
    let Some(Some(copy)) = known.get(id.raw() as usize) else {
        return false;
    };
    // Never rewrite a read of `id` into a read of `id` itself.
    if matches!(&copy.kind, MirOperandKind::Local(cid) if *cid == *id) {
        return false;
    }
    // Preserve the read's exact span and type; the copied value is the same
    // value, so the type is identical by construction.
    let replacement = MirOperand {
        kind: copy.kind.clone(),
        span: operand.span,
        ty: operand.ty,
    };
    *operand = replacement;
    true
}

// ---------------------------------------------------------------------------
// Trivial CFG simplification
// ---------------------------------------------------------------------------

/// Simplifies the control-flow graph:
///
/// - a branch on a constant condition becomes the corresponding jump
///   (`if true` lowers to a jump to the then-arm, `if false` to the
///   else-arm);
/// - a branch whose two targets coincide becomes a jump;
/// - a jump to an empty block that only jumps again is threaded through to
///   the final target.
///
/// These rewrites never change reachable behavior: the condition is an
/// already-evaluated operand, and threading only skips blocks that contain
/// no statements.
pub struct CfgSimplify;

impl MirPass for CfgSimplify {
    fn name(&self) -> &'static str {
        "cfg-simplify"
    }

    fn run(&mut self, program: &mut MirProgram) -> bool {
        let mut changed = false;
        for item in &mut program.items {
            if let MirItemKind::Fn(f) = &mut item.kind {
                changed |= simplify_function(f);
            }
        }
        changed
    }
}

/// Applies the CFG simplifications to one function.
fn simplify_function(f: &mut MirFn) -> bool {
    let mut changed = false;
    // Branch simplification is computed from a snapshot, then applied, so
    // the borrow of `f.blocks` never overlaps a mutation of it.
    let mut rewrites: Vec<(usize, MirTerminator)> = Vec::new();
    for (index, block) in f.blocks.iter().enumerate() {
        let MirTerminator::Branch {
            cond,
            then_block,
            else_block,
            span,
        } = &block.terminator
        else {
            continue;
        };
        let jump = match &cond.kind {
            MirOperandKind::Constant(MirConstant {
                kind: MirConstantKind::Bool(true),
                ..
            }) => Some(*then_block),
            MirOperandKind::Constant(MirConstant {
                kind: MirConstantKind::Bool(false),
                ..
            }) => Some(*else_block),
            _ if then_block == else_block => Some(*then_block),
            _ => None,
        };
        if let Some(target) = jump {
            rewrites.push((
                index,
                MirTerminator::Jump {
                    target,
                    span: *span,
                },
            ));
        }
    }
    for (index, terminator) in rewrites {
        f.blocks[index].terminator = terminator;
        changed = true;
    }
    // Jump threading: skip chains of empty jump-only blocks.
    for index in 0..f.blocks.len() {
        let MirTerminator::Jump { target, span } = f.blocks[index].terminator else {
            continue;
        };
        let final_target = thread_jump_target(f, index, target);
        if final_target != target {
            f.blocks[index].terminator = MirTerminator::Jump {
                target: final_target,
                span,
            };
            changed = true;
        }
    }
    changed
}

/// Follows a jump chain from `block`'s `target` through empty blocks that
/// only jump again, returning the final target. Chains that loop back to
/// `block` are left untouched so no self-loop or cycle is created.
fn thread_jump_target(f: &MirFn, block: usize, mut target: BlockId) -> BlockId {
    let mut visited = vec![false; f.blocks.len()];
    loop {
        let index = target.raw() as usize;
        if index == block || index >= f.blocks.len() || visited[index] {
            break;
        }
        visited[index] = true;
        let candidate = &f.blocks[index];
        if !candidate.stmts.is_empty() {
            break;
        }
        match candidate.terminator {
            MirTerminator::Jump { target: next, .. } => target = next,
            _ => break,
        }
    }
    target
}

// ---------------------------------------------------------------------------
// Unreachable-block elimination
// ---------------------------------------------------------------------------

/// Removes blocks that are unreachable from the entry block (block `0`) and
/// renumbers the survivors so the block at index `i` has id `i`.
///
/// Removing an unreachable block is always behavior-preserving: unreachable
/// code can never execute, so nothing it contains — calls included — is
/// observable. Renumbering restores the deterministic ordering invariant
/// (`E-M10`), and the entry block is always retained (it is the traversal's
/// root).
pub struct UnreachableElim;

impl MirPass for UnreachableElim {
    fn name(&self) -> &'static str {
        "unreachable-elim"
    }

    fn run(&mut self, program: &mut MirProgram) -> bool {
        let mut changed = false;
        for item in &mut program.items {
            if let MirItemKind::Fn(f) = &mut item.kind {
                changed |= eliminate_unreachable(f);
            }
        }
        changed
    }
}

/// Removes unreachable blocks from `f`, returning whether anything changed.
fn eliminate_unreachable(f: &mut MirFn) -> bool {
    let count = f.blocks.len();
    let mut reachable = vec![false; count];
    let mut work = vec![0usize];
    reachable[0] = true;
    while let Some(index) = work.pop() {
        for target in terminator_targets(&f.blocks[index].terminator) {
            let target = target.raw() as usize;
            if target < count && !reachable[target] {
                reachable[target] = true;
                work.push(target);
            }
        }
    }
    if reachable.iter().all(|r| *r) {
        return false;
    }
    // Keep reachable blocks in id order and record the id remapping.
    let mut remap = vec![0u32; count];
    let mut kept: Vec<MirBlock> = Vec::with_capacity(count);
    for (index, block) in std::mem::take(&mut f.blocks).into_iter().enumerate() {
        if reachable[index] {
            remap[index] = kept.len() as u32;
            kept.push(block);
        }
    }
    for (index, block) in kept.iter_mut().enumerate() {
        block.id = BlockId::new(index as u32);
        remap_terminator(&mut block.terminator, &remap);
    }
    f.blocks = kept;
    true
}

/// The blocks a terminator may transfer control to.
fn terminator_targets(terminator: &MirTerminator) -> Vec<BlockId> {
    match terminator {
        MirTerminator::Jump { target, .. } => vec![*target],
        MirTerminator::Branch {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        MirTerminator::Return { .. } => Vec::new(),
    }
}

/// Rewrites a terminator's targets through `remap` (old id → new id).
fn remap_terminator(terminator: &mut MirTerminator, remap: &[u32]) {
    let remap_id = |id: &mut BlockId| {
        let old = id.raw() as usize;
        if let Some(&new) = remap.get(old) {
            *id = BlockId::new(new);
        }
    };
    match terminator {
        MirTerminator::Jump { target, .. } => remap_id(target),
        MirTerminator::Branch {
            then_block,
            else_block,
            ..
        } => {
            remap_id(then_block);
            remap_id(else_block);
        }
        MirTerminator::Return { .. } => {}
    }
}

// ---------------------------------------------------------------------------
// Dead-code elimination
// ---------------------------------------------------------------------------

/// Removes statements whose target is a local that is never read anywhere in
/// the enclosing function (or static initializer), provided the stored
/// rvalue is provably side-effect-free.
///
/// A store `t = rvalue` is removable only when:
///
/// - the target is a plain local (never module `Static` storage, never a
///   `Member`/`Index` place — both are observable writes whose reachability
///   is not provable);
/// - the local is never read again (reads are counted across every block,
///   statement, and terminator, and — for module statics — the final value
///   operand);
/// - the rvalue is pure: `Use`, `Unary`, `Binary`, or `Range`. `Call`,
///   `RangeNext`, `RangeFinished`, `Member`, and `Index` are never removed:
///   calls and range iteration have observable effects, and member/index
///   loads have no defined semantics yet.
///
/// Parameters are function inputs and are never treated as removable
/// targets. Local *slots* are retained (with their symbols and types) so
/// `SymbolId`/`TypeId` relationships and id stability are preserved; only
/// the dead store is removed.
pub struct DeadCodeElim;

impl MirPass for DeadCodeElim {
    fn name(&self) -> &'static str {
        "dead-code-elim"
    }

    fn run(&mut self, program: &mut MirProgram) -> bool {
        let mut changed = false;
        for item in &mut program.items {
            match &mut item.kind {
                MirItemKind::Fn(f) => changed |= eliminate_in_fn(f),
                MirItemKind::Let(stat) | MirItemKind::Const(stat) => {
                    changed |= eliminate_in_static(stat)
                }
            }
        }
        changed
    }
}

/// Removes dead stores from one function.
fn eliminate_in_fn(f: &mut MirFn) -> bool {
    let mut read = vec![false; f.locals.len()];
    for block in &f.blocks {
        for stmt in &block.stmts {
            mark_stmt_reads(stmt, &mut read);
        }
        mark_terminator_reads(&block.terminator, &mut read);
    }
    let mut changed = false;
    for block in &mut f.blocks {
        let before = block.stmts.len();
        block
            .stmts
            .retain(|stmt| !is_dead_store(stmt, &read, f.params.len()));
        changed |= block.stmts.len() != before;
    }
    changed
}

/// Removes dead stores from a module static's initializer statements.
fn eliminate_in_static(stat: &mut MirStatic) -> bool {
    let mut read = vec![false; stat.locals.len()];
    for stmt in &stat.stmts {
        mark_stmt_reads(stmt, &mut read);
    }
    // The final value operand is a read of whatever it references.
    mark_operand_read(&stat.value, &mut read);
    let before = stat.stmts.len();
    stat.stmts.retain(|stmt| !is_dead_store(stmt, &read, 0));
    stat.stmts.len() != before
}

/// Whether `stmt` is a dead store that can be removed.
fn is_dead_store(stmt: &MirStmt, read: &[bool], param_count: usize) -> bool {
    let MirStmtKind::Assign { target, rvalue } = &stmt.kind;
    let MirTargetKind::Local(id) = &target.kind else {
        return false;
    };
    // Parameters are function inputs; their stores (if any) are never
    // treated as removable.
    if (id.raw() as usize) < param_count {
        return false;
    }
    // Out of bounds cannot occur in validated MIR; treat it as a read so a
    // defensive path never removes an unknown target.
    if read.get(id.raw() as usize).copied().unwrap_or(true) {
        return false;
    }
    rvalue_is_pure(rvalue)
}

/// Whether evaluating `rvalue` can have no observable effect.
fn rvalue_is_pure(rvalue: &MirRvalue) -> bool {
    matches!(
        rvalue.kind,
        MirRvalueKind::Use(_)
            | MirRvalueKind::Unary { .. }
            | MirRvalueKind::Binary { .. }
            | MirRvalueKind::Range { .. }
    )
}

/// Marks every local read by `stmt` in `read`.
fn mark_stmt_reads(stmt: &MirStmt, read: &mut [bool]) {
    let MirStmtKind::Assign { target, rvalue } = &stmt.kind;
    mark_rvalue_reads(rvalue, read);
    match &target.kind {
        MirTargetKind::Member { base, .. } => mark_operand_read(base, read),
        MirTargetKind::Index { base, index } => {
            mark_operand_read(base, read);
            mark_operand_read(index, read);
        }
        MirTargetKind::Local(_) | MirTargetKind::Static(_) => {}
    }
}

/// Marks every local read by `rvalue` in `read`.
fn mark_rvalue_reads(rvalue: &MirRvalue, read: &mut [bool]) {
    match &rvalue.kind {
        MirRvalueKind::Use(op)
        | MirRvalueKind::Unary { operand: op, .. }
        | MirRvalueKind::RangeNext { range: op }
        | MirRvalueKind::RangeFinished { range: op }
        | MirRvalueKind::Member { base: op, .. } => mark_operand_read(op, read),
        MirRvalueKind::Binary { lhs, rhs, .. } => {
            mark_operand_read(lhs, read);
            mark_operand_read(rhs, read);
        }
        MirRvalueKind::Call { callee, args } => {
            mark_operand_read(callee, read);
            for arg in args {
                mark_operand_read(arg, read);
            }
        }
        MirRvalueKind::Range { start, end, .. } => {
            mark_operand_read(start, read);
            mark_operand_read(end, read);
        }
        MirRvalueKind::Index { base, index } => {
            mark_operand_read(base, read);
            mark_operand_read(index, read);
        }
    }
}

/// Marks a local read by a terminator (return value, branch condition).
fn mark_terminator_reads(terminator: &MirTerminator, read: &mut [bool]) {
    match terminator {
        MirTerminator::Return {
            value: Some(value), ..
        } => mark_operand_read(value, read),
        MirTerminator::Branch { cond, .. } => mark_operand_read(cond, read),
        MirTerminator::Jump { .. } | MirTerminator::Return { value: None, .. } => {}
    }
}

/// Marks `operand`'s local read, if any, in `read`.
fn mark_operand_read(operand: &MirOperand, read: &mut [bool]) {
    if let MirOperandKind::Local(id) = &operand.kind {
        if let Some(slot) = read.get_mut(id.raw() as usize) {
            *slot = true;
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the individual passes on hand-built MIR, exercising
    //! the transformations a clean pipeline can produce as well as the
    //! conservative boundaries. Integration tests live in `tests/opt.rs` and
    //! `tests/cli.rs`.

    use crate::mir::{
        BlockId, LocalId, MirBlock, MirConstant, MirConstantKind, MirFn, MirIdent, MirItem,
        MirItemKind, MirLocal, MirOperand, MirOperandKind, MirPass, MirProgram, MirRvalue,
        MirRvalueKind, MirStmt, MirStmtKind, MirTarget, MirTargetKind, MirTerminator,
    };
    use crate::source::{SourceId, Span};
    use crate::typecheck::{TypeId, TypeKind, TypeTable};

    use super::{CfgSimplify, DeadCodeElim, UnreachableElim};

    fn span_at(offset: u32) -> Span {
        Span::new(SourceId::new(0), offset..offset)
    }

    /// A table with `Int` and `Bool`, returning the bool type id.
    fn table_with_bool() -> (TypeTable, TypeId) {
        let mut table = TypeTable::new();
        let bool_ty = table.push(TypeKind::Bool);
        (table, bool_ty)
    }

    fn bool_operand(value: bool, ty: TypeId, span: Span) -> MirOperand {
        MirOperand {
            kind: MirOperandKind::Constant(MirConstant {
                kind: MirConstantKind::Bool(value),
                span,
                ty,
            }),
            span,
            ty,
        }
    }

    fn local_operand(id: LocalId, ty: TypeId, span: Span) -> MirOperand {
        MirOperand {
            kind: MirOperandKind::Local(id),
            span,
            ty,
        }
    }

    fn assign(target: MirTarget, kind: MirRvalueKind, ty: TypeId, span: Span) -> MirStmt {
        MirStmt {
            kind: MirStmtKind::Assign {
                target,
                rvalue: MirRvalue { kind, span, ty },
            },
            span,
        }
    }

    /// A minimal function with no parameters and the given blocks.
    fn function_with_blocks(blocks: Vec<MirBlock>, ty: TypeId) -> MirFn {
        MirFn {
            name: MirIdent {
                name: "f".to_string(),
                span: span_at(0),
                symbol: crate::semantics::SymbolId::new(0),
                ty,
            },
            params: Vec::new(),
            locals: Vec::new(),
            blocks,
            span: span_at(0),
            ty,
        }
    }

    fn program_with(f: MirFn, table: TypeTable) -> MirProgram {
        MirProgram {
            items: vec![MirItem {
                kind: MirItemKind::Fn(f),
                span: span_at(0),
            }],
            types: table,
            intrinsic_symbols: Vec::new(),
        }
    }

    #[test]
    fn const_fold_folds_boolean_algebra() {
        let (table, bool_ty) = table_with_bool();
        let _ = &table;
        let span = span_at(4);
        // `true && false` as a binary rvalue.
        let stmt = assign(
            MirTarget {
                kind: MirTargetKind::Local(LocalId::new(0)),
                span,
                ty: bool_ty,
            },
            MirRvalueKind::Binary {
                op: crate::ast::BinaryOp::And,
                lhs: bool_operand(true, bool_ty, span),
                rhs: bool_operand(false, bool_ty, span),
            },
            bool_ty,
            span,
        );
        let mut stmt = stmt;
        assert!(super::fold_stmt(&mut stmt));
        let MirStmtKind::Assign { rvalue, .. } = &stmt.kind;
        let MirRvalueKind::Use(op) = &rvalue.kind else {
            panic!("expected a folded `Use`");
        };
        assert_eq!(
            op.kind,
            MirOperandKind::Constant(MirConstant {
                kind: MirConstantKind::Bool(false),
                span,
                ty: bool_ty,
            })
        );
        // The rvalue keeps its span and type.
        assert_eq!(rvalue.span, span);
        assert_eq!(rvalue.ty, bool_ty);
        let _ = table;
    }

    #[test]
    fn const_fold_leaves_non_boolean_ops_alone() {
        let (mut table, bool_ty) = table_with_bool();
        let _ = &bool_ty;
        let span = span_at(4);
        // `1 < 2` is well-typed but its constant values are not carried by
        // MIR; the fold must leave it alone.
        let int_ty = table.push(TypeKind::Int);
        let stmt = assign(
            MirTarget {
                kind: MirTargetKind::Local(LocalId::new(0)),
                span,
                ty: bool_ty,
            },
            MirRvalueKind::Binary {
                op: crate::ast::BinaryOp::Lt,
                lhs: MirOperand {
                    kind: MirOperandKind::Constant(MirConstant {
                        kind: MirConstantKind::Int,
                        span,
                        ty: int_ty,
                    }),
                    span,
                    ty: int_ty,
                },
                rhs: MirOperand {
                    kind: MirOperandKind::Constant(MirConstant {
                        kind: MirConstantKind::Int,
                        span,
                        ty: int_ty,
                    }),
                    span,
                    ty: int_ty,
                },
            },
            bool_ty,
            span,
        );
        let mut stmt = stmt;
        assert!(!super::fold_stmt(&mut stmt));
    }

    #[test]
    fn cfg_simplify_folds_constant_branches() {
        let (mut table, bool_ty) = table_with_bool();
        let int_ty = table.push(TypeKind::Int);
        // block 0: branch cond=true → 1 (then), 2 (else)
        // block 1: return (kept)
        // block 2: return (becomes unreachable)
        let span = span_at(0);
        let blocks = vec![
            MirBlock {
                id: BlockId::new(0),
                stmts: Vec::new(),
                terminator: MirTerminator::Branch {
                    cond: bool_operand(true, bool_ty, span),
                    then_block: BlockId::new(1),
                    else_block: BlockId::new(2),
                    span,
                },
                span,
            },
            MirBlock {
                id: BlockId::new(1),
                stmts: Vec::new(),
                terminator: MirTerminator::Return { value: None, span },
                span,
            },
            MirBlock {
                id: BlockId::new(2),
                stmts: Vec::new(),
                terminator: MirTerminator::Return { value: None, span },
                span,
            },
        ];
        let mut program = program_with(function_with_blocks(blocks, int_ty), table);
        assert!(CfgSimplify.run(&mut program));
        let MirItemKind::Fn(f) = &program.items[0].kind else {
            unreachable!()
        };
        assert_eq!(
            f.blocks[0].terminator,
            MirTerminator::Jump {
                target: BlockId::new(1),
                span
            }
        );
        // Unreachable-elimination then removes block 2.
        assert!(UnreachableElim.run(&mut program));
        let MirItemKind::Fn(f) = &program.items[0].kind else {
            unreachable!()
        };
        assert_eq!(f.blocks.len(), 2);
        assert_eq!(f.blocks[0].id, BlockId::new(0));
        assert_eq!(f.blocks[1].id, BlockId::new(1));
    }

    #[test]
    fn cfg_simplify_threads_empty_jumps() {
        let (mut table, bool_ty) = table_with_bool();
        let int_ty = table.push(TypeKind::Int);
        let span = span_at(0);
        // block 0: jump → 1
        // block 1: (empty) jump → 2
        // block 2: return
        let blocks = vec![
            MirBlock {
                id: BlockId::new(0),
                stmts: Vec::new(),
                terminator: MirTerminator::Jump {
                    target: BlockId::new(1),
                    span,
                },
                span,
            },
            MirBlock {
                id: BlockId::new(1),
                stmts: Vec::new(),
                terminator: MirTerminator::Jump {
                    target: BlockId::new(2),
                    span,
                },
                span,
            },
            MirBlock {
                id: BlockId::new(2),
                stmts: Vec::new(),
                terminator: MirTerminator::Return { value: None, span },
                span,
            },
        ];
        let mut program = program_with(function_with_blocks(blocks, int_ty), table);
        assert!(CfgSimplify.run(&mut program));
        let MirItemKind::Fn(f) = &program.items[0].kind else {
            unreachable!()
        };
        assert_eq!(
            f.blocks[0].terminator,
            MirTerminator::Jump {
                target: BlockId::new(2),
                span
            }
        );
        // Threading must not touch a self-loop (an infinite loop).
        let mut program = program_with(
            function_with_blocks(
                vec![MirBlock {
                    id: BlockId::new(0),
                    stmts: Vec::new(),
                    terminator: MirTerminator::Jump {
                        target: BlockId::new(0),
                        span,
                    },
                    span,
                }],
                bool_ty,
            ),
            table_with_bool().0,
        );
        assert!(!CfgSimplify.run(&mut program));
    }

    #[test]
    fn dead_code_elim_removes_pure_dead_stores_only() {
        let (mut table, bool_ty) = table_with_bool();
        let int_ty = table.push(TypeKind::Int);
        let span = span_at(0);
        // A store to an unused local with a pure rvalue is removed; a store
        // with a call rvalue (potential side effects) is kept even when the
        // target is unused.
        let mut blocks = vec![MirBlock {
            id: BlockId::new(0),
            stmts: vec![
                assign(
                    MirTarget {
                        kind: MirTargetKind::Local(LocalId::new(0)),
                        span,
                        ty: bool_ty,
                    },
                    MirRvalueKind::Use(bool_operand(true, bool_ty, span)),
                    bool_ty,
                    span,
                ),
                assign(
                    MirTarget {
                        kind: MirTargetKind::Local(LocalId::new(1)),
                        span,
                        ty: int_ty,
                    },
                    MirRvalueKind::Call {
                        callee: MirOperand {
                            kind: MirOperandKind::Static(crate::semantics::SymbolId::new(9)),
                            span,
                            ty: int_ty,
                        },
                        args: Vec::new(),
                    },
                    int_ty,
                    span,
                ),
            ],
            terminator: MirTerminator::Return { value: None, span },
            span,
        }];
        // The function must declare the locals the block references.
        let mut f = function_with_blocks(Vec::new(), int_ty);
        f.locals = vec![
            MirLocal {
                name: String::new(),
                symbol: None,
                ty: bool_ty,
                mutable: false,
                span,
            },
            MirLocal {
                name: String::new(),
                symbol: None,
                ty: int_ty,
                mutable: false,
                span,
            },
        ];
        f.blocks = std::mem::take(&mut blocks);
        let mut program = program_with(f, table);
        assert!(DeadCodeElim.run(&mut program));
        let MirItemKind::Fn(f) = &program.items[0].kind else {
            unreachable!()
        };
        assert_eq!(f.blocks[0].stmts.len(), 1, "only the call store survives");
        assert!(matches!(
            f.blocks[0].stmts[0].kind,
            MirStmtKind::Assign {
                rvalue: MirRvalue {
                    kind: MirRvalueKind::Call { .. },
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn copy_prop_kills_stale_copies_on_reassignment() {
        let (table, bool_ty) = table_with_bool();
        let span = span_at(0);
        let local = |id: u32| MirTarget {
            kind: MirTargetKind::Local(LocalId::new(id)),
            span,
            ty: bool_ty,
        };
        // `a` (local 0) is an opaque source with no recorded value, `x` is
        // local 1, `y` is local 2.
        let mut stmts = vec![
            // x = a
            assign(
                local(1),
                MirRvalueKind::Use(local_operand(LocalId::new(0), bool_ty, span)),
                bool_ty,
                span,
            ),
            // y = x
            assign(
                local(2),
                MirRvalueKind::Use(local_operand(LocalId::new(1), bool_ty, span)),
                bool_ty,
                span,
            ),
            // a = <opaque value> — kills x = a and y = x: y holds the OLD
            // value of a, which can no longer be expressed as a read of a.
            assign(
                local(0),
                MirRvalueKind::Use(bool_operand(false, bool_ty, span)),
                bool_ty,
                span,
            ),
        ];
        let mut terminator = MirTerminator::Return {
            value: Some(local_operand(LocalId::new(2), bool_ty, span)),
            span,
        };
        assert!(super::propagate_block(&mut stmts, &mut terminator));
        // y is read after `a` was reassigned: the read must NOT be rewritten
        // to a read of `a` (which now holds a different value). The copy
        // chain was killed, so the read stays a read of y.
        let MirTerminator::Return { value, .. } = terminator else {
            unreachable!()
        };
        assert_eq!(value.unwrap().kind, MirOperandKind::Local(LocalId::new(2)));
        // Before the reassignment, the copy chain IS propagated: the second
        // statement's rvalue reads x, which was copied from a.
        let MirStmtKind::Assign { rvalue, .. } = &stmts[1].kind;
        let MirRvalueKind::Use(op) = &rvalue.kind else {
            unreachable!()
        };
        assert_eq!(op.kind, MirOperandKind::Local(LocalId::new(0)));
        let _ = (table, bool_ty);
    }

    #[test]
    fn copy_prop_propagates_constants_through_chains() {
        let (table, bool_ty) = table_with_bool();
        let span = span_at(0);
        let local = |id: u32| MirTarget {
            kind: MirTargetKind::Local(LocalId::new(id)),
            span,
            ty: bool_ty,
        };
        // x = true; y = x; x = false; return y — y holds the OLD value of x
        // (true), so the return correctly folds to a constant.
        let mut stmts = vec![
            assign(
                local(0),
                MirRvalueKind::Use(bool_operand(true, bool_ty, span)),
                bool_ty,
                span,
            ),
            assign(
                local(1),
                MirRvalueKind::Use(local_operand(LocalId::new(0), bool_ty, span)),
                bool_ty,
                span,
            ),
            assign(
                local(0),
                MirRvalueKind::Use(bool_operand(false, bool_ty, span)),
                bool_ty,
                span,
            ),
        ];
        let mut terminator = MirTerminator::Return {
            value: Some(local_operand(LocalId::new(1), bool_ty, span)),
            span,
        };
        assert!(super::propagate_block(&mut stmts, &mut terminator));
        let MirTerminator::Return { value, .. } = terminator else {
            unreachable!()
        };
        assert_eq!(
            value.unwrap().kind,
            MirOperandKind::Constant(MirConstant {
                kind: MirConstantKind::Bool(true),
                span,
                ty: bool_ty,
            })
        );
        let _ = (table, bool_ty);
    }
}
