//! Structured runtime diagnostics: stable `E-R01`+ codes.
//!
//! Every runtime failure carries a stable machine-readable code, a
//! human-readable message, and a deterministic process exit code
//! ([`EXIT_CODE_BASE`](super::abi::EXIT_CODE_BASE) + error number). The
//! machine-level runtime writes `message()` to stderr and terminates with
//! `exit_code()`; the tests and tooling rely on both being stable.

use std::fmt;

use super::abi::EXIT_CODE_BASE;

/// The category of a runtime error.
///
/// The machine-level runtime numbers errors 1-based (`E-R01` is error
/// number 1), and the exit code is [`EXIT_CODE_BASE`](super::abi::EXIT_CODE_BASE)
/// plus the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeErrorKind {
    /// Runtime initialization failed. Reserved: initialization is
    /// currently infallible, but the code is part of the stable catalog.
    InitFailed,
    /// The heap arena is exhausted: a request cannot be satisfied by the
    /// free list or the bump cursor.
    OutOfMemory,
    /// The liveness table is full: more than [`MAX_LIVE_ALLOCS`](super::abi::MAX_LIVE_ALLOCS)
    /// allocations are simultaneously live.
    TableExhausted,
    /// An invalid free: the pointer is not the exact start of a live
    /// allocation (a double free, a never-allocated pointer, an interior
    /// pointer, or `null`).
    InvalidFree,
    /// An out-of-bounds memory access: a load or store whose 8-byte word is
    /// not fully inside a live allocation.
    OutOfBounds,
    /// A memory leak: live allocations remain when the program exits.
    Leak,
    /// A misaligned access: a free, load, or store address that is not
    /// aligned to its access width.
    Misaligned,
    /// An invalid allocation size: `rt_alloc` called with a non-positive
    /// size.
    InvalidSize,
    /// A string index out of range: `rt_str_byte` / `rt_str_set_byte`
    /// called with an index not below the string's byte length.
    StringIndexOutOfRange,
}

impl RuntimeErrorKind {
    /// The 1-based error number used by the machine runtime and the exit
    /// code.
    pub fn number(self) -> u64 {
        match self {
            Self::InitFailed => 1,
            Self::OutOfMemory => 2,
            Self::TableExhausted => 3,
            Self::InvalidFree => 4,
            Self::OutOfBounds => 5,
            Self::Leak => 6,
            Self::Misaligned => 7,
            Self::InvalidSize => 8,
            Self::StringIndexOutOfRange => 9,
        }
    }

    /// The stable machine-readable code of this error (e.g. `E-R02`).
    pub fn code(self) -> &'static str {
        match self {
            Self::InitFailed => "E-R01",
            Self::OutOfMemory => "E-R02",
            Self::TableExhausted => "E-R03",
            Self::InvalidFree => "E-R04",
            Self::OutOfBounds => "E-R05",
            Self::Leak => "E-R06",
            Self::Misaligned => "E-R07",
            Self::InvalidSize => "E-R08",
            Self::StringIndexOutOfRange => "E-R09",
        }
    }

    /// The diagnostic message the runtime writes to stderr before
    /// terminating. The full line is `mink: runtime error[CODE]: message`.
    pub fn message(self) -> &'static str {
        match self {
            Self::InitFailed => "runtime initialization failed",
            Self::OutOfMemory => "out of memory: the runtime heap is exhausted",
            Self::TableExhausted => "allocation table exhausted: too many live allocations",
            Self::InvalidFree => "invalid free: the pointer is not the start of a live allocation",
            Self::OutOfBounds => "invalid memory access: the address is outside a live allocation",
            Self::Leak => "memory leak: live allocations remain at exit",
            Self::Misaligned => "misaligned access: the address is not properly aligned",
            Self::InvalidSize => "invalid allocation size: the size must be positive",
            Self::StringIndexOutOfRange => {
                "string index out of range: the index must be below the string's byte length"
            }
        }
    }

    /// The process exit code the runtime terminates with for this error
    /// (`100 + number`).
    pub fn exit_code(self) -> u64 {
        EXIT_CODE_BASE + self.number()
    }
}

/// A single runtime error: a stable category plus the value involved where
/// relevant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeError {
    kind: RuntimeErrorKind,
    /// The value that triggered the error (an address, size, or count),
    /// where the category is value-specific.
    value: Option<u64>,
}

impl RuntimeError {
    /// Creates a runtime error of `kind`; `value` describes the offending
    /// address/size where the category is value-specific.
    pub fn new(kind: RuntimeErrorKind, value: Option<u64>) -> Self {
        Self { kind, value }
    }

    /// The category of this error.
    pub fn kind(&self) -> RuntimeErrorKind {
        self.kind
    }

    /// The stable machine-readable code of this error (e.g. `E-R02`).
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// The value that triggered the error, when relevant.
    pub fn value(&self) -> Option<u64> {
        self.value
    }

    /// The process exit code this error maps to.
    pub fn exit_code(&self) -> u64 {
        self.kind.exit_code()
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "runtime error[{}]: {}", self.code(), self.kind.message())?;
        if let Some(value) = self.value {
            write!(f, " (value: {value})")?;
        }
        Ok(())
    }
}
