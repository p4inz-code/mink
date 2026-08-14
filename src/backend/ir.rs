//! The target-independent backend instruction representation.
//!
//! This is the third IR layer and the input every target emitter consumes.
//! Where MIR is control-flow-oriented but still speaks in source-level
//! constructs (rvalues, operands that may reference module items), the
//! backend IR is a **canonical instruction stream** with three properties
//! that make it a clean code-generation boundary:
//!
//! - **values live in explicit slots** — every instruction writes its
//!   result into a named [`LocalId`]; operands are either local loads or
//!   machine constants ([`BOperand`]), so an emitter never re-evaluates an
//!   expression;
//! - **storage is explicit** — module bindings ([`BStatic`]) are separate
//!   from locals, and loads/stores between them are instructions;
//! - **types are classified** — the MIR type table is replaced by a small
//!   closed set of value types ([`BType`]) that a machine ABI can represent;
//!   everything outside that set is rejected at lowering with a structured
//!   error instead of reaching an emitter.
//!
//! Every node preserves the source [`Span`](crate::source::Span) of the
//! construct it was lowered from, and functions, statics, locals, and
//! blocks keep the deterministic source order of the MIR they came from, so
//! identical input always yields identical instructions
//! (see `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`).

use crate::ast::{BinaryOp, UnaryOp};
use crate::mir::{BlockId, LocalId};
use crate::semantics::SymbolId;
use crate::source::Span;

use super::error::BackendError;

/// A value type the backend can represent on a machine.
///
/// This is the closed classification of the MINK types the first native
/// subset supports: 64-bit integers, booleans (stored as `0`/`1`), and
/// integer ranges (stored as a two-word value). `Unit` is the type of a
/// function that produces no value (a bare `return;` or falling off the
/// end). Every other MINK type (`Float`, `Str`, `Char`, `Null`, unresolved
/// inference types) is rejected at lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BType {
    /// A 64-bit two's-complement integer.
    Int,
    /// A boolean: `0` (false) or `1` (true).
    Bool,
    /// A range of integers: a two-word value carrying the (normalized)
    /// exclusive end and the iteration cursor.
    Range,
    /// No value (a function that does not produce one).
    Unit,
}

impl BType {
    /// Whether values of this type occupy exactly one machine word.
    pub fn is_word_sized(self) -> bool {
        matches!(self, Self::Int | Self::Bool)
    }

    /// The number of 64-bit words a value of this type occupies in a stack
    /// slot or argument list.
    pub fn words(self) -> usize {
        match self {
            Self::Int | Self::Bool | Self::Unit => 1,
            Self::Range => 2,
        }
    }

    /// A human-readable name, used in diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Self::Int => "Int",
            Self::Bool => "Bool",
            Self::Range => "Range<Int>",
            Self::Unit => "unit",
        }
    }
}

/// A lowered backend program: functions and module bindings in source
/// order.
///
/// The program is self-contained: it owns every value its instructions
/// reference (locals are per-function, statics are owned here, and
/// function/static references are indices into the corresponding lists).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BProgram {
    /// The functions, in source order.
    pub functions: Vec<BFunction>,
    /// The module-level `let`/`const` bindings, in source order.
    pub statics: Vec<BStatic>,
}

/// A lowered function: a control-flow graph of blocks over a list of
/// typed locals.
///
/// The first `params.len()` locals are the parameters, in declaration
/// order. The entry block is always block `0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BFunction {
    /// The function's source name.
    pub name: String,
    /// The function's declaration symbol.
    pub symbol: SymbolId,
    /// The parameters as local ids: exactly the first locals, in
    /// declaration order.
    pub params: Vec<LocalId>,
    /// Every local value of the function: parameters first, then body
    /// bindings and lowering temporaries.
    pub locals: Vec<BLocal>,
    /// The basic blocks, ordered by id: the block at index `i` has id `i`.
    /// The entry block is block `0`.
    pub blocks: Vec<BBlock>,
    /// The function's result type.
    pub result: BType,
    /// Span covering the whole `fn` item.
    pub span: Span,
}

impl BFunction {
    /// The id of the entry block (`0` by construction).
    pub fn entry(&self) -> BlockId {
        BlockId::new(0)
    }

    /// The block registered under `id`, if any.
    pub fn block(&self, id: BlockId) -> Option<&BBlock> {
        self.blocks.get(id.raw() as usize)
    }

    /// The local registered under `id`, if any.
    pub fn local(&self, id: crate::mir::LocalId) -> Option<&BLocal> {
        self.locals.get(id.raw() as usize)
    }
}

/// A local value: a parameter, a binding, a loop variable, or a lowering
/// temporary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BLocal {
    /// The binding's source name; empty for anonymous temporaries.
    pub name: String,
    /// The source declaration's symbol, for bindings; `None` for
    /// temporaries.
    pub symbol: Option<SymbolId>,
    /// The local's classified value type.
    pub ty: BType,
    /// Whether the slot is written after initialization (parameters and
    /// bindings are immutable in the source; `for` loop variables are
    /// written by the loop machinery).
    pub mutable: bool,
    /// Span of the declaration (or of the expression that produced a
    /// temporary).
    pub span: Span,
}

/// A module-level `let`/`const` binding lowered to a machine global.
///
/// The first native subset supports only bindings whose initializer is a
/// single constant; the constant's decoded value is stored directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BStatic {
    /// The bound name.
    pub name: String,
    /// The binding's declaration symbol.
    pub symbol: SymbolId,
    /// Whether the slot is mutable (`let mut`). `const` bindings are never
    /// mutable.
    pub mutable: bool,
    /// The binding's classified value type.
    pub ty: BType,
    /// The decoded constant value the binding is initialized to.
    pub value: i64,
    /// Span of the whole binding.
    pub span: Span,
}

/// A basic block: an ordered list of instructions followed by exactly one
/// terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BBlock {
    /// This block's identity; always equal to its index in the function's
    /// block list.
    pub id: BlockId,
    /// The instructions, in order.
    pub insts: Vec<BInst>,
    /// The block's terminator — exactly one, always present.
    pub terminator: BTerminator,
    /// Span of the source construct this block was lowered from.
    pub span: Span,
}

/// A single instruction: computes a value and stores it into a local slot,
/// or moves a value between local and module storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BInst {
    /// The kind of instruction.
    pub kind: BInstKind,
    /// Span of the source statement (or expression) this was lowered from.
    pub span: Span,
}

/// The kind of a [`BInst`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BInstKind {
    /// Copy `src`'s value into `target`. Both are locals; the copy size
    /// follows the local type (`Range` copies two words).
    LoadLocal {
        /// The destination slot.
        target: crate::mir::LocalId,
        /// The source slot.
        src: crate::mir::LocalId,
    },
    /// Store a machine constant into `target`.
    LoadConst {
        /// The destination slot.
        target: crate::mir::LocalId,
        /// The decoded constant value (an integer, or `0`/`1` for a
        /// boolean).
        value: i64,
    },
    /// Load the module binding `static_index` into `target`.
    LoadStatic {
        /// The destination slot.
        target: crate::mir::LocalId,
        /// Index into [`BProgram::statics`].
        static_index: usize,
    },
    /// Store `src`'s value into the module binding `static_index`.
    StoreStatic {
        /// Index into [`BProgram::statics`].
        static_index: usize,
        /// The value being stored.
        src: BOperand,
    },
    /// Apply a unary operation to `src` and store the result in `target`.
    Unary {
        /// The destination slot.
        target: crate::mir::LocalId,
        /// The operator (`-`, `!`, `~`).
        op: UnaryOp,
        /// The operand.
        src: BOperand,
    },
    /// Apply a binary operation to `lhs` and `rhs` and store the result in
    /// `target`. Comparison, equality, and logical operators produce a
    /// `Bool` (`0`/`1`); the rest produce the operand type.
    Binary {
        /// The destination slot.
        target: crate::mir::LocalId,
        /// The operator.
        op: BinaryOp,
        /// The left operand.
        lhs: BOperand,
        /// The right operand.
        rhs: BOperand,
    },
    /// Call the function at `callee` (an index into [`BProgram::functions`])
    /// with `args` and store the result in `target`.
    Call {
        /// The destination slot for the call's result.
        target: crate::mir::LocalId,
        /// Index into [`BProgram::functions`].
        callee: usize,
        /// The arguments, in source order.
        args: Vec<BOperand>,
    },
    /// Construct a range value into `target`: a two-word value holding the
    /// normalized exclusive end (`end + 1` for inclusive ranges) and the
    /// iteration cursor (`start`).
    RangeInit {
        /// The destination slot (a two-word `Range` slot).
        target: crate::mir::LocalId,
        /// The range start.
        start: BOperand,
        /// The range end.
        end: BOperand,
        /// Whether the range is inclusive (`..=`).
        inclusive: bool,
    },
    /// Advance the range value in `range` and store the returned element in
    /// `target`: the element is the range's current cursor, and the cursor
    /// advances by one. Iteration order and exhaustion are backend
    /// semantics (see `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`).
    RangeNext {
        /// The destination slot for the next element.
        target: crate::mir::LocalId,
        /// The range value being iterated (a two-word `Range` slot).
        range: crate::mir::LocalId,
    },
    /// Whether the range value in `range` is exhausted: true when its
    /// cursor has reached the normalized exclusive end. Stores a `Bool`
    /// into `target`.
    RangeFinished {
        /// The destination slot for the `Bool` result.
        target: crate::mir::LocalId,
        /// The range value being iterated (a two-word `Range` slot).
        range: crate::mir::LocalId,
    },
}

/// How a basic block ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BTerminator {
    /// `return;` or `return expr;`. A bare return has no value.
    Return {
        /// The returned value, when present.
        value: Option<BOperand>,
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
    /// A conditional branch: continue in `then_block` when `cond` is true
    /// (non-zero), in `else_block` otherwise.
    Branch {
        /// The condition value (a `Bool` operand).
        cond: BOperand,
        /// The block entered when `cond` is true.
        then_block: BlockId,
        /// The block entered when `cond` is false.
        else_block: BlockId,
        /// Span of the construct that produced this branch.
        span: Span,
    },
}

/// An instruction operand: a load of a local slot, or a machine constant.
///
/// Constants are decoded at lowering: integers become their 64-bit
/// two's-complement value and booleans become `0`/`1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BOperand {
    /// A load of the local slot's current value.
    Local(crate::mir::LocalId),
    /// A machine constant (an integer value, or `0`/`1` for booleans).
    Const(i64),
}

/// The lowering result: either a valid [`BProgram`] or every problem found,
/// in deterministic order.
pub(crate) type LowerResult = Result<BProgram, Vec<BackendError>>;
