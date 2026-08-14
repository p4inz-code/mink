//! Runtime intrinsics: the memory/runtime primitives MINK programs call.
//!
//! The runtime is exposed to MINK source through a small closed set of
//! intrinsic functions. They are predeclared by semantic analysis as
//! [`SymbolKind::Intrinsic`](crate::semantics::SymbolKind::Intrinsic)
//! symbols (the `rt_` names are reserved), typed by the type checker with
//! the concrete signatures in this table, lowered through HIR/MIR as
//! ordinary direct calls, and lowered by the backend to
//! [`RuntimeCall`](crate::backend::BInstKind::RuntimeCall) instructions
//! that invoke the embedded machine-level services.
//!
//! The intrinsics:
//!
//! - `rt_alloc(size) -> Int` — allocate a 16-byte-aligned block of at
//!   least `size` bytes; returns the block's address as an integer, or
//!   terminates with a runtime error;
//! - `rt_free(ptr)` — deallocate the block at `ptr` (must be the exact
//!   start of a live allocation);
//! - `rt_mem_load(addr) -> Int` — load the 8-byte word at `addr`;
//! - `rt_mem_store(addr, value)` — store the 8-byte word `value` at
//!   `addr`;
//! - `rt_exit(code)` — terminate the process with exit code `code` after
//!   verifying there are no leaks;
//! - `rt_print_int(value)` — write the decimal representation of `value`
//!   plus a newline to stdout.
//!
//! Addresses are represented as integers; dereferencing goes through the
//! validated `rt_mem_load`/`rt_mem_store` accessors, so every memory
//! operation is checked against the liveness table (see [`allocator`](super::allocator)).

/// The type of an intrinsic parameter or result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicType {
    /// A 64-bit integer (addresses and sizes are integers).
    Int,
    /// No value (the intrinsic produces nothing).
    Unit,
}

/// A declared runtime intrinsic: its reserved name and its signature.
#[derive(Debug, Clone, Copy)]
pub struct Intrinsic {
    /// The reserved source name (prefix `rt_`).
    pub name: &'static str,
    /// The parameter types, in order.
    pub params: &'static [IntrinsicType],
    /// The result type.
    pub result: IntrinsicType,
}

/// The stable identity of an intrinsic: its index in [`ALL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IntrinsicId(pub usize);

impl IntrinsicId {
    /// The raw index of this intrinsic in [`ALL`].
    pub fn raw(self) -> usize {
        self.0
    }

    /// The intrinsic this id denotes.
    pub fn get(self) -> &'static Intrinsic {
        &ALL[self.0]
    }
}

/// Every runtime intrinsic, in stable order. The order is part of the
/// runtime ABI: intrinsic ids travel through HIR and MIR and are mapped
/// to backend services by the backend.
pub const ALL: &[Intrinsic] = &[
    Intrinsic {
        name: "rt_alloc",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_free",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_mem_load",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_mem_store",
        params: &[IntrinsicType::Int, IntrinsicType::Int],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_exit",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_print_int",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Unit,
    },
];

/// Looks up an intrinsic by its reserved name.
pub fn by_name(name: &str) -> Option<&'static Intrinsic> {
    ALL.iter().find(|intrinsic| intrinsic.name == name)
}

/// The [`IntrinsicId`] of the intrinsic named `name`, if any.
pub fn id_of(name: &str) -> Option<IntrinsicId> {
    ALL.iter()
        .position(|intrinsic| intrinsic.name == name)
        .map(IntrinsicId)
}
