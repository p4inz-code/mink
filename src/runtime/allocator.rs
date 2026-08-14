//! The reference implementation of the deterministic heap.
//!
//! This is the *specification* of the heap the backend's machine-level
//! runtime implements (see `src/backend/emit/runtime.rs`): a validated
//! bump allocator with LIFO free-list reuse and a bounded liveness table.
//! Keeping a pure-Rust, safe reference lets the tests validate the
//! algorithm — alignment, bounds, reuse, leak detection, and every error
//! path — without running machine code, and gives later safety work a
//! single authoritative statement of the semantics.
//!
//! The model:
//!
//! - addresses are **offsets into the arena** (`0 .. HEAP_SIZE`), which
//!   the machine runtime translates to absolute addresses (`arena + offset`);
//! - allocation rounds the requested size up to [`ALLOC_ALIGNMENT`] and
//!   either reuses the most recently freed block or bumps the cursor;
//! - every live allocation occupies one slot of a fixed-size table; a
//!   slot records the block's start offset and size;
//! - `free` requires the pointer to be the exact 16-aligned start of a
//!   **live** slot — a double free, a never-allocated pointer, an
//!   interior pointer, or a misaligned pointer is a structured error;
//! - `load`/`store` require the 8-byte word to lie entirely inside a live
//!   slot;
//! - the reference models the arena bytes too, so load/store round-trips
//!   are exercised end to end.

use super::abi::{ALLOC_ALIGNMENT, HEAP_SIZE, MAX_LIVE_ALLOCS, WORD_SIZE, align_up};
use super::error::{RuntimeError, RuntimeErrorKind};

/// A slot of the liveness table: the live flag plus the block's start
/// offset and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveEntry {
    /// The block's start offset in the arena.
    pub start: u64,
    /// The block's aligned size in bytes.
    pub size: u64,
}

/// The reference deterministic heap.
///
/// The state mirrors the machine runtime's `.bss` layout: a bump cursor
/// (an offset), a LIFO free list (offsets), a fixed-size liveness table,
/// and the arena bytes themselves.
#[derive(Debug, Clone)]
pub struct Allocator {
    /// Committed bytes from the arena base (the high-water mark).
    cursor: u64,
    /// The LIFO free list, most recently freed first.
    free: Vec<u64>,
    /// The liveness table: `MAX_LIVE_ALLOCS` slots, `None` when dead.
    table: Vec<Option<LiveEntry>>,
    /// The arena contents, `HEAP_SIZE` zero-initialized bytes.
    memory: Vec<u8>,
}

impl Default for Allocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Allocator {
    /// Creates an empty deterministic heap.
    pub fn new() -> Self {
        Self {
            cursor: 0,
            free: Vec::new(),
            table: vec![None; MAX_LIVE_ALLOCS],
            memory: vec![0; HEAP_SIZE as usize],
        }
    }

    /// The arena size in bytes.
    pub fn heap_size(&self) -> u64 {
        HEAP_SIZE
    }

    /// The number of simultaneously live allocations.
    pub fn live_count(&self) -> usize {
        self.table.iter().filter(|slot| slot.is_some()).count()
    }

    /// The currently live allocations, in table-slot order.
    pub fn live_entries(&self) -> impl Iterator<Item = LiveEntry> + '_ {
        self.table.iter().flatten().copied()
    }

    /// Allocates a block of at least `size` bytes, returning its start
    /// offset in the arena.
    ///
    /// Errors: [`RuntimeErrorKind::InvalidSize`] for non-positive sizes,
    /// [`RuntimeErrorKind::OutOfMemory`] when the arena is exhausted, and
    /// [`RuntimeErrorKind::TableExhausted`] when every liveness slot is
    /// live. The returned block is zero-filled from previous use (fresh
    /// bumps read zeros from the loader; reused blocks retain their old
    /// contents, matching the machine runtime).
    pub fn alloc(&mut self, size: u64) -> Result<u64, RuntimeError> {
        if size == 0 || size > i64::MAX as u64 {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidSize, Some(size)));
        }
        let size = align_up(size, ALLOC_ALIGNMENT);
        let start = match self.free.pop() {
            // Reuse the most recently freed block.
            Some(block) => block,
            None => {
                if self.cursor + size > HEAP_SIZE {
                    return Err(RuntimeError::new(RuntimeErrorKind::OutOfMemory, Some(size)));
                }
                let block = self.cursor;
                self.cursor += size;
                block
            }
        };
        let slot = self
            .table
            .iter()
            .position(|slot| slot.is_none())
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::TableExhausted, None))?;
        self.table[slot] = Some(LiveEntry { start, size });
        Ok(start)
    }

    /// Frees the block at `ptr` (an arena offset), returning it to the
    /// free list.
    ///
    /// Errors: [`RuntimeErrorKind::InvalidFree`] when the pointer is not
    /// the exact start of a live allocation (covers double frees,
    /// never-allocated pointers, interior pointers, and `null`), and
    /// [`RuntimeErrorKind::Misaligned`] for pointers that are not
    /// [`ALLOC_ALIGNMENT`]-aligned.
    pub fn free(&mut self, ptr: u64) -> Result<(), RuntimeError> {
        // In this offset model the first block legitimately starts at 0;
        // the machine runtime's null check targets the absolute address 0
        // (its blocks live at `arena + offset`, so no block is ever 0).
        if ptr % ALLOC_ALIGNMENT != 0 {
            return Err(RuntimeError::new(RuntimeErrorKind::Misaligned, Some(ptr)));
        }
        let slot = self
            .table
            .iter()
            .position(|slot| matches!(slot, Some(entry) if entry.start == ptr))
            .ok_or_else(|| RuntimeError::new(RuntimeErrorKind::InvalidFree, Some(ptr)))?;
        let Some(entry) = self.table[slot].take() else {
            unreachable!("a matching slot is always live")
        };
        debug_assert_eq!(entry.start, ptr);
        self.free.push(ptr);
        Ok(())
    }

    /// Loads the 8-byte word at `addr` (an arena offset).
    ///
    /// Errors: [`RuntimeErrorKind::Misaligned`] for addresses that are not
    /// 8-byte aligned, and [`RuntimeErrorKind::OutOfBounds`] when the word
    /// is not entirely inside a live allocation.
    pub fn load(&self, addr: u64) -> Result<u64, RuntimeError> {
        self.check_access(addr)?;
        Ok(u64::from_le_bytes(
            self.memory[addr as usize..addr as usize + 8]
                .try_into()
                .unwrap(),
        ))
    }

    /// Stores the 8-byte word `value` at `addr` (an arena offset).
    ///
    /// Errors: [`RuntimeErrorKind::Misaligned`] for addresses that are not
    /// 8-byte aligned, and [`RuntimeErrorKind::OutOfBounds`] when the word
    /// is not entirely inside a live allocation.
    pub fn store(&mut self, addr: u64, value: u64) -> Result<(), RuntimeError> {
        self.check_access(addr)?;
        self.memory[addr as usize..addr as usize + 8].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    /// Validates that `addr` is 8-byte aligned and that the word at
    /// `addr` lies entirely inside one live allocation.
    fn check_access(&self, addr: u64) -> Result<(), RuntimeError> {
        if addr % WORD_SIZE != 0 {
            return Err(RuntimeError::new(RuntimeErrorKind::Misaligned, Some(addr)));
        }
        let contained = self
            .table
            .iter()
            .flatten()
            .any(|entry| addr >= entry.start && addr + WORD_SIZE <= entry.start + entry.size);
        if contained {
            Ok(())
        } else {
            Err(RuntimeError::new(RuntimeErrorKind::OutOfBounds, Some(addr)))
        }
    }

    /// The live allocations remaining, in table-slot order. A non-empty
    /// result at exit is a leak (`E-R06`).
    pub fn leaks(&self) -> Vec<LiveEntry> {
        self.table.iter().flatten().copied().collect()
    }
}

// ---------------------------------------------------------------------------
// Internal helpers shared with the verifier
// ---------------------------------------------------------------------------

impl Allocator {
    /// The bump cursor (committed bytes from the arena base).
    pub(crate) fn cursor(&self) -> u64 {
        self.cursor
    }

    /// The LIFO free list, most recently freed first.
    pub(crate) fn free_list(&self) -> &[u64] {
        &self.free
    }

    /// The liveness table slots.
    pub(crate) fn table(&self) -> &[Option<LiveEntry>] {
        &self.table
    }

    /// Test-only corruption helper: records `entry` in `slot`, even when
    /// the slot is already occupied (used by the verifier tests to build
    /// invalid states).
    #[cfg(test)]
    pub(crate) fn corrupt_set_live(&mut self, slot: usize, entry: LiveEntry) {
        self.table[slot] = Some(entry);
    }

    /// Test-only corruption helper: pushes `offset` onto the free list
    /// without validating it (used by the verifier tests to build invalid
    /// states).
    #[cfg(test)]
    pub(crate) fn corrupt_push_free(&mut self, offset: u64) {
        self.free.push(offset);
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::error::RuntimeErrorKind;

    #[test]
    fn alloc_returns_aligned_blocks() {
        let mut heap = Allocator::new();
        let a = heap.alloc(1).unwrap();
        let b = heap.alloc(10).unwrap();
        let c = heap.alloc(17).unwrap();
        assert_eq!(a, 0);
        assert_eq!(b, ALLOC_ALIGNMENT);
        assert_eq!(c, 2 * ALLOC_ALIGNMENT);
        assert!(a % ALLOC_ALIGNMENT == 0 && b % ALLOC_ALIGNMENT == 0 && c % ALLOC_ALIGNMENT == 0);
        assert_eq!(heap.live_count(), 3);
    }

    #[test]
    fn free_reuses_the_most_recent_block() {
        let mut heap = Allocator::new();
        let _a = heap.alloc(16).unwrap();
        let b = heap.alloc(16).unwrap();
        heap.free(b).unwrap();
        // The next allocation reuses b's block (LIFO).
        let c = heap.alloc(16).unwrap();
        assert_eq!(c, b);
        assert_eq!(heap.live_count(), 2);
        let d = heap.alloc(16).unwrap();
        // a and b bumped the cursor to 2 * ALLOC_ALIGNMENT; c reused b's
        // block, so the next fresh block starts there.
        assert_eq!(d, 2 * ALLOC_ALIGNMENT);
    }

    #[test]
    fn load_store_round_trip() {
        let mut heap = Allocator::new();
        let block = heap.alloc(32).unwrap();
        heap.store(block, 0xDEAD_BEEF).unwrap();
        heap.store(block + 8, 42).unwrap();
        assert_eq!(heap.load(block).unwrap(), 0xDEAD_BEEF);
        assert_eq!(heap.load(block + 8).unwrap(), 42);
    }

    #[test]
    fn invalid_size_is_rejected() {
        let mut heap = Allocator::new();
        assert_eq!(
            heap.alloc(0).unwrap_err().kind(),
            RuntimeErrorKind::InvalidSize
        );
    }

    #[test]
    fn out_of_memory_is_reported() {
        let mut heap = Allocator::new();
        // Two 512 KiB blocks fit (1 MiB arena); the third does not.
        heap.alloc(HEAP_SIZE / 2).unwrap();
        heap.alloc(HEAP_SIZE / 2).unwrap();
        assert_eq!(
            heap.alloc(16).unwrap_err().kind(),
            RuntimeErrorKind::OutOfMemory
        );
    }

    #[test]
    fn table_exhaustion_is_reported() {
        let mut heap = Allocator::new();
        for _ in 0..MAX_LIVE_ALLOCS {
            heap.alloc(16).unwrap();
        }
        assert_eq!(
            heap.alloc(16).unwrap_err().kind(),
            RuntimeErrorKind::TableExhausted
        );
    }

    #[test]
    fn double_free_is_rejected() {
        let mut heap = Allocator::new();
        let block = heap.alloc(16).unwrap();
        heap.free(block).unwrap();
        assert_eq!(
            heap.free(block).unwrap_err().kind(),
            RuntimeErrorKind::InvalidFree
        );
    }

    #[test]
    fn null_and_interior_frees_are_rejected() {
        let mut heap = Allocator::new();
        // Block 0 lives at offset 0 (a valid block in this model), so the
        // first free of it succeeds; the second is a null/double free.
        heap.alloc(16).unwrap();
        let block = heap.alloc(64).unwrap();
        heap.free(0).unwrap();
        assert_eq!(
            heap.free(0).unwrap_err().kind(),
            RuntimeErrorKind::InvalidFree
        );
        assert_eq!(
            heap.free(block + 16).unwrap_err().kind(),
            RuntimeErrorKind::InvalidFree
        );
        assert_eq!(
            heap.free(block + 8).unwrap_err().kind(),
            RuntimeErrorKind::Misaligned
        );
    }

    #[test]
    fn misaligned_load_and_store_are_rejected() {
        let mut heap = Allocator::new();
        let block = heap.alloc(32).unwrap();
        assert_eq!(
            heap.load(block + 4).unwrap_err().kind(),
            RuntimeErrorKind::Misaligned
        );
        assert_eq!(
            heap.store(block + 4, 1).unwrap_err().kind(),
            RuntimeErrorKind::Misaligned
        );
    }

    #[test]
    fn out_of_bounds_access_is_rejected() {
        let mut heap = Allocator::new();
        let block = heap.alloc(16).unwrap();
        // Inside the arena but outside the live block.
        assert_eq!(
            heap.load(block + 16).unwrap_err().kind(),
            RuntimeErrorKind::OutOfBounds
        );
        assert_eq!(
            heap.store(block + 16, 1).unwrap_err().kind(),
            RuntimeErrorKind::OutOfBounds
        );
        // A misaligned address inside the block is caught by the
        // alignment check before the bounds scan.
        assert_eq!(
            heap.load(block + 12).unwrap_err().kind(),
            RuntimeErrorKind::Misaligned
        );
    }

    #[test]
    fn use_after_free_is_an_out_of_bounds_access() {
        let mut heap = Allocator::new();
        let block = heap.alloc(16).unwrap();
        heap.free(block).unwrap();
        assert_eq!(
            heap.load(block).unwrap_err().kind(),
            RuntimeErrorKind::OutOfBounds
        );
    }

    #[test]
    fn leak_detection_reports_live_allocations() {
        let mut heap = Allocator::new();
        let a = heap.alloc(16).unwrap();
        let b = heap.alloc(16).unwrap();
        let c = heap.alloc(32).unwrap();
        heap.free(a).unwrap();
        heap.free(b).unwrap();
        let leaks = heap.leaks();
        assert_eq!(leaks.len(), 1);
        assert_eq!(leaks[0].start, c);
        assert_eq!(leaks[0].size, 32);
    }

    #[test]
    fn allocation_is_deterministic() {
        let mut first = Allocator::new();
        let mut second = Allocator::new();
        for size in [1u64, 16, 100, 7, 1024] {
            let a = first.alloc(size).unwrap();
            let b = second.alloc(size).unwrap();
            assert_eq!(a, b, "identical sequences allocate identically");
        }
    }

    #[test]
    fn allocator_layout_constants_are_consistent() {
        // Every live entry must fit exactly in LIVE_ENTRY_BYTES.
        assert_eq!(std::mem::size_of::<LiveEntry>(), 16);
        const _: () = assert!(crate::runtime::abi::LIVE_ENTRY_BYTES >= 16);
        assert_eq!(HEAP_SIZE % ALLOC_ALIGNMENT, 0);
    }
}
