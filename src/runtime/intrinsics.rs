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
//! - `rt_alloc(size: Int) -> Ptr<Int>` — allocate a 16-byte-aligned block
//!   of at least `size` bytes; returns the block's address as a typed
//!   pointer (or terminates with a runtime error);
//! - `rt_free(ptr: Ptr<Int>)` — deallocate the block at `ptr` (must be the
//!   exact start of a live allocation);
//! - `rt_mem_load(ptr: Ptr<Int>) -> Int` — load the 8-byte word at `ptr`;
//! - `rt_mem_store(ptr: Ptr<Int>, value: Int)` — store the 8-byte word
//!   `value` at `ptr`;
//! - `rt_str_alloc(size: Int) -> Str` — allocate a zero-initialized string
//!   blob of `size` bytes (a length-prefixed heap block) and return it;
//! - `rt_str_free(s: Str)` — deallocate the string blob at `s`;
//! - `rt_str_len(s: Str) -> Int` — the byte length of `s`;
//! - `rt_str_byte(s: Str, index: Int) -> Int` — the byte of `s` at
//!   `index` (0-based, bounds-checked);
//! - `rt_str_set_byte(s: Str, index: Int, value: Int)` — write the byte
//!   `value` of `s` at `index` (heap strings only; immutable literals are
//!   rejected);
//! - `rt_print_str(s: Str)` — write the bytes of `s` plus a newline to
//!   stdout;
//! - `rt_exit(code: Int)` — terminate the process with exit code `code`
//!   after verifying there are no leaks;
//! - `rt_print_int(value: Int)` — write the decimal representation of
//!   `value` plus a newline to stdout;
//! - `rt_print_float(value: Float)` — write the decimal representation of
//!   `value` plus a newline to stdout (session 24: exact
//!   17-significant-digit expansion, fixed or scientific, with
//!   `Inf`/`NaN`/`-0` forms);
//! - `rt_print_char(value: Char)` — write the single byte of `value` plus
//!   a newline to stdout;
//! - `rt_str_concat(a: Str, b: Str) -> Str` — allocate a new string
//!   containing the bytes of `a` followed by the bytes of `b`;
//! - `rt_str_eq(a: Str, b: Str) -> Bool` — byte-for-byte comparison,
//!   returning true when both strings have equal length and content;
//! - `rt_str_from_int(value: Int) -> Str` — allocate a string holding
//!   the decimal representation of `value`;
//! - `rt_str_from_bool(value: Bool) -> Str` — allocate a string holding
//!   `"true"` or `"false"`.
//!
//! Addresses are typed pointers (`Ptr<Int>`), distinct from strings
//! (`Str`). Dereferencing goes through the validated `rt_mem_load` /
//! `rt_mem_store` accessors, so every memory operation is checked against
//! the liveness table (see [`allocator`](super::allocator)), and string
//! operations validate their targets against the liveness table and the
//! image's immutable string-data region.

/// The type of an intrinsic parameter or result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicType {
    /// A 64-bit integer (sizes, indices, and word values).
    Int,
    /// A typed pointer to a word (`Ptr<Int>`): the type of a heap-block
    /// address in the runtime model. The pointer element types the model
    /// needs are a closed set; today only `Ptr<Int>` occurs.
    Ptr,
    /// A string value (the address of a length-prefixed byte blob).
    Str,
    /// A 64-bit IEEE-754 double-precision floating-point value.
    Float,
    /// A byte-sized character (a Unicode scalar value that fits in one
    /// byte; the runtime's char model is byte-sized).
    Char,
    /// A boolean value (0 = false, 1 = true).
    Bool,
    /// No value (the intrinsic produces nothing).
    Unit,
    /// A Vec<T> value: a dynamic array of word-sized elements.
    Vec,
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
        result: IntrinsicType::Ptr,
    },
    Intrinsic {
        name: "rt_free",
        params: &[IntrinsicType::Ptr],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_mem_load",
        params: &[IntrinsicType::Ptr],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_mem_store",
        params: &[IntrinsicType::Ptr, IntrinsicType::Int],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_str_alloc",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_str_free",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_str_len",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_str_byte",
        params: &[IntrinsicType::Str, IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_str_set_byte",
        params: &[IntrinsicType::Str, IntrinsicType::Int, IntrinsicType::Int],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_print_str",
        params: &[IntrinsicType::Str],
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
    Intrinsic {
        name: "rt_print_float",
        params: &[IntrinsicType::Float],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_print_char",
        params: &[IntrinsicType::Char],
        result: IntrinsicType::Unit,
    },
    // --- Vec intrinsics (Session 41) ---
    // Buffer layout: [capacity: Int][length: Int][elem0][elem1]...
    // Each element is one word (8 bytes).
    Intrinsic {
        name: "rt_vec_new",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Vec,
    },
    Intrinsic {
        name: "rt_vec_push",
        params: &[IntrinsicType::Vec, IntrinsicType::Int],
        result: IntrinsicType::Vec,
    },
    Intrinsic {
        name: "rt_vec_get",
        params: &[IntrinsicType::Vec, IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_vec_len",
        params: &[IntrinsicType::Vec],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_vec_free",
        params: &[IntrinsicType::Vec],
        result: IntrinsicType::Unit,
    },
    // --- Vec operations (Session 57) ---
    Intrinsic {
        name: "rt_vec_set",
        params: &[IntrinsicType::Vec, IntrinsicType::Int, IntrinsicType::Int],
        result: IntrinsicType::Vec,
    },
    Intrinsic {
        name: "rt_vec_pop",
        params: &[IntrinsicType::Vec],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_vec_remove",
        params: &[IntrinsicType::Vec, IntrinsicType::Int],
        result: IntrinsicType::Vec,
    },
    // --- String operations (Session 44) ---
    Intrinsic {
        name: "rt_str_concat",
        params: &[IntrinsicType::Str, IntrinsicType::Str],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_str_eq",
        params: &[IntrinsicType::Str, IntrinsicType::Str],
        result: IntrinsicType::Bool,
    },
    Intrinsic {
        name: "rt_str_from_int",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_str_from_bool",
        params: &[IntrinsicType::Bool],
        result: IntrinsicType::Str,
    },
    // --- Numeric conversion (Session 54) ---
    Intrinsic {
        name: "rt_int_to_float",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Float,
    },
    Intrinsic {
        name: "rt_float_to_int",
        params: &[IntrinsicType::Float],
        result: IntrinsicType::Int,
    },
    // --- Filesystem (Session 56) ---
    Intrinsic {
        name: "rt_fs_read",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_fs_write",
        params: &[IntrinsicType::Str, IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_fs_exists",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Bool,
    },
    Intrinsic {
        name: "rt_fs_file_size",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_fs_create_dir",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_fs_remove_dir",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_fs_remove_file",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_fs_copy",
        params: &[IntrinsicType::Str, IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_fs_move",
        params: &[IntrinsicType::Str, IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_fs_get_cwd",
        params: &[],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_fs_set_cwd",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_to_cstr",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Ptr,
    },
    Intrinsic {
        name: "rt_free_cstr",
        params: &[IntrinsicType::Ptr],
        result: IntrinsicType::Unit,
    },
    // --- Process (Session 59) ---
    Intrinsic {
        name: "rt_process_run",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_process_stdout",
        params: &[],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_process_stderr",
        params: &[],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_process_stdout_len",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_process_stderr_len",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_process_id",
        params: &[],
        result: IntrinsicType::Int,
    },
    // --- Time (Session 60) ---
    Intrinsic {
        name: "rt_time_now",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_time_millis",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_time_ticks",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_time_freq",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_time_filetime",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_time_filetime_high",
        params: &[],
        result: IntrinsicType::Int,
    },
    // --- Random (Session 61) ---
    Intrinsic {
        name: "rt_random_seed",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_random_next",
        params: &[],
        result: IntrinsicType::Int,
    },
    // --- Environment (Session 65) ---
    Intrinsic {
        name: "rt_env_get",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_env_set",
        params: &[IntrinsicType::Str, IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_env_has",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Bool,
    },
    Intrinsic {
        name: "rt_env_remove",
        params: &[IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    // --- Networking (Session 67) ---
    Intrinsic {
        name: "rt_net_wsa_startup",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_wsa_cleanup",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_wsa_last_error",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_socket",
        params: &[IntrinsicType::Int, IntrinsicType::Int, IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_connect",
        params: &[IntrinsicType::Int, IntrinsicType::Str, IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_bind",
        params: &[IntrinsicType::Int, IntrinsicType::Str, IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_listen",
        params: &[IntrinsicType::Int, IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_accept",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_send",
        params: &[IntrinsicType::Int, IntrinsicType::Str],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_recv",
        params: &[IntrinsicType::Int, IntrinsicType::Int],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_net_close",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_shutdown",
        params: &[IntrinsicType::Int, IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_net_getaddrinfo",
        params: &[IntrinsicType::Str, IntrinsicType::Int],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_net_freeaddrinfo",
        params: &[],
        result: IntrinsicType::Unit,
    },
    Intrinsic {
        name: "rt_net_gethostname",
        params: &[],
        result: IntrinsicType::Str,
    },
    Intrinsic {
        name: "rt_net_htons",
        params: &[IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    // --- Crypto (Session 71) ---
    Intrinsic {
        name: "rt_crypto_init",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_crypto_random_bytes",
        params: &[IntrinsicType::Str, IntrinsicType::Int],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_crypto_random_int",
        params: &[],
        result: IntrinsicType::Int,
    },
    Intrinsic {
        name: "rt_crypto_secure_zero",
        params: &[IntrinsicType::Int, IntrinsicType::Int],
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
