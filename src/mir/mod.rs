//! MIR: the control-flow-oriented mid-level intermediate representation.
//!
//! MIR is the second compiler IR layer, produced by lowering the
//! [`HirProgram`](crate::hir::HirProgram) (see [`lower`]). Where HIR mirrors
//! the source's structural shape (nested blocks, one node per construct),
//! MIR is **linear and control-flow-explicit**: every function is a
//! directed graph of basic blocks, each block is an ordered list of
//! statements ending in exactly one terminator, and every construct —
//! `if`/`else`, `while`, `for`, `loop`, `break`, `continue`, `return` —
//! has been lowered into jumps, branches, and returns.
//!
//! The design follows the boundary established by HIR:
//!
//! - **no re-analysis** — lowering never re-runs name resolution or type
//!   checking; it only consumes the answers HIR already carries;
//! - **no duplicated systems** — MIR references [`TypeId`]s from the HIR's
//!   cloned [`TypeTable`] (re-cloned here so the program is self-contained)
//!   and [`SymbolId`]s where useful (module-item references, locals bound to
//!   source declarations); it defines its own local/block identity;
//! - **exact spans** — every node preserves the source span of the
//!   construct it was lowered from;
//! - **deterministic output** — items, locals, and blocks are produced in
//!   source order, so identical input always yields identical MIR;
//! - **structured failures** — internal inconsistencies are reported as
//!   [`MirError`]s (`E-M01`…`E-M11`) instead of panicking (see
//!   [`validate`] and `docs/implementation/MIR_IMPLEMENTATION.md`).
//!
//! The pipeline continues from HIR:
//!
//! ```text
//! HIR → MIR lowering → MIR validation → MIR optimization → future backend
//! ```
//!
//! MIR is not executable: no code generation, runtime, or backend exists
//! yet, and `mink build` remains unimplemented.

mod error;
mod lower;
mod optimize;
mod validate;

use crate::ast::{BinaryOp, UnaryOp};
use crate::hir::HirProgram;
use crate::semantics::SymbolId;
use crate::source::Span;
use crate::typecheck::{TypeId, TypeTable};

pub use error::{MirError, MirErrorKind};
pub use optimize::{CfgSimplify, ConstFold, CopyProp, DeadCodeElim, MirPass, UnreachableElim};

/// Stable identity of a local value within one [`MirFn`].
///
/// A local is a named slot for a value: a function parameter, a user
/// binding (`let`/`const`), a `for` loop variable, or an anonymous
/// temporary produced during expression lowering. Ids are assigned
/// sequentially as locals are declared — parameters first, then body
/// declarations and temporaries in traversal order — and remain valid for
/// the lifetime of the function that created them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalId(u32);

impl LocalId {
    /// The raw numeric value of this id.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Creates an id from its raw numeric value.
    ///
    /// Ids should normally be produced by MIR lowering; constructing one
    /// directly is only useful for tests and tooling that manages MIR
    /// itself.
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Stable identity of a basic block within one [`MirFn`].
///
/// Blocks are numbered in creation order, and every function's block list
/// is ordered so the block at index `i` has id `i`; the entry block is
/// always block `0` (see [`validate`]). Ids remain valid for the lifetime
/// of the function that created them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(u32);

impl BlockId {
    /// The raw numeric value of this id.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Creates an id from its raw numeric value.
    ///
    /// Ids should normally be produced by MIR lowering; constructing one
    /// directly is only useful for tests and tooling that manages MIR
    /// itself.
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// A lowered MINK program: control-flow items in source order, plus the
/// type table every [`TypeId`] in the program refers to.
///
/// Like the HIR it was lowered from, the MIR is self-contained: it owns all
/// of its data and its own (cloned) type table, so it remains valid after
/// the HIR is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirProgram {
    /// The top-level items, in source order.
    pub items: Vec<MirItem>,
    /// The type table backing every [`TypeId`] in this program. It is a
    /// clone of the HIR's table, possibly extended with the `Bool` type
    /// loop lowering needs, so the MIR is self-contained.
    pub types: TypeTable,
    /// The predeclared runtime intrinsics: symbol → stable intrinsic id.
    /// Intrinsics are referenced as module-item-style `Static` operands;
    /// the backend maps their calls to embedded runtime services.
    pub intrinsic_symbols: Vec<(crate::semantics::SymbolId, crate::runtime::IntrinsicId)>,
}

/// A top-level declaration lowered to MIR: a function, a `let` binding, or
/// a `const` binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirItem {
    /// The kind of declaration.
    pub kind: MirItemKind,
    /// Span covering the whole item.
    pub span: Span,
}

/// The kind of a top-level [`MirItem`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirItemKind {
    /// A `fn` function declaration: a control-flow graph.
    Fn(MirFn),
    /// A module-level `let` binding.
    Let(MirStatic),
    /// A module-level `const` binding.
    Const(MirStatic),
}

/// A lowered function: a control-flow graph of basic blocks over a list of
/// locals.
///
/// The first `params.len()` locals are the parameters, in declaration
/// order; the remaining locals are body bindings and lowering temporaries.
/// The entry block is always block `0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirFn {
    /// The function's name, resolved to its symbol.
    pub name: MirIdent,
    /// The parameters as local ids: exactly the first locals, in
    /// declaration order.
    pub params: Vec<LocalId>,
    /// Every local value of the function: parameters first, then body
    /// bindings and temporaries in lowering order.
    pub locals: Vec<MirLocal>,
    /// The basic blocks, ordered by id: the block at index `i` has id `i`.
    /// The entry block is block `0`.
    pub blocks: Vec<MirBlock>,
    /// Span covering the whole `fn` item.
    pub span: Span,
    /// The function's `Fn` type.
    pub ty: TypeId,
}

impl MirFn {
    /// The id of the entry block (`0` by construction).
    pub fn entry(&self) -> BlockId {
        BlockId::new(0)
    }

    /// The block registered under `id`, if any.
    pub fn block(&self, id: BlockId) -> Option<&MirBlock> {
        self.blocks.get(id.raw() as usize)
    }

    /// The local registered under `id`, if any.
    pub fn local(&self, id: LocalId) -> Option<&MirLocal> {
        self.locals.get(id.raw() as usize)
    }
}

/// A local value: a parameter, a binding, a loop variable, or a lowering
/// temporary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirLocal {
    /// The binding's source name; empty for anonymous temporaries.
    pub name: String,
    /// The source declaration's symbol, for bindings; `None` for
    /// temporaries.
    pub symbol: Option<SymbolId>,
    /// The local's canonical type.
    pub ty: TypeId,
    /// Whether the local's slot is written by user code or loop lowering.
    /// Parameters, `let` bindings, and `const` bindings are immutable in
    /// the source; `let mut` bindings are mutable; `for` loop variables are
    /// marked mutable because the loop machinery writes them each
    /// iteration (source-level reassignment is still rejected by semantic
    /// analysis).
    pub mutable: bool,
    /// Span of the declaration (or of the expression that produced a
    /// temporary).
    pub span: Span,
}

/// A basic block: an ordered list of statements followed by exactly one
/// terminator.
///
/// The "exactly one terminator" invariant holds **by construction**: blocks
/// are only ever produced by the lowering builder, which terminates each
/// block exactly once. Validation ([`validate`]) checks every terminator's
/// references, so malformed hand-built MIR fails cleanly instead of
/// panicking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirBlock {
    /// This block's identity; always equal to its index in the function's
    /// block list.
    pub id: BlockId,
    /// The statements, in order.
    pub stmts: Vec<MirStmt>,
    /// The block's terminator — exactly one, always present.
    pub terminator: MirTerminator,
    /// Span of the source construct this block was lowered from.
    pub span: Span,
}

/// How a basic block ends.
///
/// Terminators cover the three required forms — return, unconditional jump,
/// and conditional branch — and are the only place control flow leaves a
/// block. `break` and `continue` lower to jumps (to the enclosing loop's
/// exit and continue targets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirTerminator {
    /// `return;` or `return expr;`. A bare return has no value.
    Return {
        /// The returned value, when present.
        value: Option<MirOperand>,
        /// Span of the `return` statement.
        span: Span,
    },
    /// An unconditional jump to another block.
    Jump {
        /// The target block.
        target: BlockId,
        /// Span of the construct that produced this jump.
        span: Span,
    },
    /// A conditional branch: continue in `then_block` when `cond` is true,
    /// in `else_block` otherwise.
    Branch {
        /// The condition value.
        cond: MirOperand,
        /// The block entered when `cond` is true.
        then_block: BlockId,
        /// The block entered when `cond` is false.
        else_block: BlockId,
        /// Span of the construct that produced this branch.
        span: Span,
    },
}

/// A single statement inside a basic block.
///
/// Statements have no control flow: they compute a value and store it. The
/// current language needs exactly one statement form — assignment into a
/// target — which covers `let`/`const` bindings, plain and compound
/// assignments, and temporary-producing expression evaluation; the enum is
/// kept so later milestones (storage markers, debug info) can extend it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirStmt {
    /// The kind of statement.
    pub kind: MirStmtKind,
    /// Span of the source statement (or expression) this was lowered from.
    pub span: Span,
}

/// The kind of a [`MirStmt`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirStmtKind {
    /// Compute `rvalue` and store it into `target`.
    Assign {
        /// The storage target.
        target: MirTarget,
        /// The value being computed.
        rvalue: MirRvalue,
    },
}

/// A storage target for assignment: a local, module-level storage, or a
/// structural member/index place.
///
/// Member and index targets are represented structurally — the base is the
/// evaluated base *value* — because the memory-model milestone that defines
/// their place semantics does not exist yet; they are preserved so valid
/// programs (which the front end allows) lower cleanly. The same applies to
/// the `Member`/`Index` rvalue forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirTarget {
    /// The kind of target.
    pub kind: MirTargetKind,
    /// Span of the target expression.
    pub span: Span,
    /// The target's type.
    pub ty: TypeId,
}

/// The kind of a [`MirTarget`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirTargetKind {
    /// A local slot.
    Local(LocalId),
    /// Module-level storage (a module-scope `let`, which is assignable when
    /// declared `mut`). Referenced by its declaration's [`SymbolId`].
    Static(SymbolId),
    /// A member place: `base.member` whose base is a plain local (a
    /// single-step place). The base is the evaluated local operand.
    Member {
        /// The evaluated base value.
        base: MirOperand,
        /// The member name.
        member: MirName,
    },
    /// An index place: `base[index]` whose base is a plain local (a
    /// single-step place).
    Index {
        /// The evaluated base value.
        base: MirOperand,
        /// The evaluated index value.
        index: MirOperand,
    },
    /// A deref place: `*r` (session 16) — the storage addressed by
    /// reference `r`. Reads load through the address (`Deref` rvalue);
    /// writes store through it.
    Deref {
        /// The evaluated reference value (a single-word address).
        operand: MirOperand,
    },
    /// A multi-step storage place: `root` (a local holding the outermost
    /// value) plus a path of field/index steps to the addressed element.
    /// Chains are kept structurally so an assignment reaches the root
    /// value instead of a temporary copy of an intermediate member/index
    /// result (the backend resolves each step's byte offset/stride from
    /// the deterministic layout).
    Place {
        /// The local holding the outermost value of the chain.
        root: LocalId,
        /// The steps from the root to the addressed element, in source
        /// order (outermost first).
        steps: Vec<MirPlaceStep>,
    },
}

/// One step of a multi-step storage place ([`MirTargetKind::Place`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirPlaceStep {
    /// The kind of step.
    pub kind: MirPlaceStepKind,
    /// Span of the member/index expression this step was lowered from.
    pub span: Span,
}

/// The kind of a [`MirPlaceStep`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirPlaceStepKind {
    /// A field selection: `value.field`.
    Field(MirName),
    /// An index selection: `value[index]`, with the index evaluated to an
    /// operand.
    Index(MirOperand),
}

/// A value computed by a statement: an operand read, a unary/binary
/// operation, a call, a range construction, or a range-iteration step.
///
/// Every rvalue carries the source span and the canonical type of the
/// expression it was lowered from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirRvalue {
    /// The kind of rvalue.
    pub kind: MirRvalueKind,
    /// Span of the source expression.
    pub span: Span,
    /// The rvalue's canonical type.
    pub ty: TypeId,
}

/// The kind of a [`MirRvalue`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirRvalueKind {
    /// Read a value: a local load, a literal constant, or a module-item
    /// reference.
    Use(MirOperand),
    /// A prefix unary operation: `-x`, `!x`, `~x`.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// The operand.
        operand: MirOperand,
    },
    /// An infix binary operation.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// The left operand.
        lhs: MirOperand,
        /// The right operand.
        rhs: MirOperand,
    },
    /// A function call: `callee(args)`.
    Call {
        /// The called value (typically a module-item function reference).
        callee: MirOperand,
        /// The arguments, in source order.
        args: Vec<MirOperand>,
    },
    /// A range construction: `start..end` or `start..=end`.
    Range {
        /// Whether the range is inclusive (`..=`).
        inclusive: bool,
        /// The range start.
        start: MirOperand,
        /// The range end.
        end: MirOperand,
    },
    /// The next element of a range value (used by `for` loop lowering).
    /// Iteration order and the effect on an exhausted range are backend
    /// semantics.
    RangeNext {
        /// The range value being iterated.
        range: MirOperand,
    },
    /// Whether a range value's iteration is complete (used by `for` loop
    /// lowering): `Bool` — true when there are no more elements.
    RangeFinished {
        /// The range value being iterated.
        range: MirOperand,
    },
    /// A member load: `base.member`.
    Member {
        /// The evaluated base value.
        base: MirOperand,
        /// The member name.
        member: MirName,
    },
    /// An index load: `base[index]`.
    Index {
        /// The evaluated base value.
        base: MirOperand,
        /// The evaluated index value.
        index: MirOperand,
    },
    /// A reference formation (session 16): `&place` / `&mut place`. The
    /// value is the machine address of the place rooted at `root`'s slot,
    /// walked by `steps` (a field step is a static byte offset; an index
    /// step is bounds-checked at execution, `E-R10`). Mutability is a
    /// compile-time concept carried for IR fidelity.
    RefAddr {
        /// Whether the borrow is mutable (`&mut`).
        mutable: bool,
        /// The local holding the outermost value of the borrowed place.
        root: LocalId,
        /// The steps from the root to the borrowed element, in source
        /// order (outermost first).
        steps: Vec<MirPlaceStep>,
    },
    /// A deref read (session 16): `*r` loads `size` bytes through the
    /// reference's address.
    Deref {
        /// The evaluated reference value.
        operand: MirOperand,
    },
    /// A struct literal: `Name { field: value, ... }`. Every field value is
    /// already evaluated to an operand; the materialized struct value is
    /// stored into the rvalue's target local, one field at a time (the
    /// backend resolves field offsets from the struct's deterministic
    /// layout).
    StructLit {
        /// The field initializers, in source order: (name, value).
        fields: Vec<(MirName, MirOperand)>,
    },
    /// An array literal: `[elem, ...]`. Every element is already evaluated
    /// to an operand; the materialized array value is stored into the
    /// rvalue's target local element by element.
    ArrayLit {
        /// The elements, in source order.
        elems: Vec<MirOperand>,
    },
}

/// An operand: the leaf of an expression. Operands are either local loads,
/// literal constants, or references to module-level items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirOperand {
    /// The kind of operand.
    pub kind: MirOperandKind,
    /// Span of the source expression.
    pub span: Span,
    /// The operand's canonical type.
    pub ty: TypeId,
}

/// The kind of a [`MirOperand`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirOperandKind {
    /// A local load.
    Local(LocalId),
    /// A literal constant.
    Constant(MirConstant),
    /// A reference to a module-level item — a function, `let`, or `const`
    /// declaration — by its declaration's [`SymbolId`].
    Static(SymbolId),
}

/// A literal constant.
///
/// Like the AST and HIR, literal *values* are not decoded into the IR: the
/// raw source text is recovered from the constant's span via the source
/// map. Decoding belongs to a later milestone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirConstant {
    /// The kind of literal.
    pub kind: MirConstantKind,
    /// Span of the literal token.
    pub span: Span,
    /// The literal's type.
    pub ty: TypeId,
}

/// The kind of a [`MirConstant`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirConstantKind {
    /// An integer literal.
    Int,
    /// A floating-point literal.
    Float,
    /// A string literal.
    Str,
    /// A character literal.
    Char,
    /// The boolean literal `true` or `false`.
    Bool(bool),
    /// The `null` literal.
    Null,
    /// An enum variant constant (session 17): the variant's discriminant
    /// (assigned in declaration order, starting at 0). Unlike the other
    /// constants, the value is computed by the compiler (from the enum's
    /// variant table), not decoded from source text; the constant's type
    /// is the enum type.
    Enum {
        /// The variant's discriminant.
        variant: u32,
    },
}

/// A module-level `let` or `const` binding lowered to MIR.
///
/// The initializer is evaluated into `locals`/`stmts` (temporaries and the
/// statements that compute them) and its final value is `value` — an
/// operand that may reference other module items. When module-scope
/// initialization runs is a backend concern (see
/// `docs/implementation/MIR_IMPLEMENTATION.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirStatic {
    /// The bound name and symbol.
    pub name: MirIdent,
    /// Whether the slot is mutable (`let mut`). `const` bindings are never
    /// mutable.
    pub mutable: bool,
    /// Temporaries used while evaluating the initializer.
    pub locals: Vec<MirLocal>,
    /// The statements that compute the initializer's value.
    pub stmts: Vec<MirStmt>,
    /// The initializer's final value.
    pub value: MirOperand,
    /// Span of the whole binding.
    pub span: Span,
    /// The binding's type.
    pub ty: TypeId,
}

/// A resolved identifier: source spelling, exact span, the symbol it refers
/// to, and the symbol's canonical type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirIdent {
    /// The identifier's exact source spelling.
    pub name: String,
    /// Span of the identifier token.
    pub span: Span,
    /// The symbol this identifier refers to.
    pub symbol: SymbolId,
    /// The symbol's canonical type.
    pub ty: TypeId,
}

/// A plain name that is not a symbol reference (for example a member name;
/// member symbols arrive with user-defined types in a later milestone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirName {
    /// The name's exact source spelling.
    pub name: String,
    /// Span of the name token.
    pub span: Span,
}

/// Lowers a [`HirProgram`] into a [`MirProgram`] of explicit control flow.
///
/// The entry point of the HIR → MIR boundary: it walks the HIR once,
/// consuming only the answers HIR already carries (never re-running name
/// resolution or type checking), and produces the block graphs, locals,
/// statements, and terminators described in the module documentation. For a
/// program that passed through a clean front end it always succeeds;
/// internal inconsistencies are collected as [`MirError`]s (`E-M01`…`E-M06`)
/// and returned instead of the program.
pub fn lower(program: &HirProgram) -> Result<MirProgram, Vec<MirError>> {
    lower::lower(program)
}

/// Validates the structural integrity of a [`MirProgram`].
///
/// Checks that every terminator references a block that exists, every
/// statement/operand references a local that exists, every type reference
/// resolves in the program's type table, blocks are ordered by id
/// (deterministic ordering, entry block first), and parameter locals are
/// the first locals in order. Returns every problem found as an
/// [`MirError`] (`E-M07`…`E-M11`) instead of panicking.
///
/// Lowering always produces valid MIR, so this exists to defend the
/// pipeline and tooling against malformed hand-built or mutated programs.
pub fn validate(program: &MirProgram) -> Result<(), Vec<MirError>> {
    validate::validate(program)
}

/// Optimizes a [`MirProgram`] through the standard pass pipeline.
///
/// Runs the session-10 passes — constant folding, copy propagation, trivial
/// CFG simplification, unreachable-block elimination, and dead-code
/// elimination — to a fixpoint, validating the program before the first
/// pass and after every pass. Malformed input (or a pass that breaks a
/// structural invariant) is reported as [`MirError`]s (`E-M07`…`E-M11`)
/// instead of panicking; the returned program is structurally valid and
/// behavior-preserving (see `docs/implementation/OPTIMIZATION_IMPLEMENTATION.md`).
pub fn optimize(program: &MirProgram) -> Result<MirProgram, Vec<MirError>> {
    optimize::optimize(program)
}
