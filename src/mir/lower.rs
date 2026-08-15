//! HIR → MIR lowering.
//!
//! The [`lower`] entry point walks the [`HirProgram`] once and produces a
//! [`MirProgram`] by consuming — never re-running — the answers HIR already
//! carries:
//!
//! - every expression is lowered to an *operand* (a local load, a literal
//!   constant, or a module-item reference) when it is operand-shaped, or to
//!   a temporary plus the statements that compute it otherwise;
//! - every control-flow construct (`if`/`else`, `while`, `for`, `loop`,
//!   `break`, `continue`, `return`) is lowered into explicit basic blocks,
//!   terminators, and jumps;
//! - `for` loops lower to a range iteration over the iterable value using
//!   the [`RangeNext`](MirRvalueKind::RangeNext) and
//!   [`RangeFinished`](MirRvalueKind::RangeFinished) rvalues — the inclusive
//!   flag of a syntactically written range is preserved in the `Range`
//!   construction, and iteration semantics over a range *value* are left to
//!   the backend;
//! - compound assignments (`x += 1`) desugar into a binary operation plus a
//!   store;
//! - `break` and `continue` lower to jumps to the enclosing loop's exit and
//!   continue targets, tracked on a stack as blocks are built.
//!
//! Lowering is deterministic (source order) and never panics on malformed
//! input: internal inconsistencies (`break` outside a loop, a `for` over a
//! non-range, an identifier with no corresponding local, an invalid
//! assignment target, a block left without a terminator) are collected as
//! structured [`MirError`]s and returned as an `Err`, with fallback nodes
//! produced so lowering can continue and report every independent problem.

use std::collections::{HashMap, HashSet};

use crate::ast::{AssignOp, BinaryOp};
use crate::hir::{
    HirBlock, HirConst, HirElseBranch, HirExpr, HirExprKind, HirFn, HirIdent, HirIf, HirItemKind,
    HirLet, HirProgram, HirStmt, HirStmtKind,
};
use crate::semantics::SymbolId;
use crate::source::{SourceId, Span};
use crate::typecheck::{TypeId, TypeKind, TypeTable};

use super::error::MirError;
use super::{
    BlockId, LocalId, MirBlock, MirConstant, MirConstantKind, MirFn, MirIdent, MirItem,
    MirItemKind, MirLocal, MirName, MirOperand, MirOperandKind, MirProgram, MirRvalue,
    MirRvalueKind, MirStatic, MirStmt, MirStmtKind, MirTarget, MirTargetKind, MirTerminator,
};

/// Lowers `hir` into MIR.
///
/// Returns the lowered [`MirProgram`], or every [`MirError`] collected in
/// source order when the input is internally inconsistent. These failures
/// are only reachable on malformed input — a program that passed semantic,
/// type, and HIR analysis always lowers successfully.
pub(crate) fn lower(hir: &HirProgram) -> Result<MirProgram, Vec<MirError>> {
    let mut lowerer = Lowerer::new(hir);
    lowerer.run();
    if lowerer.errors.is_empty() {
        Ok(MirProgram {
            items: lowerer.items,
            types: lowerer.table,
            intrinsic_symbols: hir.intrinsic_symbols.clone(),
        })
    } else {
        Err(lowerer.errors)
    }
}

/// The program-wide lowering traversal.
struct Lowerer<'a> {
    hir: &'a HirProgram,
    /// The type table backing every [`TypeId`] stored in the MIR. Cloned
    /// from the HIR so the program is self-contained; loop lowering may
    /// extend it (interning the `Bool` completion flag type).
    table: TypeTable,
    /// The symbols of every module-level item, so references that are not
    /// function locals lower to module-item references.
    module_symbols: HashSet<SymbolId>,
    items: Vec<MirItem>,
    errors: Vec<MirError>,
}

impl<'a> Lowerer<'a> {
    fn new(hir: &'a HirProgram) -> Self {
        let module_symbols = hir
            .items
            .iter()
            .map(|item| match &item.kind {
                HirItemKind::Fn(f) => f.name.symbol,
                HirItemKind::Let(binding) => binding.name.symbol,
                HirItemKind::Const(binding) => binding.name.symbol,
            })
            // The predeclared runtime intrinsics resolve like module items:
            // a reference is a `Static` operand the backend recognizes.
            .chain(hir.intrinsic_symbols.iter().map(|(symbol, _)| *symbol))
            .collect();
        Self {
            hir,
            table: hir.types.clone(),
            module_symbols,
            items: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn run(&mut self) {
        for item in &self.hir.items {
            let kind = match &item.kind {
                HirItemKind::Fn(f) => {
                    let builder = FnBuilder::new(&mut self.table, &self.module_symbols);
                    let (function, errors) = builder.lower(f);
                    self.errors.extend(errors);
                    MirItemKind::Fn(function)
                }
                HirItemKind::Let(binding) => MirItemKind::Let(self.lower_static(
                    &binding.name,
                    binding.mutable,
                    &binding.init,
                    binding.span,
                    binding.ty,
                )),
                HirItemKind::Const(binding) => MirItemKind::Const(self.lower_static(
                    &binding.name,
                    false,
                    &binding.init,
                    binding.span,
                    binding.ty,
                )),
            };
            self.items.push(MirItem {
                kind,
                span: item.span,
            });
        }
    }

    /// Lowers a module-level `let`/`const` binding into a [`MirStatic`]: the
    /// initializer is evaluated into temporaries and statements, and its
    /// final value is kept as an operand.
    fn lower_static(
        &mut self,
        name: &HirIdent,
        mutable: bool,
        init: &HirExpr,
        span: Span,
        ty: TypeId,
    ) -> MirStatic {
        let mut eval = StmtEval::new(&mut self.table, &self.module_symbols);
        let value = eval.eval_operand(init);
        let name = eval.mir_ident(name);
        let StmtEval {
            locals,
            stmts,
            errors,
            ..
        } = eval;
        self.errors.extend(errors);
        MirStatic {
            name,
            mutable,
            locals,
            stmts,
            value,
            span,
            ty,
        }
    }
}

/// The context in which expressions are lowered: shared state for locals,
/// statements, and the emission target, used by both function bodies and
/// module statics.
///
/// `stmts` is the statement accumulation target — for a function body it is
/// the block currently being filled; for a module static it is the static's
/// own statement list. Temporaries are allocated in `locals` as expressions
/// are lowered.
struct StmtEval<'a> {
    table: &'a mut TypeTable,
    module_symbols: &'a HashSet<SymbolId>,
    locals: Vec<MirLocal>,
    /// Symbol → local mapping for the scope being lowered (a function's
    /// locals, or empty for a module static).
    symbols: HashMap<SymbolId, LocalId>,
    stmts: Vec<MirStmt>,
    errors: Vec<MirError>,
}

impl<'a> StmtEval<'a> {
    fn new(table: &'a mut TypeTable, module_symbols: &'a HashSet<SymbolId>) -> Self {
        Self {
            table,
            module_symbols,
            locals: Vec::new(),
            symbols: HashMap::new(),
            stmts: Vec::new(),
            errors: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Declarations and small helpers
    // ------------------------------------------------------------------

    /// The MIR form of an identifier, carrying its symbol and type.
    fn mir_ident(&self, ident: &HirIdent) -> MirIdent {
        MirIdent {
            name: ident.name.clone(),
            span: ident.span,
            symbol: ident.symbol,
            ty: ident.ty,
        }
    }

    /// Declares a new local and returns its id.
    fn declare_local(
        &mut self,
        name: String,
        symbol: Option<SymbolId>,
        ty: TypeId,
        mutable: bool,
        span: Span,
    ) -> LocalId {
        let id = LocalId::new(self.locals.len() as u32);
        self.locals.push(MirLocal {
            name,
            symbol,
            ty,
            mutable,
            span,
        });
        id
    }

    /// Allocates an anonymous temporary of type `ty` and returns its id.
    fn temp(&mut self, ty: TypeId, span: Span) -> LocalId {
        self.declare_local(String::new(), None, ty, false, span)
    }

    /// The `Bool` type id. `TypeTable::push` interns concrete kinds, so
    /// repeated calls share one slot with any existing `Bool`.
    fn bool_ty(&mut self) -> TypeId {
        self.table.push(TypeKind::Bool)
    }

    /// Whether `symbol` is a module-level item (function, `let`, or
    /// `const`).
    fn is_module_item(&self, symbol: SymbolId) -> bool {
        self.module_symbols.contains(&symbol)
    }

    /// Appends a statement to the current emission target.
    fn emit(&mut self, stmt: MirStmt) {
        self.stmts.push(stmt);
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    /// Lowers `expr` to an operand, emitting any statements needed to
    /// compute it into the current emission target.
    ///
    /// Operand-shaped expressions (literals, identifier references) produce
    /// an operand directly; compound expressions are computed into a fresh
    /// temporary and the temporary's load is returned.
    fn eval_operand(&mut self, expr: &HirExpr) -> MirOperand {
        match &expr.kind {
            HirExprKind::Int
            | HirExprKind::Float
            | HirExprKind::Str
            | HirExprKind::Char
            | HirExprKind::Bool(_)
            | HirExprKind::Null => self.literal_operand(expr),
            HirExprKind::Var(ident) => self.operand_for_var(ident),
            HirExprKind::Unary { op, operand } => {
                let operand = self.eval_operand(operand);
                self.temp_rvalue(expr, MirRvalueKind::Unary { op: *op, operand })
            }
            HirExprKind::Binary { op, lhs, rhs } => {
                let lhs = self.eval_operand(lhs);
                let rhs = self.eval_operand(rhs);
                self.temp_rvalue(expr, MirRvalueKind::Binary { op: *op, lhs, rhs })
            }
            HirExprKind::Range {
                inclusive,
                start,
                end,
            } => {
                let start = self.eval_operand(start);
                let end = self.eval_operand(end);
                self.temp_rvalue(
                    expr,
                    MirRvalueKind::Range {
                        inclusive: *inclusive,
                        start,
                        end,
                    },
                )
            }
            HirExprKind::Call { callee, args } => {
                let callee = self.eval_operand(callee);
                let args = args.iter().map(|arg| self.eval_operand(arg)).collect();
                self.temp_rvalue(expr, MirRvalueKind::Call { callee, args })
            }
            HirExprKind::Member { base, member } => {
                let base = self.eval_operand(base);
                self.temp_rvalue(
                    expr,
                    MirRvalueKind::Member {
                        base,
                        member: MirName {
                            name: member.name.clone(),
                            span: member.span,
                        },
                    },
                )
            }
            HirExprKind::Index { base, index } => {
                let base = self.eval_operand(base);
                let index = self.eval_operand(index);
                self.temp_rvalue(expr, MirRvalueKind::Index { base, index })
            }
            HirExprKind::Assign { .. } => self.eval_assign(expr),
        }
    }

    /// A literal expression as a constant operand.
    fn literal_operand(&self, expr: &HirExpr) -> MirOperand {
        let kind = match &expr.kind {
            HirExprKind::Int => MirConstantKind::Int,
            HirExprKind::Float => MirConstantKind::Float,
            HirExprKind::Str => MirConstantKind::Str,
            HirExprKind::Char => MirConstantKind::Char,
            HirExprKind::Bool(value) => MirConstantKind::Bool(*value),
            HirExprKind::Null => MirConstantKind::Null,
            _ => unreachable!("literal_operand is only called for literals"),
        };
        let constant = MirConstant {
            kind,
            span: expr.span,
            ty: expr.ty,
        };
        MirOperand {
            kind: MirOperandKind::Constant(constant),
            span: expr.span,
            ty: expr.ty,
        }
    }

    /// A variable reference as an operand: a local load when the symbol is
    /// a local of the current scope, a module-item reference when it is a
    /// module-level declaration, and a structured error otherwise (only
    /// reachable on malformed input).
    fn operand_for_var(&mut self, ident: &HirIdent) -> MirOperand {
        if let Some(&local) = self.symbols.get(&ident.symbol) {
            return MirOperand {
                kind: MirOperandKind::Local(local),
                span: ident.span,
                ty: ident.ty,
            };
        }
        if self.is_module_item(ident.symbol) {
            return MirOperand {
                kind: MirOperandKind::Static(ident.symbol),
                span: ident.span,
                ty: ident.ty,
            };
        }
        self.errors.push(MirError::unresolved_local(ident.span));
        MirOperand {
            kind: MirOperandKind::Static(ident.symbol),
            span: ident.span,
            ty: ident.ty,
        }
    }

    /// Computes `kind` into a fresh temporary and returns the temporary's
    /// load. `expr` supplies the rvalue's span and type.
    fn temp_rvalue(&mut self, expr: &HirExpr, kind: MirRvalueKind) -> MirOperand {
        let temp = self.temp(expr.ty, expr.span);
        let target = MirTarget {
            kind: MirTargetKind::Local(temp),
            span: expr.span,
            ty: expr.ty,
        };
        self.emit(MirStmt {
            kind: MirStmtKind::Assign {
                target,
                rvalue: MirRvalue {
                    kind,
                    span: expr.span,
                    ty: expr.ty,
                },
            },
            span: expr.span,
        });
        MirOperand {
            kind: MirOperandKind::Local(temp),
            span: expr.span,
            ty: expr.ty,
        }
    }

    /// Lowers an assignment expression, emitting the store and returning
    /// the assigned value as an operand (so nested `a = b = 5` chains work).
    ///
    /// The value is evaluated before the target, deterministically. A plain
    /// assignment stores the value directly; a compound assignment
    /// (`x += v`) loads the target's current value, applies the
    /// corresponding binary operator, and stores the result.
    fn eval_assign(&mut self, expr: &HirExpr) -> MirOperand {
        let (op, target_expr, value_expr) = match &expr.kind {
            HirExprKind::Assign { op, target, value } => (*op, target, value),
            _ => unreachable!("eval_assign is only called for assignments"),
        };
        let value = self.eval_operand(value_expr);
        let target = match self.place_for_target(target_expr) {
            Ok(target) => target,
            Err(error) => {
                self.errors.push(error);
                return value;
            }
        };
        match op {
            AssignOp::Assign => {
                self.emit(MirStmt {
                    kind: MirStmtKind::Assign {
                        target,
                        rvalue: use_rvalue(value.clone(), expr.span, expr.ty),
                    },
                    span: expr.span,
                });
                value
            }
            _ => {
                let binary = compound_binary(op);
                let lhs = self.load_target(&target);
                let result = self.temp(expr.ty, expr.span);
                let result_operand = MirOperand {
                    kind: MirOperandKind::Local(result),
                    span: expr.span,
                    ty: expr.ty,
                };
                let result_target = MirTarget {
                    kind: MirTargetKind::Local(result),
                    span: expr.span,
                    ty: expr.ty,
                };
                self.emit(MirStmt {
                    kind: MirStmtKind::Assign {
                        target: result_target,
                        rvalue: MirRvalue {
                            kind: MirRvalueKind::Binary {
                                op: binary,
                                lhs,
                                rhs: value,
                            },
                            span: expr.span,
                            ty: expr.ty,
                        },
                    },
                    span: expr.span,
                });
                self.emit(MirStmt {
                    kind: MirStmtKind::Assign {
                        target,
                        rvalue: use_rvalue(result_operand.clone(), expr.span, expr.ty),
                    },
                    span: expr.span,
                });
                result_operand
            }
        }
    }

    /// The storage target of an assignment expression.
    ///
    /// Variable targets resolve to a local or to module-level storage; a
    /// member/index target evaluates its base (and index) to operands and
    /// keeps the place structurally. Any other shape is an internal error
    /// (the parser and semantic analysis reject non-place targets).
    fn place_for_target(&mut self, expr: &HirExpr) -> Result<MirTarget, MirError> {
        match &expr.kind {
            HirExprKind::Var(ident) => {
                if let Some(&local) = self.symbols.get(&ident.symbol) {
                    Ok(MirTarget {
                        kind: MirTargetKind::Local(local),
                        span: expr.span,
                        ty: expr.ty,
                    })
                } else if self.is_module_item(ident.symbol) {
                    Ok(MirTarget {
                        kind: MirTargetKind::Static(ident.symbol),
                        span: expr.span,
                        ty: expr.ty,
                    })
                } else {
                    Err(MirError::unresolved_local(expr.span))
                }
            }
            HirExprKind::Member { base, member } => {
                let base = self.eval_operand(base);
                Ok(MirTarget {
                    kind: MirTargetKind::Member {
                        base,
                        member: MirName {
                            name: member.name.clone(),
                            span: member.span,
                        },
                    },
                    span: expr.span,
                    ty: expr.ty,
                })
            }
            HirExprKind::Index { base, index } => {
                let base = self.eval_operand(base);
                let index = self.eval_operand(index);
                Ok(MirTarget {
                    kind: MirTargetKind::Index { base, index },
                    span: expr.span,
                    ty: expr.ty,
                })
            }
            _ => Err(MirError::invalid_assignment_target(expr.span)),
        }
    }

    /// Loads a target's current value as an operand: a local/static load,
    /// or — for member/index targets — a member/index rvalue into a fresh
    /// temporary.
    fn load_target(&mut self, target: &MirTarget) -> MirOperand {
        match &target.kind {
            MirTargetKind::Local(id) => MirOperand {
                kind: MirOperandKind::Local(*id),
                span: target.span,
                ty: target.ty,
            },
            MirTargetKind::Static(symbol) => MirOperand {
                kind: MirOperandKind::Static(*symbol),
                span: target.span,
                ty: target.ty,
            },
            MirTargetKind::Member { base, member } => {
                let kind = MirRvalueKind::Member {
                    base: base.clone(),
                    member: member.clone(),
                };
                self.loaded_operand(target, kind)
            }
            MirTargetKind::Index { base, index } => {
                let kind = MirRvalueKind::Index {
                    base: base.clone(),
                    index: index.clone(),
                };
                self.loaded_operand(target, kind)
            }
        }
    }

    /// Computes a member/index load into a temporary and returns its load.
    fn loaded_operand(&mut self, target: &MirTarget, kind: MirRvalueKind) -> MirOperand {
        let temp = self.temp(target.ty, target.span);
        let target_for_temp = MirTarget {
            kind: MirTargetKind::Local(temp),
            span: target.span,
            ty: target.ty,
        };
        self.emit(MirStmt {
            kind: MirStmtKind::Assign {
                target: target_for_temp,
                rvalue: MirRvalue {
                    kind,
                    span: target.span,
                    ty: target.ty,
                },
            },
            span: target.span,
        });
        MirOperand {
            kind: MirOperandKind::Local(temp),
            span: target.span,
            ty: target.ty,
        }
    }
}

/// The binary operation a compound assignment operator desugars to.
fn compound_binary(op: AssignOp) -> BinaryOp {
    match op {
        AssignOp::AddAssign => BinaryOp::Add,
        AssignOp::SubAssign => BinaryOp::Sub,
        AssignOp::MulAssign => BinaryOp::Mul,
        AssignOp::DivAssign => BinaryOp::Div,
        AssignOp::RemAssign => BinaryOp::Rem,
        AssignOp::Assign => unreachable!("plain assignment is handled before compound lowering"),
    }
}

/// An rvalue that copies `operand` into the target.
fn use_rvalue(operand: MirOperand, span: Span, ty: TypeId) -> MirRvalue {
    MirRvalue {
        kind: MirRvalueKind::Use(operand),
        span,
        ty,
    }
}

/// The enclosing-loop context a `break`/`continue` resolves against.
#[derive(Debug, Clone, Copy)]
struct LoopCtx {
    /// Where `break` jumps: the loop's exit block.
    break_target: BlockId,
    /// Where `continue` jumps: the loop's continue block.
    continue_target: BlockId,
}

/// The function-body builder: owns the block graph and fills it as
/// statements and expressions are lowered.
struct FnBuilder<'a> {
    eval: StmtEval<'a>,
    /// The parameters as local ids (the first locals).
    params: Vec<LocalId>,
    /// Finalized blocks, ordered by id.
    blocks: Vec<MirBlock>,
    /// The next block id to allocate. Equal to `blocks.len()` while no
    /// block is in flight.
    next_id: u32,
    /// The block currently being filled, if any. Blocks are pushed into
    /// `blocks` (with their terminator) the moment they are terminated.
    current: Option<BlockId>,
    /// Span of the block currently being filled.
    current_span: Span,
    /// The stack of enclosing loops, innermost last.
    loops: Vec<LoopCtx>,
}

impl<'a> FnBuilder<'a> {
    fn new(table: &'a mut TypeTable, module_symbols: &'a HashSet<SymbolId>) -> Self {
        Self {
            eval: StmtEval::new(table, module_symbols),
            params: Vec::new(),
            blocks: Vec::new(),
            next_id: 0,
            current: None,
            current_span: Span::new(SourceId::new(0), 0..0),
            loops: Vec::new(),
        }
    }

    /// Lowers a function: parameters become the first locals, the body is
    /// lowered into the entry block, and falling off the end of the body is
    /// a bare return.
    fn lower(mut self, f: &HirFn) -> (MirFn, Vec<MirError>) {
        let name = self.eval.mir_ident(&f.name);
        for param in &f.params {
            let local = self.eval.declare_local(
                param.name.name.clone(),
                Some(param.name.symbol),
                param.ty,
                false,
                param.span,
            );
            self.params.push(local);
            self.eval.symbols.insert(param.name.symbol, local);
        }
        let entry = self.alloc_block();
        self.start_block(entry, f.body.span);
        self.lower_block(&f.body);
        // Falling off the end of a function body is a bare return.
        self.fall_through_return(f.body.span);
        // Defensive: every block must end in exactly one terminator. A block
        // left unterminated is an internal builder error; finalize it with a
        // bare return so the program stays structurally valid.
        if let Some(current) = self.current {
            self.eval
                .errors
                .push(MirError::missing_terminator(self.current_span));
            self.blocks.push(MirBlock {
                id: current,
                stmts: std::mem::take(&mut self.eval.stmts),
                terminator: MirTerminator::Return {
                    value: None,
                    span: self.current_span,
                },
                span: self.current_span,
            });
            self.current = None;
        }
        let function = MirFn {
            name,
            params: std::mem::take(&mut self.params),
            locals: std::mem::take(&mut self.eval.locals),
            blocks: Self::renumber(std::mem::take(&mut self.blocks)),
            span: f.span,
            ty: f.ty,
        };
        (function, std::mem::take(&mut self.eval.errors))
    }

    /// Renumbers finalized blocks so ids are contiguous and match the block
    /// list order, with the entry block first.
    ///
    /// Blocks are allocated when their construct is entered but finalized
    /// (pushed into `blocks`) when they are terminated, so a loop's exit
    /// block — allocated before the body's nested blocks — can be finalized
    /// *after* them. This pass assigns each finalized block its final id in
    /// list order (finalization order, deterministic for identical input)
    /// and rewrites every terminator target, restoring the invariant that
    /// the block at index `i` has id `i`.
    fn renumber(blocks: Vec<MirBlock>) -> Vec<MirBlock> {
        let mapping: HashMap<BlockId, BlockId> = blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (block.id, BlockId::new(index as u32)))
            .collect();
        let remap = |id: BlockId| {
            debug_assert!(
                mapping.contains_key(&id),
                "terminator targets a block that was not finalized"
            );
            mapping.get(&id).copied().unwrap_or(id)
        };
        blocks
            .into_iter()
            .enumerate()
            .map(|(index, block)| {
                let terminator = match block.terminator {
                    MirTerminator::Return { value, span } => MirTerminator::Return { value, span },
                    MirTerminator::Jump { target, span } => MirTerminator::Jump {
                        target: remap(target),
                        span,
                    },
                    MirTerminator::Branch {
                        cond,
                        then_block,
                        else_block,
                        span,
                    } => MirTerminator::Branch {
                        cond,
                        then_block: remap(then_block),
                        else_block: remap(else_block),
                        span,
                    },
                };
                MirBlock {
                    id: BlockId::new(index as u32),
                    stmts: block.stmts,
                    terminator,
                    span: block.span,
                }
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Block machinery
    // ------------------------------------------------------------------

    /// Allocates a block id. The block is only materialized (pushed into
    /// `blocks`) when it is terminated, in id order.
    fn alloc_block(&mut self) -> BlockId {
        let id = BlockId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Begins filling `id` as the current block.
    fn start_block(&mut self, id: BlockId, span: Span) {
        debug_assert!(self.current.is_none(), "a block is already being filled");
        self.current = Some(id);
        self.current_span = span;
    }

    /// Ensures a current block exists, creating one if control flow left
    /// the previous block.
    fn ensure_current(&mut self, span: Span) {
        if self.current.is_none() {
            let id = self.alloc_block();
            self.start_block(id, span);
        }
    }

    /// Ends the current block with `terminator`, materializing it.
    ///
    /// This is the only place blocks are finalized; every block therefore
    /// has exactly one terminator by construction. If no block is current
    /// (an internal invariant violation) a structured error is recorded and
    /// nothing is emitted — the program is discarded on errors.
    fn terminate(&mut self, terminator: MirTerminator) {
        let Some(id) = self.current else {
            self.eval
                .errors
                .push(MirError::missing_terminator(self.current_span));
            return;
        };
        self.blocks.push(MirBlock {
            id,
            stmts: std::mem::take(&mut self.eval.stmts),
            terminator,
            span: self.current_span,
        });
        self.current = None;
    }

    /// Emits a statement into the current block, creating one if needed.
    fn emit_stmt(&mut self, stmt: MirStmt) {
        self.ensure_current(stmt.span);
        self.eval.emit(stmt);
    }

    /// Jumps to `target` when the current block fell through (has no
    /// terminator yet). When control flow already diverged (a `return`,
    /// `break`, or `continue`), nothing is emitted.
    fn jump_to(&mut self, target: BlockId, span: Span) {
        if self.current.is_some() {
            self.terminate(MirTerminator::Jump { target, span });
        }
    }

    /// Ends a function body that fell off the end with a bare return.
    fn fall_through_return(&mut self, span: Span) {
        if self.current.is_some() {
            self.terminate(MirTerminator::Return { value: None, span });
        }
    }

    /// After lowering a branch arm, jumps to the shared continuation block,
    /// creating it on first use. When the arm diverged (a `return`,
    /// `break`, or `continue`), nothing is emitted.
    fn finish_into(&mut self, after: &mut Option<BlockId>, span: Span) {
        if self.current.is_some() {
            let target = *after.get_or_insert_with(|| self.alloc_block());
            self.terminate(MirTerminator::Jump { target, span });
        }
    }

    // ------------------------------------------------------------------
    // Statements and control flow
    // ------------------------------------------------------------------

    fn lower_block(&mut self, block: &HirBlock) {
        for stmt in &block.stmts {
            self.lower_stmt(stmt);
        }
    }

    fn lower_stmt(&mut self, stmt: &HirStmt) {
        match &stmt.kind {
            HirStmtKind::Let(binding) => self.lower_binding(binding, binding.mutable),
            HirStmtKind::Const(binding) => self.lower_const(binding),
            HirStmtKind::Return(value) => {
                // A `return` after a terminator is unreachable dead code;
                // skip it rather than start a new block.
                if self.current.is_none() {
                    return;
                }
                let value = value.as_ref().map(|expr| self.eval.eval_operand(expr));
                self.terminate(MirTerminator::Return {
                    value,
                    span: stmt.span,
                });
            }
            HirStmtKind::Break => {
                let Some(ctx) = self.loops.last() else {
                    // Defensive: semantic analysis rejects `break` outside a
                    // loop; report and continue lowering.
                    self.eval
                        .errors
                        .push(MirError::break_outside_loop(stmt.span));
                    return;
                };
                // An unreachable `break` (after a terminator) is skipped.
                if self.current.is_some() {
                    self.terminate(MirTerminator::Jump {
                        target: ctx.break_target,
                        span: stmt.span,
                    });
                }
            }
            HirStmtKind::Continue => {
                let Some(ctx) = self.loops.last() else {
                    // Defensive: semantic analysis rejects `continue`
                    // outside a loop; report and continue lowering.
                    self.eval
                        .errors
                        .push(MirError::continue_outside_loop(stmt.span));
                    return;
                };
                // An unreachable `continue` (after a terminator) is skipped.
                if self.current.is_some() {
                    self.terminate(MirTerminator::Jump {
                        target: ctx.continue_target,
                        span: stmt.span,
                    });
                }
            }
            HirStmtKind::If(stmt) => self.lower_if(stmt),
            HirStmtKind::While { cond, body } => self.lower_while(cond, body, stmt.span),
            HirStmtKind::For {
                var,
                iterable,
                body,
            } => self.lower_for(var, iterable, body, stmt.span),
            HirStmtKind::Loop(body) => self.lower_loop(body, stmt.span),
            HirStmtKind::Expr(expr) => {
                // Evaluate for side effects; the value is discarded. A dead
                // expression (after a terminator) starts a fresh block so
                // the emission target always exists.
                self.ensure_current(stmt.span);
                let _ = self.eval.eval_operand(expr);
            }
        }
    }

    /// Declares a binding's local, evaluates its initializer, and stores it.
    fn lower_binding(&mut self, binding: &HirLet, mutable: bool) {
        let local = self.eval.declare_local(
            binding.name.name.clone(),
            Some(binding.name.symbol),
            binding.ty,
            mutable,
            binding.span,
        );
        self.eval.symbols.insert(binding.name.symbol, local);
        let value = self.eval.eval_operand(&binding.init);
        let target = MirTarget {
            kind: MirTargetKind::Local(local),
            span: binding.span,
            ty: binding.ty,
        };
        self.emit_stmt(MirStmt {
            kind: MirStmtKind::Assign {
                target,
                rvalue: use_rvalue(value, binding.span, binding.ty),
            },
            span: binding.span,
        });
    }

    /// A `const` binding inside a function body: like a `let`, but the slot
    /// is immutable.
    fn lower_const(&mut self, binding: &HirConst) {
        let local = self.eval.declare_local(
            binding.name.name.clone(),
            Some(binding.name.symbol),
            binding.ty,
            false,
            binding.span,
        );
        self.eval.symbols.insert(binding.name.symbol, local);
        let value = self.eval.eval_operand(&binding.init);
        let target = MirTarget {
            kind: MirTargetKind::Local(local),
            span: binding.span,
            ty: binding.ty,
        };
        self.emit_stmt(MirStmt {
            kind: MirStmtKind::Assign {
                target,
                rvalue: use_rvalue(value, binding.span, binding.ty),
            },
            span: binding.span,
        });
    }

    /// Lowers an `if`/`else if`/`else` statement into a conditional branch
    /// with two branch blocks that join at a shared continuation block.
    fn lower_if(&mut self, stmt: &HirIf) {
        let mut after = None;
        self.lower_if_into(stmt, &mut after);
        // Continue in the shared continuation block, when one exists.
        if let Some(after) = after {
            self.start_block(after, stmt.span);
        }
    }

    /// The workhorse of [`FnBuilder::lower_if`]: lowers one `if` node,
    /// joining both arms into the shared `after` continuation block.
    ///
    /// The condition is evaluated in the current block, which the branch
    /// terminator then ends. The continuation block is created lazily on
    /// first fall-through — when both arms diverge (`return`/`break`/
    /// `continue`), no dead continuation block is produced — and nested
    /// `else if` nodes receive the *same* `after` slot, so an `else if`
    /// chain joins at one block instead of cascading through intermediates.
    /// On return, `current` is `None`; the caller starts the continuation.
    fn lower_if_into(&mut self, stmt: &HirIf, after: &mut Option<BlockId>) {
        // A dead `if` (after a terminator) starts a fresh block so the
        // emission target always exists.
        self.ensure_current(stmt.span);
        let cond = self.eval.eval_operand(&stmt.cond);
        let then_block = self.alloc_block();
        let else_block = self.alloc_block();
        self.terminate(MirTerminator::Branch {
            cond,
            then_block,
            else_block,
            span: stmt.span,
        });
        // Then arm.
        self.start_block(then_block, stmt.then_block.span);
        self.lower_block(&stmt.then_block);
        self.finish_into(after, stmt.span);
        // Else arm.
        self.start_block(else_block, stmt.span);
        match &stmt.else_branch {
            None => self.finish_into(after, stmt.span),
            Some(HirElseBranch::Block(block)) => {
                self.lower_block(block);
                self.finish_into(after, stmt.span);
            }
            Some(HirElseBranch::If(nested)) => {
                // The nested else-if joins the same continuation block.
                self.lower_if_into(nested, after);
            }
        }
    }

    /// Lowers `while cond { body }` into a header block that tests the
    /// condition, a body block, and an exit block. The preceding block
    /// jumps into the header; `continue` and the natural loop-back both
    /// jump to the header, which re-evaluates the condition; `break` jumps
    /// to the exit.
    fn lower_while(&mut self, cond: &HirExpr, body: &HirBlock, span: Span) {
        let header = self.alloc_block();
        let body_block = self.alloc_block();
        let exit = self.alloc_block();
        self.loops.push(LoopCtx {
            break_target: exit,
            continue_target: header,
        });
        // Jump into the loop from the preceding block.
        self.ensure_current(span);
        self.jump_to(header, span);
        self.start_block(header, span);
        let cond = self.eval.eval_operand(cond);
        self.terminate(MirTerminator::Branch {
            cond,
            then_block: body_block,
            else_block: exit,
            span,
        });
        self.start_block(body_block, body.span);
        self.lower_block(body);
        self.jump_to(header, span);
        self.loops.pop();
        self.start_block(exit, span);
    }

    /// Lowers `for var in iterable { body }` into a range iteration:
    ///
    /// ```text
    /// init:   iter = <iterable value>
    ///         jump header
    /// header: done = RangeFinished(iter)     ← continue target
    ///         branch done → exit, body
    /// body:   var = RangeNext(iter)
    ///         <body statements>
    ///         jump header
    /// exit:   ...
    /// ```
    ///
    /// `continue` jumps to the header (which re-checks completion and lets
    /// the body fetch the next element); `break` jumps to the exit. A
    /// syntactically written range keeps its inclusive flag in the `Range`
    /// construction; iteration over a range *value* defers inclusive-ness
    /// to the backend.
    fn lower_for(&mut self, var: &HirIdent, iterable: &HirExpr, body: &HirBlock, span: Span) {
        // Defensive: the type checker guarantees a range-typed iterable; an
        // internally inconsistent one is reported and lowered through the
        // value path anyway, so all independent problems surface.
        if !matches!(self.eval.table.kind(iterable.ty), Some(TypeKind::Range(_))) {
            self.eval
                .errors
                .push(MirError::non_range_for_iterable(iterable.span));
        }
        // The loop variable's slot is written by the loop machinery each
        // iteration, so it is marked mutable (source-level reassignment is
        // still rejected by semantic analysis).
        let var_local =
            self.eval
                .declare_local(var.name.clone(), Some(var.symbol), var.ty, true, var.span);
        self.eval.symbols.insert(var.symbol, var_local);
        let iter_local =
            self.eval
                .declare_local(String::new(), None, iterable.ty, false, iterable.span);
        let bool_ty = self.eval.bool_ty();
        let done_local =
            self.eval
                .declare_local(String::new(), None, bool_ty, false, iterable.span);

        let init_block = self.alloc_block();
        let header = self.alloc_block();
        let body_block = self.alloc_block();
        let exit = self.alloc_block();
        self.loops.push(LoopCtx {
            break_target: exit,
            continue_target: header,
        });

        // Jump into the init block from the preceding block.
        self.ensure_current(span);
        self.jump_to(init_block, span);

        // Init: capture the iterable value once.
        self.start_block(init_block, iterable.span);
        let iter_rvalue = match &iterable.kind {
            HirExprKind::Range {
                inclusive,
                start,
                end,
            } => {
                let start = self.eval.eval_operand(start);
                let end = self.eval.eval_operand(end);
                MirRvalue {
                    kind: MirRvalueKind::Range {
                        inclusive: *inclusive,
                        start,
                        end,
                    },
                    span: iterable.span,
                    ty: iterable.ty,
                }
            }
            _ => {
                let value = self.eval.eval_operand(iterable);
                use_rvalue(value, iterable.span, iterable.ty)
            }
        };
        self.emit_stmt(MirStmt {
            kind: MirStmtKind::Assign {
                target: MirTarget {
                    kind: MirTargetKind::Local(iter_local),
                    span: iterable.span,
                    ty: iterable.ty,
                },
                rvalue: iter_rvalue,
            },
            span: iterable.span,
        });
        self.terminate(MirTerminator::Jump {
            target: header,
            span,
        });

        // Header: completion test.
        self.start_block(header, span);
        let done_operand = MirOperand {
            kind: MirOperandKind::Local(done_local),
            span: iterable.span,
            ty: self.eval.bool_ty(),
        };
        self.emit_stmt(MirStmt {
            kind: MirStmtKind::Assign {
                target: MirTarget {
                    kind: MirTargetKind::Local(done_local),
                    span: iterable.span,
                    ty: done_operand.ty,
                },
                rvalue: MirRvalue {
                    kind: MirRvalueKind::RangeFinished {
                        range: MirOperand {
                            kind: MirOperandKind::Local(iter_local),
                            span: iterable.span,
                            ty: iterable.ty,
                        },
                    },
                    span: iterable.span,
                    ty: done_operand.ty,
                },
            },
            span: iterable.span,
        });
        self.terminate(MirTerminator::Branch {
            cond: done_operand,
            then_block: exit,
            else_block: body_block,
            span,
        });

        // Body: fetch the next element, run the body, loop back.
        self.start_block(body_block, body.span);
        self.emit_stmt(MirStmt {
            kind: MirStmtKind::Assign {
                target: MirTarget {
                    kind: MirTargetKind::Local(var_local),
                    span: var.span,
                    ty: var.ty,
                },
                rvalue: MirRvalue {
                    kind: MirRvalueKind::RangeNext {
                        range: MirOperand {
                            kind: MirOperandKind::Local(iter_local),
                            span: iterable.span,
                            ty: iterable.ty,
                        },
                    },
                    span: iterable.span,
                    ty: var.ty,
                },
            },
            span: var.span,
        });
        self.lower_block(body);
        self.jump_to(header, span);
        self.loops.pop();
        self.start_block(exit, span);
    }

    /// Lowers `loop { body }` into a single-body-block loop: the preceding
    /// block jumps into the body, the body jumps back to itself, `continue`
    /// jumps to the body start, and `break` jumps to the exit block.
    fn lower_loop(&mut self, body: &HirBlock, span: Span) {
        let header = self.alloc_block();
        let exit = self.alloc_block();
        self.loops.push(LoopCtx {
            break_target: exit,
            continue_target: header,
        });
        // Jump into the loop from the preceding block.
        self.ensure_current(span);
        self.jump_to(header, span);
        self.start_block(header, span);
        self.lower_block(body);
        self.jump_to(header, span);
        self.loops.pop();
        self.start_block(exit, span);
    }
}

#[cfg(test)]
mod tests {
    //! Internal-failure tests for the lowering error paths that a clean
    //! pipeline (valid HIR) can never reach. These build on real analysis
    //! results and mutate the lowered HIR to fabricate inconsistent inputs,
    //! asserting the structured errors produced instead of panics.

    use std::path::Path;

    use crate::ast::Ast;
    use crate::hir::{
        self, HirExpr, HirExprKind, HirIdent, HirItemKind, HirProgram, HirStmt, HirStmtKind,
    };
    use crate::mir::MirErrorKind;
    use crate::parser;
    use crate::semantics::{self, SemanticResult};
    use crate::source::{SourceId, SourceMap, Span};
    use crate::typecheck::{self, TypeResult};

    use super::lower;

    /// Parses, semantically analyzes, and type-checks `src`, asserting it
    /// is clean. The file is registered as the first source (id `0`).
    fn analyze_src(src: &str) -> (Ast, SemanticResult, TypeResult) {
        let mut sources = SourceMap::new();
        let id = sources.add(Path::new("t.mink"), src);
        let file = sources.get(id).unwrap();
        let parsed = parser::parse(file);
        assert!(
            parsed.is_valid(),
            "test source must parse: {:?}",
            parsed.parse_errors()
        );
        let (ast, _, _) = parsed.into_parts();
        let semantic = semantics::analyze(&ast);
        let types = typecheck::check(&ast, &semantic, &sources);
        (ast, semantic, types)
    }

    fn text_span(src: &str, needle: &str) -> Span {
        let start = src
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not found"));
        Span::new(
            SourceId::new(0),
            start as u32..start as u32 + needle.len() as u32,
        )
    }

    /// The lowered HIR of a clean program.
    fn clean_hir(src: &str) -> HirProgram {
        let (ast, semantic, types) = analyze_src(src);
        hir::lower(&ast, &semantic, &types)
            .unwrap_or_else(|errors| panic!("clean front end must lower: {errors:?}"))
    }

    #[test]
    fn break_without_loop_is_reported() {
        let src = "fn f() { return; }";
        let mut program = clean_hir(src);
        let HirItemKind::Fn(f) = &mut program.items[0].kind else {
            unreachable!()
        };
        f.body.stmts = vec![HirStmt {
            kind: HirStmtKind::Break,
            span: text_span(src, "return"),
        }];
        let errors = lower(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::BreakOutsideLoop);
        assert_eq!(errors[0].code(), "E-M01");
        assert_eq!(errors[0].span(), text_span(src, "return"));
    }

    #[test]
    fn continue_without_loop_is_reported() {
        let src = "fn f() { return; }";
        let mut program = clean_hir(src);
        let HirItemKind::Fn(f) = &mut program.items[0].kind else {
            unreachable!()
        };
        f.body.stmts = vec![HirStmt {
            kind: HirStmtKind::Continue,
            span: text_span(src, "return"),
        }];
        let errors = lower(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::ContinueOutsideLoop);
        assert_eq!(errors[0].code(), "E-M02");
    }

    #[test]
    fn for_over_non_range_is_reported() {
        let src = "fn f() { for i in 0..10 { } }";
        let (_ast, _semantic, types) = analyze_src(src);
        let int_ty = types
            .expr_type_exact(text_span(src, "0"))
            .expect("the range start has a type");
        let mut program = clean_hir(src);
        // Replace the iterable with an integer literal of integer type: the
        // iterable is then not range-typed at all.
        let HirItemKind::Fn(f) = &mut program.items[0].kind else {
            unreachable!()
        };
        let HirStmtKind::For { iterable, .. } = &mut f.body.stmts[0].kind else {
            unreachable!()
        };
        iterable.kind = HirExprKind::Int;
        iterable.ty = int_ty;
        let errors = lower(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::NonRangeForIterable);
        assert_eq!(errors[0].code(), "E-M03");
        assert_eq!(errors[0].span(), text_span(src, "0..10"));
    }

    #[test]
    fn unresolved_var_reference_is_reported() {
        // A parameter of one function referenced from another: the symbol is
        // neither a local of the referencing function nor a module item.
        let src = "fn g(p) { } fn f() { return; }";
        let (_ast, semantic, types) = analyze_src(src);
        let p = semantic.symbols().iter().find(|s| s.name == "p").unwrap();
        let p_ty = types.symbol_type(p.id).expect("p has a type");
        let mut program = clean_hir(src);
        let HirItemKind::Fn(f) = &mut program.items[1].kind else {
            unreachable!()
        };
        f.body.stmts = vec![HirStmt {
            kind: HirStmtKind::Expr(HirExpr {
                kind: HirExprKind::Var(HirIdent {
                    name: "p".to_string(),
                    span: p.span,
                    symbol: p.id,
                    ty: p_ty,
                }),
                span: p.span,
                ty: p_ty,
            }),
            span: p.span,
        }];
        let errors = lower(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::UnresolvedLocal);
        assert_eq!(errors[0].code(), "E-M04");
    }

    #[test]
    fn invalid_assignment_target_is_reported() {
        let src = "fn f() { let mut x = 1; x = 2; }";
        let mut program = clean_hir(src);
        let HirItemKind::Fn(f) = &mut program.items[0].kind else {
            unreachable!()
        };
        let HirStmtKind::Expr(assign) = &mut f.body.stmts[1].kind else {
            unreachable!()
        };
        // Replace the assignment target with a literal, which is not a
        // place expression.
        let HirExprKind::Assign { target, .. } = &mut assign.kind else {
            unreachable!()
        };
        **target = HirExpr {
            kind: HirExprKind::Int,
            span: text_span(src, "2"),
            ty: assign.ty,
        };
        let errors = lower(&program).unwrap_err();
        assert_eq!(errors.len(), 1, "errors: {errors:?}");
        assert_eq!(errors[0].kind(), MirErrorKind::InvalidAssignmentTarget);
        assert_eq!(errors[0].code(), "E-M05");
    }
}
