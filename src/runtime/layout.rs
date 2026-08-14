//! Explicit memory-layout representation.
//!
//! [`MemoryLayout`] is the authoritative size/alignment description of a
//! value, the representation future structs, arrays, and strings will be
//! built from. The current language has exactly three layout classes
//! ([`ValueClass`]); the machinery — size, alignment, and the guarantee
//! that layout is deterministic and documented — is the foundation for
//! aggregates.
//!
//! Rules of the memory model:
//!
//! - every value's size is a multiple of its alignment;
//! - alignment is always a power of two;
//! - a `Word` value occupies one 8-byte word (an integer or a boolean
//!   stored as `0`/`1`);
//! - a `Range<Int>` occupies two consecutive 8-byte words;
//! - a `Unit` value occupies no storage: it is the type of an expression
//!   that produces no value, and it is never materialized;
//! - heap blocks are rounded up to the allocator alignment (16) regardless
//!   of the value they hold, so every block can hold any value class.

use super::abi::{ALLOC_ALIGNMENT, WORD_SIZE};

/// The layout classes the current memory model distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueClass {
    /// One machine word: `Int` or `Bool` (stored as `0`/`1`).
    Word,
    /// Two consecutive words: `Range<Int>`.
    TwoWords,
    /// No storage: a value that is never materialized.
    Unit,
}

/// The size (in bytes) and alignment (in bytes) of a [`ValueClass`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryLayout {
    /// The size of the value in bytes.
    pub size: u64,
    /// The alignment of the value in bytes (a power of two).
    pub align: u64,
}

impl MemoryLayout {
    /// The layout of [`ValueClass::Word`].
    pub fn word() -> Self {
        Self {
            size: WORD_SIZE,
            align: WORD_SIZE,
        }
    }

    /// The layout of [`ValueClass::TwoWords`].
    pub fn two_words() -> Self {
        Self {
            size: 2 * WORD_SIZE,
            align: WORD_SIZE,
        }
    }

    /// The layout of [`ValueClass::Unit`]: zero size, alignment 1.
    pub fn unit() -> Self {
        Self { size: 0, align: 1 }
    }

    /// The layout of a [`ValueClass`].
    pub fn of(class: ValueClass) -> Self {
        match class {
            ValueClass::Word => Self::word(),
            ValueClass::TwoWords => Self::two_words(),
            ValueClass::Unit => Self::unit(),
        }
    }

    /// The size of a [`ValueClass`].
    pub fn size_of(class: ValueClass) -> u64 {
        Self::of(class).size
    }

    /// The alignment of a [`ValueClass`].
    pub fn align_of(class: ValueClass) -> u64 {
        Self::of(class).align
    }

    /// The size of a heap block holding a value of `class`: the value's
    /// size rounded up to the allocator alignment, so every block is
    /// independently addressable at the allocator's alignment.
    pub fn heap_block_size(class: ValueClass) -> u64 {
        let size = Self::size_of(class);
        if size == 0 {
            0
        } else {
            size.div_ceil(ALLOC_ALIGNMENT) * ALLOC_ALIGNMENT
        }
    }
}
