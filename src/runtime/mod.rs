//! The MINK runtime foundation.
//!
//! This module is the authoritative specification of the MINK memory model
//! and runtime ABI that the native backend emits code against. It is
//! compiler-side: it defines the *contract* between generated code and the
//! machine-level runtime services the backend embeds into every image (see
//! `src/backend/emit/runtime.rs`), the stable runtime error catalog
//! (`E-R01`…), the explicit memory-layout representation, and a pure-Rust
//! reference implementation of the deterministic allocator that unit tests
//! validate against.
//!
//! ## Memory-model boundaries
//!
//! The foundation covers exactly:
//!
//! - **stack storage** — every function's parameters and locals live in its
//!   own 16-byte-aligned stack frame, from prologue to epilogue
//!   ([`abi`]`::FRAME` rules; generated-code discipline, not a runtime
//!   service);
//! - **static storage** — module bindings live in the image's `.data`
//!   section, initialized by the loader;
//! - **heap storage** — a fixed-size arena in the image's `.bss` section,
//!   managed by the deterministic validated allocator ([`allocator`]) with
//!   a bounded liveness table, reached through the `rt_alloc` /
//!   `rt_free` / `rt_mem_load` / `rt_mem_store` intrinsics
//!   ([`intrinsics`]).
//!
//! The foundation deliberately does **not** claim: garbage collection,
//! borrowing or full ownership checking, thread safety, or automatic
//! reclamation. Values own their storage: a local owns its stack slot, and
//! a heap allocation is owned by the code that called `rt_alloc` until it
//! calls `rt_free` (a leak is a runtime error, `E-R06`, reported at exit).
//! Every invalid memory operation the model can observe is reported as a
//! structured runtime error instead of being silently allowed.
//!
//! The full specification is `docs/implementation/RUNTIME_IMPLEMENTATION.md`.

pub mod abi;
pub mod allocator;
pub mod error;
pub mod intrinsics;
pub mod layout;
pub mod verify;

pub use abi::{HEAP_SIZE, MAX_LIVE_ALLOCS, RuntimeLayout};
pub use error::{RuntimeError, RuntimeErrorKind};
pub use intrinsics::{Intrinsic, IntrinsicId, IntrinsicType};
pub use layout::{MemoryLayout, ValueClass};
pub use verify::RuntimeVerifier;
