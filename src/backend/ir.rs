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
/// subset supports: 64-bit integers, booleans (stored as `0`/`1`), integer
/// ranges (stored as a two-word value), typed pointers and strings (each a
/// single word holding an address), structs and arrays (session 14: values
/// occupying a fixed number of words in a slot, with the byte layout
/// resolved by the lowering stage from the deterministic layout engine),
/// and `Unit` (the type of a function that produces no value). Every other
/// MINK type (`Float`, `Char`, `Null`, unresolved inference types) is
/// rejected at lowering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BType {
    /// A 64-bit two's-complement integer.
    Int,
    /// A boolean: `0` (false) or `1` (true). Within an aggregate value a
    /// boolean occupies one byte; as a standalone value it occupies one
    /// word.
    Bool,
    /// A range of integers: a two-word value carrying the (normalized)
    /// exclusive end and the iteration cursor.
    Range,
    /// A typed pointer (`Ptr<Int>` in the current language): a single word
    /// holding an address, produced by `rt_alloc` and consumed by the raw
    /// memory intrinsics. Pointer arithmetic is byte-addressed; the
    /// runtime validates alignment and bounds at every access.
    Ptr,
    /// A reference (session 16): `&T` / `&mut T`, a single word holding
    /// the machine address of a stack slot (or of a field/element region
    /// inside one). Mutability is a compile-time concept — the ABI is an
    /// address either way. The referent's shape and size are carried by
    /// the `RefLoad`/`RefStore` instructions, never by this type itself.
    Ref,
    /// A string: a single word holding the address of a length-prefixed
    /// UTF-8 byte blob (immutable image data for literals, a heap block
    /// for `rt_str_alloc` results).
    Str,
    /// A struct value: a fixed number of words in a slot (see
    /// [`BLocal::words`]); the field byte offsets and types are carried by
    /// the field-access instructions. Aggregate values are never returned
    /// from or stored in module bindings (rejected at lowering).
    Struct,
    /// An array value: a fixed number of words in a slot (see
    /// [`BLocal::words`]); the element size, length, and stride are
    /// carried by the index-access instructions.
    Array,
    /// No value (a function that does not produce one).
    Unit,
}

impl BType {
    /// Whether values of this type occupy exactly one machine word.
    pub fn is_word_sized(self) -> bool {
        matches!(self, Self::Int | Self::Bool)
    }

    /// The number of 64-bit words a scalar value of this type occupies in
    /// a stack slot or argument list. Struct and array values have a
    /// per-local word count ([`BLocal::words`]); this returns `1` as a
    /// scalar fallback and callers that handle aggregates read the local's
    /// count instead.
    pub fn words(self) -> usize {
        match self {
            Self::Int | Self::Bool | Self::Ptr | Self::Ref | Self::Str | Self::Unit => 1,
            Self::Range => 2,
            Self::Struct | Self::Array => 1,
        }
    }

    /// A human-readable name, used in diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Self::Int => "Int",
            Self::Bool => "Bool",
            Self::Range => "Range<Int>",
            Self::Ptr => "Ptr<Int>",
            Self::Ref => "reference",
            Self::Str => "Str",
            Self::Struct => "struct",
            Self::Array => "array",
            Self::Unit => "unit",
        }
    }
}

/// A lowered backend program: functions, module bindings, and string
/// literals in source order.
///
/// The program is self-contained: it owns every value its instructions
/// reference (locals are per-function, statics and strings are owned here,
/// and function/static/string references are indices into the
/// corresponding lists).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BProgram {
    /// The functions, in source order.
    pub functions: Vec<BFunction>,
    /// The module-level `let`/`const` bindings, in source order.
    pub statics: Vec<BStatic>,
    /// The string literals, in first-use order: the decoded UTF-8 byte
    /// contents of every string literal in the program, plus its exact
    /// source span. The emitter places each blob (length prefix + bytes)
    /// into the image and [`BInstKind::LoadStr`] references it by index.
    pub strings: Vec<BString>,
}

/// A decoded string literal: the immutable byte data the image will carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BString {
    /// The decoded UTF-8 bytes of the literal (escapes already decoded;
    /// the raw source text is recovered from `span`).
    pub bytes: Vec<u8>,
    /// The exact span of the literal token, preserved for diagnostics and
    /// tooling.
    pub span: Span,
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
    /// The number of 64-bit words the value occupies in its stack slot:
    /// `ty.words()` for scalars, `ceil(size / 8)` for struct and array
    /// values (computed from the deterministic layout).
    pub words: u32,
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
    /// Load the address of string blob `string_index` (an index into
    /// [`BProgram::strings`]) into `target`. The blob lives in the image's
    /// immutable data; its address is the string value.
    LoadStr {
        /// The destination slot (a `Str` slot).
        target: crate::mir::LocalId,
        /// Index into [`BProgram::strings`].
        string_index: usize,
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
    /// Call an embedded runtime service (the `rt_*` intrinsics) with
    /// `args` and store the result in `target`. The service uses the same
    /// calling convention as [`BInstKind::Call`]; the emitter resolves the
    /// service to the machine code of the embedded runtime.
    RuntimeCall {
        /// The destination slot for the call's result (`Unit`-typed for
        /// services that produce no value).
        target: crate::mir::LocalId,
        /// The runtime service to invoke.
        service: RuntimeService,
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
    /// Load a struct field of the value in `base` into `target`.
    /// `field_ty` is the field's classified type, `byte_offset` its byte
    /// offset within the value, and `size` its byte size (the emitter
    /// copies exactly `size` bytes, so fields that are not word-aligned —
    /// booleans, nested all-bool structs — are copied exactly). The
    /// offsets and sizes are resolved at lowering from the deterministic
    /// layout, so the emitter never re-computes layout.
    FieldLoad {
        /// The destination slot.
        target: crate::mir::LocalId,
        /// The struct value's slot.
        base: crate::mir::LocalId,
        /// The field's classified type.
        field_ty: BType,
        /// The field's byte offset within the value.
        byte_offset: u32,
        /// The field's byte size.
        size: u32,
    },
    /// Store `src` into the field of the value in `base` (see
    /// [`BInstKind::FieldLoad`] for the field description).
    FieldStore {
        /// The struct value's slot.
        base: crate::mir::LocalId,
        /// The field's classified type.
        field_ty: BType,
        /// The field's byte offset within the value.
        byte_offset: u32,
        /// The field's byte size.
        size: u32,
        /// The value being stored.
        src: BOperand,
    },
    /// Load the element at `index` of the array value in `base` into
    /// `target`. `elem_ty` is the element's classified type, `stride` its
    /// byte size (the array's stride), and `len` the array's length. The
    /// index is bounds checked at execution (`E-R10`): a negative index or
    /// an index at or above `len` terminates with a structured runtime
    /// error.
    IndexLoad {
        /// The destination slot.
        target: crate::mir::LocalId,
        /// The array value's slot.
        base: crate::mir::LocalId,
        /// The element's classified type.
        elem_ty: BType,
        /// The element byte size (array stride).
        stride: u32,
        /// The array's length.
        len: u64,
        /// The index.
        index: BOperand,
    },
    /// Store `src` into the element at `index` of the array value in
    /// `base` (see [`BInstKind::IndexLoad`] for the description). The
    /// index is bounds checked at execution (`E-R10`).
    IndexStore {
        /// The array value's slot.
        base: crate::mir::LocalId,
        /// The element's classified type.
        elem_ty: BType,
        /// The element byte size (array stride).
        stride: u32,
        /// The array's length.
        len: u64,
        /// The index.
        index: BOperand,
        /// The value being stored.
        src: BOperand,
    },
    /// Store `src` into a multi-step storage place: the value rooted at
    /// `base`'s slot, addressed by walking `steps` from the root's first
    /// word (each field step subtracts its byte offset; each index step
    /// subtracts `index * stride` with an `E-R10` bounds check). `size` is
    /// the target's byte size, so the emitter copies exactly the target's
    /// bytes. The steps are resolved at lowering from the deterministic
    /// layout.
    PlaceStore {
        /// The local holding the outermost value of the chain.
        base: crate::mir::LocalId,
        /// The address steps from the root to the target, outermost first.
        steps: Vec<PlaceAddrStep>,
        /// The target's byte size.
        size: u32,
        /// The value being stored.
        src: BOperand,
    },
    /// Form a reference (session 16): compute the machine address of the
    /// place rooted at `base`'s slot, walked by `steps` from the root's
    /// first word (field steps subtract their static byte offset; index
    /// steps are bounds-checked, `E-R10`, and subtract `index * stride`),
    /// and store the address into `target` (a word-sized `Ref` slot). The
    /// `E-R10` fail path is the function's shared bounds-check block.
    RefAddr {
        /// The destination slot (a `Ref` slot).
        target: crate::mir::LocalId,
        /// The local holding the outermost value of the borrowed place.
        base: crate::mir::LocalId,
        /// The address steps from the root to the borrowed element,
        /// outermost first.
        steps: Vec<PlaceAddrStep>,
    },
    /// Load through a reference (session 16): copy `size` bytes from the
    /// memory addressed by the reference in `reference` into `target`'s
    /// slot. `elem_ty` is the referent's classified type; `size` its byte
    /// size. The reference's mutability is not an ABI concern.
    RefLoad {
        /// The destination slot.
        target: crate::mir::LocalId,
        /// The reference value (a single-word address).
        reference: BOperand,
        /// The referent's classified type.
        elem_ty: BType,
        /// The referent's byte size.
        size: u32,
    },
    /// Store through a reference (session 16): copy `src`'s `size` bytes
    /// into the memory addressed by the reference in `reference`.
    RefStore {
        /// The reference value (a single-word address).
        reference: BOperand,
        /// The referent's classified type.
        elem_ty: BType,
        /// The referent's byte size.
        size: u32,
        /// The value being stored.
        src: BOperand,
    },
}

/// One resolved address step of a [`BInstKind::PlaceStore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceAddrStep {
    /// A field selection: the value's bytes at `byte_offset` (a static
    /// displacement from the current address).
    Field {
        /// The field's byte offset within the value.
        byte_offset: u32,
    },
    /// An index selection: `index * stride` bytes from the current
    /// address, bounds-checked against `len` (`E-R10`).
    Index {
        /// The index operand.
        index: BOperand,
        /// The element byte size (array stride).
        stride: u32,
        /// The array's length.
        len: u64,
    },
}

/// A machine-level runtime service of the embedded MINK runtime.
///
/// Services are called with the MINK calling convention (stack arguments,
/// result in `rax`) and are emitted into every image after the user
/// functions. The intrinsic calls produced by lowering use the *callable*
/// subset ([`RuntimeService::is_callable`]); the remaining services are
/// invoked by the entry stub or internally by other services. The machine
/// implementations live in `src/backend/emit/runtime.rs` and the ABI in
/// `src/runtime/abi.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeService {
    /// Runtime initialization: sets the bump cursor to the arena base,
    /// resets the free list, and records the immutable string-data bounds.
    /// Called by the entry stub before `main`.
    Init,
    /// `rt_alloc(size) -> Ptr<Int>`: allocate a 16-byte-aligned block.
    Alloc,
    /// `rt_free(ptr)`: deallocate a live block.
    Free,
    /// `rt_mem_load(ptr) -> Int`: load a validated 8-byte word.
    MemLoad,
    /// `rt_mem_store(ptr, value)`: store a validated 8-byte word.
    MemStore,
    /// `rt_str_alloc(size) -> Str`: allocate a length-prefixed,
    /// zero-initialized string blob of `size` bytes.
    StrAlloc,
    /// `rt_str_free(s)`: deallocate a live string blob.
    StrFree,
    /// `rt_str_len(s) -> Int`: the byte length of a validated string.
    StrLen,
    /// `rt_str_byte(s, index) -> Int`: the byte of a validated string at
    /// `index` (`E-R09` when out of range).
    StrByte,
    /// `rt_str_set_byte(s, index, value)`: write a byte of a *heap*
    /// string (`E-R09` when out of range; immutable literals are rejected).
    StrSetByte,
    /// `rt_print_str(s)`: write the bytes of a validated string plus a
    /// newline to stdout.
    PrintStr,
    /// `rt_exit(code)`: terminate with `code` after the leak check. Also
    /// the exit path the entry stub invokes with `main`'s result.
    Exit,
    /// `rt_print_int(value)`: write the decimal value plus a newline to
    /// stdout.
    PrintInt,
    /// Internal: report a runtime error (`rcx` = error number) to stderr
    /// and terminate with exit code `100 + number`. Never returns.
    Fail,
    /// Internal: write `rcx`-pointed bytes of `rdx` length to stdout.
    WriteStdout,
    /// Internal: write `rcx`-pointed bytes of `rdx` length to stderr.
    WriteStderr,
    /// Internal: validate a string pointer (`rax` in → `rax` out, or
    /// `E-R05`). Accepts a live heap-block start or a pointer into the
    /// image's immutable string-data region.
    StrValidate,
    /// Internal: validate a *mutable* string pointer (`rax` in → `rax`
    /// out, or `E-R05`). Accepts only a live heap-block start; immutable
    /// image strings are rejected.
    StrValidateHeap,
}

impl RuntimeService {
    /// The number of stack arguments this service consumes. The type
    /// checker guarantees call-site arity; the verifier re-checks it.
    pub fn arity(self) -> usize {
        match self {
            Self::Init
            | Self::Fail
            | Self::WriteStdout
            | Self::WriteStderr
            | Self::StrValidate
            | Self::StrValidateHeap => 0,
            Self::Alloc
            | Self::Free
            | Self::MemLoad
            | Self::StrAlloc
            | Self::StrFree
            | Self::StrLen
            | Self::PrintStr
            | Self::Exit
            | Self::PrintInt => 1,
            Self::MemStore | Self::StrByte => 2,
            Self::StrSetByte => 3,
        }
    }

    /// Whether generated code may call this service directly (the `rt_*`
    /// intrinsics). The remaining services are entry-stub or internal.
    pub fn is_callable(self) -> bool {
        matches!(
            self,
            Self::Alloc
                | Self::Free
                | Self::MemLoad
                | Self::MemStore
                | Self::StrAlloc
                | Self::StrFree
                | Self::StrLen
                | Self::StrByte
                | Self::StrSetByte
                | Self::PrintStr
                | Self::Exit
                | Self::PrintInt
        )
    }
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
