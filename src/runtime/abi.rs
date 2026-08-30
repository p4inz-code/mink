//! The runtime ABI: the contract between generated code and the MINK
//! runtime.
//!
//! Every native image embeds a small runtime (see
//! `src/backend/emit/runtime.rs`) that provides initialization, the
//! deterministic heap, and error/exit handling. The ABI below is the
//! single source of truth for the machine-level layout; the backend
//! emitter and the reference allocator both read these constants, so the
//! Rust-side specification and the emitted machine code cannot drift.
//!
//! ## Calling convention
//!
//! Generated code calls runtime services exactly like MINK functions (the
//! same convention used between MINK functions, see
//! `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`):
//!
//! - arguments are pushed on the stack rightmost-first, one 64-bit word
//!   each;
//! - the callee reads argument `i` at `[rbp + 16 + 8 * i]`;
//! - the result is returned in `rax`;
//! - the stack stays 16-byte aligned at every `call` (the emitter inserts
//!   padding when an odd number of argument words is pushed);
//! - `rbp`/`rsp` are the only callee-saved registers.
//!
//! Runtime-internal helpers (error reporting, output thunks) use a private
//! register convention documented in `src/backend/emit/runtime.rs`.
//!
//! ## The heap
//!
//! The heap is a fixed-size arena in the image's `.bss` section (zero-
//! filled by the loader, so the runtime state starts deterministic):
//!
//! - blocks are 16-byte aligned, sizes are rounded up to 16;
//! - allocation is a validated bump allocator with LIFO free-list reuse;
//! - every live allocation is recorded in a bounded liveness table
//!   ([`MAX_LIVE_ALLOCS`] entries); invalid operations are detected
//!   against the table and reported as runtime errors (`E-R02`…`E-R08`);
//! - the heap is exhausted when the bump cursor reaches [`HEAP_SIZE`]
//!   (`E-R02`).
//!
//! ## Process lifecycle
//!
//! The entry stub captures the process-entry stack pointer, calls
//! [`RuntimeService::Init`](crate::backend::RuntimeService::Init), calls
//! `main`, then calls the exit service with `main`'s result. The exit
//! service verifies that no live allocations remain (a leak is `E-R06`)
//! and terminates by restoring the captured stack pointer and returning —
//! the loader turns the result into the process exit code. Runtime errors
//! terminate with exit code [`EXIT_CODE_BASE`]` + error number` after
//! writing a structured diagnostic to stderr.

/// The size of one machine word in bytes.
pub const WORD_SIZE: u64 = 8;

/// The alignment (and minimum rounded size) of every heap allocation.
pub const ALLOC_ALIGNMENT: u64 = 16;

/// The size of the runtime heap arena, in bytes. Fixed at compile time so
/// allocation is deterministic and needs no operating-system interaction.
pub const HEAP_SIZE: u64 = 1024 * 1024;

/// The maximum number of simultaneously live allocations. The liveness
/// table is a fixed-size array, so the bound is part of the ABI.
pub const MAX_LIVE_ALLOCS: usize = 256;

/// The size of one liveness-table entry: (address `u64`, size `u64`,
/// live flag `u64`).
pub const LIVE_ENTRY_BYTES: u64 = 24;

/// The bytes of the liveness table (all entries).
pub const LIVE_TABLE_BYTES: u64 = MAX_LIVE_ALLOCS as u64 * LIVE_ENTRY_BYTES;

/// The exit code base for runtime errors: a runtime error with number `n`
/// terminates the process with exit code [`EXIT_CODE_BASE`]` + n`.
pub const EXIT_CODE_BASE: u64 = 100;

/// The stack frame of every generated function: the return address and the
/// saved `rbp` occupy `[rbp]` and `[rbp + 8]`; the first argument is at
/// `[rbp + 16]`; locals live below `rbp` in a 16-byte-aligned frame.
pub mod frame {
    /// Bytes of the return address + saved `rbp` (the "argument base").
    pub const ARG_BASE: u64 = 16;
}

/// The offsets of the runtime state within the `.bss` section. The state
/// is zero-initialized by the loader; only position-dependent values
/// (the arena base) are written by `rt_init`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeLayout {
    /// Process-entry stack pointer, captured by the entry stub.
    pub entry_rsp: u64,
    /// Head of the LIFO free list (an absolute address, or 0 when empty).
    pub free_head: u64,
    /// The bump cursor: committed bytes from the arena base. Zero-initialized.
    pub cursor: u64,
    /// Cached stdout handle (0 = not fetched yet).
    pub stdout_handle: u64,
    /// Cached stderr handle (0 = not fetched yet).
    pub stderr_handle: u64,
    /// Scratch slot for `WriteFile`'s bytes-written count.
    pub bytes_written: u64,
    /// Scratch buffer for decimal conversion in `rt_print_int` and
    /// `rt_print_float` output assembly (32 bytes).
    pub print_buf: u64,
    /// Scratch words for `rt_print_float`'s exact big-integer expansion
    /// (40 u64 words = 320 bytes; `f * 5^1074` needs 2548 bits).
    pub dtoa_words: u64,
    /// Scratch digits for `rt_print_float`: the exact decimal expansion
    /// (up to 767 digits, stored one byte each).
    pub dtoa_digits: u64,
    /// Start of the image's immutable string-data region (an absolute
    /// address, written by `rt_init`). String blobs (length prefix + UTF-8
    /// bytes) live here; the bounds let the string intrinsics validate
    /// literal strings and distinguish them from heap blobs.
    pub str_data_start: u64,
    /// One past the end of the image's immutable string-data region.
    pub str_data_end: u64,
    // --- Process output storage (Session 59) ---
    /// Pointer to last process stdout (BSS Str, or 0).
    pub proc_stdout_ptr: u64,
    /// Length of last process stdout.
    pub proc_stdout_len: u64,
    /// Pointer to last process stderr (BSS Str, or 0).
    pub proc_stderr_ptr: u64,
    /// Length of last process stderr.
    pub proc_stderr_len: u64,
    /// The heap arena, 16-byte aligned.
    pub arena: u64,
    /// The liveness table: `MAX_LIVE_ALLOCS` entries of
    /// `LIVE_ENTRY_BYTES` bytes each.
    pub table: u64,
    /// Fixed BSS buffer for process stdout [len:u64][bytes;4088].
    pub stdout_buf: u64,
    /// Fixed BSS buffer for process stderr [len:u64][bytes;4088].
    pub stderr_buf: u64,
    // --- Networking (Session 67) ---
    /// Winsock2 initialized flag.
    pub wsa_initialized: u64,
    /// ws2_32.dll handle from LoadLibraryA.
    pub ws2_dll_handle: u64,
    /// Function pointer table for ws2_32 functions (17 × 8 = 136 bytes).
    pub net_func_table: u64,
    /// Buffer for received data [len:u64][bytes;4088].
    pub recv_buf: u64,
    // --- Crypto (Session 71) ---
    /// bcryptprimitives.dll handle from LoadLibraryA.
    pub bcrypt_dll_handle: u64,
    /// Function pointer table for bcrypt functions (3 × 8 = 24 bytes).
    pub crypto_func_table: u64,
    // --- Random (Session 73) ---
    /// xorshift64* PRNG state (8 bytes).
    pub rng_state: u64,
    // --- Environment (Session 73) ---
    /// Simple env key-value storage (4096 bytes).
    pub env_storage: u64,
    /// The total `.bss` size.
    pub size: u64,
}

/// The canonical [`RuntimeLayout`] the backend emits against.
pub const BSS: RuntimeLayout = RuntimeLayout {
    entry_rsp: 0,
    free_head: 8,
    cursor: 16,
    stdout_handle: 24,
    stderr_handle: 32,
    bytes_written: 40,
    print_buf: 48,
    dtoa_words: 80,
    dtoa_digits: 400,
    str_data_start: 1424,
    str_data_end: 1432,
    // Process output storage at 1440..1472 (4 qwords)
    proc_stdout_ptr: 1440,
    proc_stdout_len: 1448,
    proc_stderr_ptr: 1456,
    proc_stderr_len: 1464,
    arena: 1488,
    table: 1488 + HEAP_SIZE,
    // Output buffers after liveness table (Session 62)
    stdout_buf: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES,
    stderr_buf: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096,
    // --- Networking (Session 67) ---
    wsa_initialized: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096 * 2,
    ws2_dll_handle: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096 * 2 + 8,
    net_func_table: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096 * 2 + 16,
    recv_buf: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096 * 2 + 16 + 17 * 8,
    // Crypto BSS after networking
    bcrypt_dll_handle: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096 * 2 + 16 + 17 * 8 + 4096,
    crypto_func_table: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096 * 2 + 16 + 17 * 8 + 4096 + 8,
    // --- Random (Session 73) ---
    rng_state: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096 * 2 + 16 + 17 * 8 + 4096 + 8 + 24,
    // --- Environment (Session 73) ---
    env_storage: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096 * 2 + 16 + 17 * 8 + 4096 + 8 + 24 + 8,
    size: 1488 + HEAP_SIZE + LIVE_TABLE_BYTES + 4096 * 2 + 16 + 17 * 8 + 4096 + 8 + 24 + 8 + 4096,
};

/// The arithmetic performed on sizes: round up to [`ALLOC_ALIGNMENT`].
pub fn align_up(size: u64, alignment: u64) -> u64 {
    debug_assert!(alignment.is_power_of_two());
    size.wrapping_add(alignment - 1) & !(alignment - 1)
}
