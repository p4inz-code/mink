//! Optimized MIR → backend-instruction lowering.
//!
//! The [`lower`] entry point walks the optimized [`MirProgram`] once and
//! produces the target-independent [`BProgram`] that emitters consume. It
//! never re-runs name resolution, type checking, or MIR analysis: it only
//! consumes the answers MIR already carries — classified types, locals,
//! operands, and control flow.
//!
//! The first native subset deliberately supports a small scalar core:
//!
//! - **types** — `Int` (64-bit), `Bool`, `Range<Int>` as a two-word value,
//!   and unit (a function that produces no value);
//! - **values** — integer and boolean literals (decoded from the source
//!   text; the backend is the first stage to decode literal values),
//!   local loads, module-binding loads and stores;
//! - **operations** — arithmetic (`+ - * / %`), shifts (`<< >>`), bitwise
//!   (`& ^ | ~`), comparisons and equality (`< <= > >= == !=`), logical
//!   (`&& || !`), negation (`-`), range construction and iteration, and
//!   direct function calls;
//! - **control flow** — `if`/`else`, `while`, `for` over ranges, `loop`,
//!   `break`, `continue`, and `return`.
//!
//! Everything else — floating-point, strings, characters, `null`, member
//! and index places, function values, module bindings that need runtime
//! initialization — is **rejected with a structured error**
//! ([`BackendError`], `E-B01`…) instead of being silently miscompiled.
//! Lowering reports every independent problem in deterministic source
//! order; a single error keeps the program from reaching an emitter.
//!
//! Lowering is deterministic (source order) and defensive: statements that
//! touch a value whose type is unsupported are skipped, because their error
//! was already reported when the value's type was classified, so one root
//! cause never cascades into a swarm of diagnostics.

use std::collections::HashMap;

use crate::mir::{
    MirConstantKind, MirFn, MirItemKind, MirOperand, MirOperandKind, MirProgram, MirRvalueKind,
    MirStatic, MirStmtKind, MirTargetKind, MirTerminator,
};
use crate::semantics::SymbolId;
use crate::source::{SourceMap, Span};
use crate::typecheck::{TypeId, TypeKind};

use super::error::BackendError;
use super::ir::{
    BBlock, BFunction, BInst, BInstKind, BLocal, BOperand, BProgram, BStatic, BTerminator, BType,
    LowerResult, RuntimeService,
};

/// The resolved target of a call: a user function or an embedded runtime
/// service.
enum Callee {
    /// A user function, by index into [`BProgram::functions`].
    Function(usize),
    /// A runtime service (`rt_*` intrinsic).
    Runtime(RuntimeService),
}

/// Lowers an optimized [`MirProgram`] into a [`BProgram`].
///
/// Returns the lowered program, or every [`BackendError`] collected in
/// deterministic order when the input contains constructs outside the
/// native subset (or is internally inconsistent).
pub(crate) fn lower(program: &MirProgram, sources: &SourceMap) -> LowerResult {
    let mut lowerer = Lowerer::new(program, sources);
    lowerer.run();
    if lowerer.errors.is_empty() {
        Ok(BProgram {
            functions: lowerer.functions,
            statics: lowerer.statics,
        })
    } else {
        Err(lowerer.errors)
    }
}

/// The program-wide lowering traversal.
struct Lowerer<'a> {
    program: &'a MirProgram,
    sources: &'a SourceMap,
    /// Symbol → function index, for `Static` operands that name functions.
    fn_index: HashMap<SymbolId, usize>,
    /// Symbol → static slot, for `Static` operands that name module
    /// bindings. Slots cover every module binding (supported or not) so
    /// references always resolve; unsupported bindings are skipped.
    static_slots: HashMap<SymbolId, usize>,
    /// The classification of each static slot; `None` means the binding's
    /// type was already rejected.
    static_types: Vec<Option<BType>>,
    functions: Vec<BFunction>,
    statics: Vec<BStatic>,
    errors: Vec<BackendError>,
    /// Locals of the function currently being lowered (a copy of the MIR
    /// function's locals, classified, plus any lowering temporaries).
    fn_locals: Vec<BLocal>,
    /// Parallel to `fn_locals`: the classification of each local; `None`
    /// means the local's type was already rejected.
    fn_classified: Vec<Option<BType>>,
    /// The instruction buffer of the block currently being lowered.
    fn_insts: Vec<BInst>,
}

impl<'a> Lowerer<'a> {
    fn new(program: &'a MirProgram, sources: &'a SourceMap) -> Self {
        let mut fn_index = HashMap::new();
        let mut static_slots = HashMap::new();
        let mut fn_count = 0;
        let mut static_count = 0;
        for item in &program.items {
            match &item.kind {
                MirItemKind::Fn(f) => {
                    fn_index.insert(f.name.symbol, fn_count);
                    fn_count += 1;
                }
                MirItemKind::Let(binding) | MirItemKind::Const(binding) => {
                    static_slots.insert(binding.name.symbol, static_count);
                    static_count += 1;
                }
            }
        }
        Self {
            program,
            sources,
            fn_index,
            static_slots,
            static_types: Vec::new(),
            functions: Vec::new(),
            statics: Vec::new(),
            errors: Vec::new(),
            fn_locals: Vec::new(),
            fn_classified: Vec::new(),
            fn_insts: Vec::new(),
        }
    }

    fn run(&mut self) {
        // Pre-pass: classify every module binding so function bodies can
        // resolve (and skip) references to unsupported bindings.
        for item in &self.program.items {
            if let MirItemKind::Let(binding) | MirItemKind::Const(binding) = &item.kind {
                self.classify_static(binding);
            }
        }
        // Lower items in source order.
        for item in &self.program.items {
            match &item.kind {
                MirItemKind::Fn(f) => self.lower_fn(f),
                MirItemKind::Let(binding) | MirItemKind::Const(binding) => {
                    self.lower_static(binding);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Classification
    // ------------------------------------------------------------------

    /// Classifies a MIR type id into a backend value type.
    ///
    /// `Int`, `Bool`, and `Range<Int>` are representable; an unresolved
    /// inference type is unit (a value that is never meaningfully read);
    /// everything else (`Float`, `Str`, `Char`, `Null`, the error type,
    /// `Range` over another element, function types) is unsupported and
    /// classifies to `None`.
    fn classify(&self, ty: TypeId) -> Option<BType> {
        match self.program.types.kind(ty) {
            Some(TypeKind::Int) => Some(BType::Int),
            Some(TypeKind::Bool) => Some(BType::Bool),
            Some(TypeKind::Range(elem)) => match self.program.types.kind(*elem) {
                Some(TypeKind::Int) => Some(BType::Range),
                _ => None,
            },
            // `kind` follows resolved inference chains; an unresolved
            // variable is the only `Infer` that remains. Unit is the type
            // of intrinsics that produce no value.
            Some(TypeKind::Infer(_)) | Some(TypeKind::Unit) => Some(BType::Unit),
            Some(
                TypeKind::Error
                | TypeKind::Float
                | TypeKind::Str
                | TypeKind::Char
                | TypeKind::Null
                | TypeKind::Fn { .. },
            )
            | None => None,
        }
    }

    /// The display name of `ty`, for diagnostics.
    fn display(&self, ty: TypeId) -> String {
        self.program.types.display(ty)
    }

    /// Classifies a module binding's type and reserves its static slot.
    ///
    /// Unsupported binding types report one error and keep the slot marked
    /// `None`; function bodies that reference the binding skip their
    /// statements instead of re-reporting.
    fn classify_static(&mut self, s: &MirStatic) {
        let slot = self.static_slots[&s.name.symbol];
        match self.classify(s.ty) {
            Some(ty @ (BType::Int | BType::Bool)) => self.static_types.push(Some(ty)),
            _ => {
                self.errors.push(BackendError::unsupported_type(
                    s.span,
                    format!(
                        "module bindings of type `{}` are not supported",
                        self.display(s.ty)
                    ),
                ));
                self.static_types.push(None);
            }
        }
        debug_assert_eq!(self.static_types.len(), slot + 1);
    }

    /// The classified result type of a function, or a structured error.
    fn classify_result(&mut self, f: &MirFn) -> Option<BType> {
        let result = match self.program.types.kind(f.ty) {
            Some(TypeKind::Fn { result, .. }) => *result,
            _ => {
                // Defensive: function symbols always receive a `Fn` type.
                self.errors.push(BackendError::unsupported_type(
                    f.span,
                    "function has no function type",
                ));
                return None;
            }
        };
        match self.classify(result) {
            Some(BType::Range) => {
                self.errors.push(BackendError::unsupported_type(
                    f.span,
                    format!("cannot return a value of type `{}`", self.display(result)),
                ));
                None
            }
            Some(classified) => Some(classified),
            None => {
                self.errors.push(BackendError::unsupported_type(
                    f.span,
                    format!(
                        "the function's result type `{}` is not supported",
                        self.display(result)
                    ),
                ));
                None
            }
        }
    }

    // ------------------------------------------------------------------
    // Items
    // ------------------------------------------------------------------

    fn lower_static(&mut self, s: &MirStatic) {
        let slot = self.static_slots[&s.name.symbol];
        let Some(ty) = self.static_types[slot] else {
            // The binding's type was already rejected; skip it.
            return;
        };
        if !s.stmts.is_empty() || !s.locals.is_empty() {
            self.errors.push(BackendError::unsupported_static(
                s.span,
                "the native subset supports only module bindings initialized by a constant",
            ));
            return;
        }
        let value = match &s.value.kind {
            MirOperandKind::Constant(constant) => match self.decode_constant(constant) {
                Ok(value) => value,
                Err(error) => {
                    self.errors.push(error);
                    return;
                }
            },
            _ => {
                self.errors.push(BackendError::unsupported_static(
                    s.span,
                    format!(
                        "the initializer of `{}` references other values; only constants are supported",
                        s.name.name
                    ),
                ));
                return;
            }
        };
        self.statics.push(BStatic {
            name: s.name.name.clone(),
            symbol: s.name.symbol,
            mutable: s.mutable,
            ty,
            value,
            span: s.span,
        });
    }

    fn lower_fn(&mut self, f: &MirFn) {
        let Some(result) = self.classify_result(f) else {
            // The result-type error was reported; the body cannot be
            // emitted meaningfully.
            return;
        };
        // Classify every local up front. Unsupported locals report one
        // error each and classify to `None`; statements that touch them are
        // skipped during instruction lowering so the same root cause is not
        // reported twice.
        self.fn_locals.clear();
        self.fn_classified.clear();
        for local in &f.locals {
            match self.classify(local.ty) {
                Some(ty) => {
                    self.fn_classified.push(Some(ty));
                    self.fn_locals.push(BLocal {
                        name: local.name.clone(),
                        symbol: local.symbol,
                        ty,
                        mutable: local.mutable,
                        span: local.span,
                    });
                }
                None => {
                    self.errors.push(BackendError::unsupported_type(
                        local.span,
                        format!(
                            "the type `{}` is not supported by the native subset",
                            self.display(local.ty)
                        ),
                    ));
                    self.fn_classified.push(None);
                    self.fn_locals.push(BLocal {
                        name: local.name.clone(),
                        symbol: local.symbol,
                        ty: BType::Int,
                        mutable: local.mutable,
                        span: local.span,
                    });
                }
            }
        }
        let mut blocks = Vec::with_capacity(f.blocks.len());
        for block in &f.blocks {
            blocks.push(self.lower_block(block));
        }
        self.functions.push(BFunction {
            name: f.name.name.clone(),
            symbol: f.name.symbol,
            params: f.params.clone(),
            locals: std::mem::take(&mut self.fn_locals),
            blocks,
            result,
            span: f.span,
        });
        self.fn_classified.clear();
    }

    /// Allocates a lowering temporary of `ty` and returns its id.
    fn alloc_temp(&mut self, ty: BType, span: Span) -> crate::mir::LocalId {
        let id = crate::mir::LocalId::new(self.fn_classified.len() as u32);
        self.fn_classified.push(Some(ty));
        self.fn_locals.push(BLocal {
            name: String::new(),
            symbol: None,
            ty,
            mutable: false,
            span,
        });
        id
    }

    /// Appends an instruction to the block currently being lowered.
    fn push(&mut self, kind: BInstKind, span: Span) {
        self.fn_insts.push(BInst { kind, span });
    }

    fn lower_block(&mut self, block: &crate::mir::MirBlock) -> BBlock {
        for stmt in &block.stmts {
            let start = self.fn_insts.len();
            if self.lower_stmt(stmt).is_none() {
                // The statement was skipped or errored; drop any partial
                // temporary loads it emitted so the block stays clean.
                self.fn_insts.truncate(start);
            }
        }
        let terminator = self.lower_terminator(&block.terminator);
        BBlock {
            id: block.id,
            insts: std::mem::take(&mut self.fn_insts),
            terminator,
            span: block.span,
        }
    }

    /// Whether a local classifies to `None` (its unsupported-type error was
    /// already reported).
    fn local_is_unsupported(&self, id: crate::mir::LocalId) -> bool {
        self.fn_classified
            .get(id.raw() as usize)
            .copied()
            .flatten()
            .is_none()
    }

    /// Whether a static slot classifies to `None`.
    fn static_is_unsupported(&self, slot: usize) -> bool {
        self.static_types.get(slot).copied().flatten().is_none()
    }

    /// Whether an operand touches an unsupported local or module binding.
    fn operand_is_unsupported(&self, operand: &MirOperand) -> bool {
        match &operand.kind {
            MirOperandKind::Local(id) => self.local_is_unsupported(*id),
            MirOperandKind::Static(symbol) => self
                .static_slots
                .get(symbol)
                .is_some_and(|&slot| self.static_is_unsupported(slot)),
            MirOperandKind::Constant(_) => false,
        }
    }

    /// Whether an rvalue's operands touch an unsupported local or binding.
    fn rvalue_touches_unsupported(&self, rvalue: &crate::mir::MirRvalue) -> bool {
        match &rvalue.kind {
            MirRvalueKind::Use(operand)
            | MirRvalueKind::Unary { operand, .. }
            | MirRvalueKind::RangeNext { range: operand }
            | MirRvalueKind::RangeFinished { range: operand } => {
                self.operand_is_unsupported(operand)
            }
            MirRvalueKind::Binary { lhs, rhs, .. } => {
                self.operand_is_unsupported(lhs) || self.operand_is_unsupported(rhs)
            }
            MirRvalueKind::Call { callee, args } => {
                self.operand_is_unsupported(callee)
                    || args.iter().any(|arg| self.operand_is_unsupported(arg))
            }
            MirRvalueKind::Range { start, end, .. } => {
                self.operand_is_unsupported(start) || self.operand_is_unsupported(end)
            }
            MirRvalueKind::Member { .. } | MirRvalueKind::Index { .. } => false,
        }
    }

    /// Lowers one statement, pushing its instructions.
    ///
    /// Returns `None` when the statement was skipped (it touches an
    /// unsupported value whose error was already reported) or when its own
    /// error was reported; `Some(())` when it was emitted.
    fn lower_stmt(&mut self, stmt: &crate::mir::MirStmt) -> Option<()> {
        let (target, rvalue) = match &stmt.kind {
            MirStmtKind::Assign { target, rvalue } => (target, rvalue),
        };
        match &target.kind {
            MirTargetKind::Local(id) => {
                if self.local_is_unsupported(*id) || self.rvalue_touches_unsupported(rvalue) {
                    return None;
                }
                match self.lower_rvalue_into(*id, rvalue) {
                    Ok(()) => Some(()),
                    Err(error) => {
                        self.errors.push(error);
                        None
                    }
                }
            }
            MirTargetKind::Static(symbol) => {
                let Some(&slot) = self.static_slots.get(symbol) else {
                    self.errors.push(BackendError::invalid_backend_ir(
                        stmt.span,
                        "assignment target references an unknown module binding",
                    ));
                    return None;
                };
                if self.static_is_unsupported(slot) || self.rvalue_touches_unsupported(rvalue) {
                    return None;
                }
                match self.lower_rvalue_to_operand(rvalue) {
                    Ok(src) => {
                        self.push(
                            BInstKind::StoreStatic {
                                static_index: slot,
                                src,
                            },
                            stmt.span,
                        );
                        Some(())
                    }
                    Err(error) => {
                        self.errors.push(error);
                        None
                    }
                }
            }
            MirTargetKind::Member { .. } | MirTargetKind::Index { .. } => {
                self.errors.push(BackendError::unsupported_assign_target(
                    target.span,
                    "member and index assignment is not supported by the native subset",
                ));
                None
            }
        }
    }

    /// Lowers an rvalue into `target`, pushing the instruction.
    fn lower_rvalue_into(
        &mut self,
        target: crate::mir::LocalId,
        rvalue: &crate::mir::MirRvalue,
    ) -> Result<(), BackendError> {
        let kind = match &rvalue.kind {
            MirRvalueKind::Use(operand) => match &operand.kind {
                MirOperandKind::Local(src) => BInstKind::LoadLocal { target, src: *src },
                MirOperandKind::Constant(constant) => BInstKind::LoadConst {
                    target,
                    value: self.decode_constant(constant)?,
                },
                MirOperandKind::Static(symbol) => BInstKind::LoadStatic {
                    target,
                    static_index: self.resolve_static(*symbol, rvalue.span)?,
                },
            },
            MirRvalueKind::Unary { op, operand } => BInstKind::Unary {
                target,
                op: *op,
                src: self.eval_operand(operand)?,
            },
            MirRvalueKind::Binary { op, lhs, rhs } => BInstKind::Binary {
                target,
                op: *op,
                lhs: self.eval_operand(lhs)?,
                rhs: self.eval_operand(rhs)?,
            },
            MirRvalueKind::Call { callee, args } => {
                let mut lowered_args = Vec::with_capacity(args.len());
                for arg in args {
                    lowered_args.push(self.eval_operand(arg)?);
                }
                match self.resolve_callee(callee, rvalue.span)? {
                    Callee::Function(callee_index) => BInstKind::Call {
                        target,
                        callee: callee_index,
                        args: lowered_args,
                    },
                    Callee::Runtime(service) => BInstKind::RuntimeCall {
                        target,
                        service,
                        args: lowered_args,
                    },
                }
            }
            MirRvalueKind::Range {
                inclusive,
                start,
                end,
            } => BInstKind::RangeInit {
                target,
                start: self.eval_operand(start)?,
                end: self.eval_operand(end)?,
                inclusive: *inclusive,
            },
            MirRvalueKind::RangeNext { range } => BInstKind::RangeNext {
                target,
                range: self.range_slot(range, rvalue.span)?,
            },
            MirRvalueKind::RangeFinished { range } => BInstKind::RangeFinished {
                target,
                range: self.range_slot(range, rvalue.span)?,
            },
            MirRvalueKind::Member { .. } => {
                return Err(BackendError::unsupported_rvalue(
                    rvalue.span,
                    "member access is not supported by the native subset",
                ));
            }
            MirRvalueKind::Index { .. } => {
                return Err(BackendError::unsupported_rvalue(
                    rvalue.span,
                    "indexing is not supported by the native subset",
                ));
            }
        };
        self.push(kind, rvalue.span);
        Ok(())
    }

    /// Lowers an rvalue into an operand for the store-to-static path:
    /// operand-shaped rvalues evaluate directly; other rvalues are computed
    /// into a temporary slot and the temporary's load is returned.
    fn lower_rvalue_to_operand(
        &mut self,
        rvalue: &crate::mir::MirRvalue,
    ) -> Result<BOperand, BackendError> {
        if let MirRvalueKind::Use(operand) = &rvalue.kind {
            return self.eval_operand(operand);
        }
        let ty = match self.classify(rvalue.ty) {
            Some(ty) => ty,
            // Defensive: a clean pipeline never stores an
            // unsupported-typed value (the type checker rejects the
            // assignment); report it.
            None => {
                return Err(BackendError::unsupported_type(
                    rvalue.span,
                    format!(
                        "the value's type `{}` is not supported by the native subset",
                        self.display(rvalue.ty)
                    ),
                ));
            }
        };
        let temp = self.alloc_temp(ty, rvalue.span);
        self.lower_rvalue_into(temp, rvalue)?;
        Ok(BOperand::Local(temp))
    }

    /// Resolves a `Static` operand to a static slot, rejecting function
    /// references.
    fn resolve_static(&mut self, symbol: SymbolId, span: Span) -> Result<usize, BackendError> {
        if let Some(&slot) = self.static_slots.get(&symbol) {
            return Ok(slot);
        }
        if self.fn_index.contains_key(&symbol) {
            return Err(BackendError::unsupported_rvalue(
                span,
                "a function cannot be used as a value",
            ));
        }
        Err(BackendError::invalid_backend_ir(
            span,
            "operand references an unknown module item",
        ))
    }

    /// Resolves a call's callee operand to a user function index or an
    /// embedded runtime service.
    fn resolve_callee(&mut self, callee: &MirOperand, span: Span) -> Result<Callee, BackendError> {
        match &callee.kind {
            MirOperandKind::Static(symbol) => {
                if let Some(&index) = self.fn_index.get(symbol) {
                    return Ok(Callee::Function(index));
                }
                // A predeclared runtime intrinsic: the program carries the
                // symbol → intrinsic mapping; map the intrinsic to its
                // machine service.
                if let Some((_, intrinsic)) = self
                    .program
                    .intrinsic_symbols
                    .iter()
                    .find(|(symbol_id, _)| symbol_id == symbol)
                {
                    let name = intrinsic.get().name;
                    return Self::runtime_service(name)
                        .map(Callee::Runtime)
                        .ok_or_else(|| {
                            BackendError::invalid_backend_ir(
                                span,
                                format!("the intrinsic `{name}` has no runtime service mapping"),
                            )
                        });
                }
                if self.static_slots.contains_key(symbol) {
                    return Err(BackendError::unsupported_callee(
                        span,
                        "the callee is a module binding, not a function",
                    ));
                }
                Err(BackendError::invalid_backend_ir(
                    span,
                    "callee references an unknown module item",
                ))
            }
            _ => Err(BackendError::unsupported_callee(
                span,
                "the native subset supports only direct calls to module-level functions",
            )),
        }
    }

    /// The runtime service an intrinsic name maps to, if any. Only the
    /// callable subset of services is exposed to generated code.
    fn runtime_service(name: &str) -> Option<RuntimeService> {
        let service = match name {
            "rt_alloc" => RuntimeService::Alloc,
            "rt_free" => RuntimeService::Free,
            "rt_mem_load" => RuntimeService::MemLoad,
            "rt_mem_store" => RuntimeService::MemStore,
            "rt_exit" => RuntimeService::Exit,
            "rt_print_int" => RuntimeService::PrintInt,
            _ => return None,
        };
        debug_assert!(service.is_callable());
        Some(service)
    }

    /// The slot of a range being iterated: range iteration reads a local
    /// slot, so only local operands are supported here.
    fn range_slot(
        &mut self,
        range: &MirOperand,
        span: Span,
    ) -> Result<crate::mir::LocalId, BackendError> {
        match &range.kind {
            MirOperandKind::Local(id) => Ok(*id),
            _ => Err(BackendError::unsupported_rvalue(
                span,
                "range iteration must read a local range value",
            )),
        }
    }

    /// Evaluates an operand into its backend form, decoding constants and
    /// materializing module-binding reads.
    ///
    /// A module-binding operand is read through an explicit load into a
    /// temporary slot.
    fn eval_operand(&mut self, operand: &MirOperand) -> Result<BOperand, BackendError> {
        match &operand.kind {
            MirOperandKind::Local(id) => Ok(BOperand::Local(*id)),
            MirOperandKind::Constant(constant) => {
                Ok(BOperand::Const(self.decode_constant(constant)?))
            }
            MirOperandKind::Static(symbol) => {
                let slot = self.resolve_static(*symbol, operand.span)?;
                let ty = self.static_types[slot]
                    .expect("unsupported statics are skipped before operand evaluation");
                let temp = self.alloc_temp(ty, operand.span);
                self.push(
                    BInstKind::LoadStatic {
                        target: temp,
                        static_index: slot,
                    },
                    operand.span,
                );
                Ok(BOperand::Local(temp))
            }
        }
    }

    /// Decodes a literal constant into a 64-bit machine value.
    ///
    /// Integers are decoded from the literal's source text (the backend is
    /// the first stage to decode literal values; the text is recovered via
    /// the source map). Booleans carry their value directly. Every other
    /// literal kind is rejected — and, since unsupported literal kinds
    /// always pair with unsupported local types, this path is defensive: a
    /// clean pipeline reports the type error first.
    fn decode_constant(&self, constant: &crate::mir::MirConstant) -> Result<i64, BackendError> {
        match constant.kind {
            MirConstantKind::Bool(value) => Ok(i64::from(value)),
            MirConstantKind::Int => self.decode_int(constant.span),
            MirConstantKind::Float
            | MirConstantKind::Str
            | MirConstantKind::Char
            | MirConstantKind::Null => Err(BackendError::unsupported_constant(
                constant.span,
                "only integer and boolean literals are supported by the native subset",
            )),
        }
    }

    /// Decodes an integer literal's source text into a 64-bit two's
    /// complement value, supporting decimal, `0x`/`0o`/`0b` radix prefixes,
    /// and `_` digit separators (the lexer's literal forms). Values that do
    /// not fit wrap, matching the 64-bit integer model.
    fn decode_int(&self, span: Span) -> Result<i64, BackendError> {
        let Some(file) = self.sources.get(span.file()) else {
            return Err(BackendError::decode_error(
                span,
                "no source file for the literal",
            ));
        };
        let Some(text) = file.span_text(span) else {
            return Err(BackendError::decode_error(
                span,
                "literal text is unavailable",
            ));
        };
        let (radix, digits) =
            if let Some(rest) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
                (16, rest)
            } else if let Some(rest) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
                (8, rest)
            } else if let Some(rest) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
                (2, rest)
            } else {
                (10, text)
            };
        let mut value: u64 = 0;
        for byte in digits.bytes() {
            if byte == b'_' {
                continue;
            }
            let digit = match byte {
                b'0'..=b'9' => u64::from(byte - b'0'),
                b'a'..=b'f' => u64::from(byte - b'a' + 10),
                b'A'..=b'F' => u64::from(byte - b'A' + 10),
                // A clean pipeline never produces an out-of-radix character
                // (the lexer would have rejected the literal).
                _ => {
                    return Err(BackendError::decode_error(
                        span,
                        "literal contains a character that is not a digit of its radix",
                    ));
                }
            };
            if digit >= radix as u64 {
                return Err(BackendError::decode_error(
                    span,
                    "literal contains a digit outside its radix",
                ));
            }
            value = value.wrapping_mul(radix as u64).wrapping_add(digit);
        }
        Ok(value as i64)
    }

    /// Lowers a block's terminator, pushing any temporary loads its operand
    /// evaluation needs.
    fn lower_terminator(&mut self, terminator: &MirTerminator) -> BTerminator {
        match terminator {
            MirTerminator::Return { value, span } => {
                let value = match value {
                    Some(operand) if !self.operand_is_unsupported(operand) => {
                        match self.eval_operand(operand) {
                            Ok(operand) => Some(operand),
                            Err(error) => {
                                self.errors.push(error);
                                None
                            }
                        }
                    }
                    // A bare return, or a return of an unsupported value
                    // whose error was already reported.
                    _ => None,
                };
                BTerminator::Return { value, span: *span }
            }
            MirTerminator::Jump { target, span } => BTerminator::Jump {
                target: *target,
                span: *span,
            },
            MirTerminator::Branch {
                cond,
                then_block,
                else_block,
                span,
            } => {
                let cond = if self.operand_is_unsupported(cond) {
                    BOperand::Const(0)
                } else {
                    match self.eval_operand(cond) {
                        Ok(operand) => operand,
                        Err(error) => {
                            self.errors.push(error);
                            BOperand::Const(0)
                        }
                    }
                };
                BTerminator::Branch {
                    cond,
                    then_block: *then_block,
                    else_block: *else_block,
                    span: *span,
                }
            }
        }
    }
}
