//! The reference implementation of the deterministic heap.
//!
//! This is the *specification* of the heap the backend's machine-level
//! runtime implements (see `src/backend/emit/runtime.rs`): a validated
//! bump allocator with an address-ordered free list, exact block splitting
//! and adjacent-block coalescing, behind a bounded liveness table.
//! Keeping a pure-Rust, safe reference lets the tests validate the
//! algorithm — alignment, bounds, reuse, leak detection, and every error
//! path — without running machine code, and gives later safety work a
//! single authoritative statement of the semantics.
//!
//! The model:
//!
//! - addresses are **offsets into the logical arena**
//!   (`0 .. HEAP_MAX_BYTES`), which the machine runtime translates to
//!   absolute addresses (the `.bss` prefix for the first
//!   [`HEAP_BSS_BYTES`] of the space, the lazily reserved growth region
//!   beyond it); the reference model backs its arena sparsely, so tests
//!   can exercise far more than the `.bss` prefix without allocating it;
//! - allocation rounds the requested size up to [`ALLOC_ALIGNMENT`],
//!   then first-fits the free list in **address order** (each free block
//!   is tried in turn; too-small blocks are skipped and stay reusable),
//!   falling back to the bump cursor when nothing fits;
//! - a reused block larger than the request is **split**: the live block
//!   gets exactly the requested size and the remainder is re-inserted as
//!   an independent free block, so skipped capacity is never discarded;
//! - `free` inserts the block in address order and **coalesces** it with
//!   an immediately preceding and/or following free block, so the free
//!   list never holds two adjacent blocks and mixed-size churn returns to
//!   the block it carved (this is what keeps a 64 KiB/1 KiB churn inside
//!   one 64 KiB block instead of consuming the arena);
//! - every live allocation occupies one slot of a fixed-size table; a
//!   slot records the block's start offset and size (exactly the
//!   requested, aligned size — never the reused block's original size);
//! - `free` requires the pointer to be the exact 16-aligned start of a
//!   **live** slot — a double free, a never-allocated pointer, an
//!   interior pointer, or a misaligned pointer is a structured error;
//! - `load`/`store` require the 8-byte word to lie entirely inside a live
//!   slot;
//! - the reference models the arena bytes too, so load/store round-trips
//!   are exercised end to end. The backing store is sparse: it grows only
//!   to the highest address actually written, so the logical capacity can
//!   be far larger than any single test's working set.

use super::abi::{
    ALLOC_ALIGNMENT, HEAP_BSS_BYTES, HEAP_MAX_BYTES, MAX_LIVE_ALLOCS, WORD_SIZE, align_up,
};
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
/// (an offset), an intrusive free list sorted by address, a fixed-size
/// liveness table, and the arena bytes themselves.
#[derive(Debug, Clone)]
pub struct Allocator {
    /// Committed bytes from the arena base (the high-water mark).
    cursor: u64,
    /// The free list, sorted by start offset and holding no two adjacent
    /// blocks. Each entry is (start_offset, size).
    free: Vec<(u64, u64)>,
    /// The liveness table: `MAX_LIVE_ALLOCS` slots, `None` when dead.
    table: Vec<Option<LiveEntry>>,
    /// The arena contents, grown lazily up to the highest address any
    /// access has touched. Freshly grown regions read as zero (matching
    /// the loader/first-commit contract); reused blocks retain their old
    /// contents (matching the machine runtime).
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
            memory: Vec::new(),
        }
    }

    /// The number of bytes of the `.bss` prefix arena.
    pub fn bss_prefix_size() -> u64 {
        HEAP_BSS_BYTES
    }

    /// The logical arena capacity in bytes.
    pub fn heap_size(&self) -> u64 {
        HEAP_MAX_BYTES
    }

    /// Extends the sparse backing store so the word at `addr` is present
    /// (zero-filled on first touch).
    fn ensure_memory(&mut self, addr: u64, len: u64) {
        let end = addr + len;
        if end > self.memory.len() as u64 {
            self.memory.resize(end as usize, 0);
        }
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
        // First-fit over the address-ordered free list. A too-small block
        // is skipped and stays on the list (it may serve a later small
        // request); a block that fits is reused, and any capacity beyond
        // the request is re-inserted as a free block of its own. Dropping
        // either the skipped blocks or the remainder loses capacity that
        // was already released, which is exactly how mixed-size churn used
        // to grow the arena by a large block per iteration until `E-R02`.
        let start = match self.free.iter().position(|&(_, old_size)| old_size >= size) {
            Some(index) => {
                let (block, old_size) = self.free.remove(index);
                let remainder = old_size - size;
                if remainder > 0 {
                    // Sizes are multiples of ALLOC_ALIGNMENT and >= one
                    // step, so the remainder is a legal free block. It
                    // starts above the reused block and below the next
                    // free block (the list never holds adjacent blocks),
                    // so re-inserting at the fit's index keeps the order.
                    debug_assert_eq!(remainder % ALLOC_ALIGNMENT, 0);
                    self.free.insert(index, (block + size, remainder));
                }
                block
            }
            None => {
                if self.cursor + size > HEAP_MAX_BYTES {
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
    /// The block is inserted in address order and coalesced with an
    /// adjacent free block before and/or after it, so released capacity is
    /// always reusable and the list holds no two adjacent entries.
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

        // Insert in address order: `index` is the first free block above
        // the released one (or the end of the list).
        let index = self.free.partition_point(|&(start, _)| start < ptr);
        let mut block = ptr;
        let mut size = entry.size;

        // Coalesce with the preceding free block when it ends exactly at
        // the released block's start.
        if index > 0 {
            let (prev_start, prev_size) = self.free[index - 1];
            if prev_start + prev_size == block {
                self.free.remove(index - 1);
                block = prev_start;
                size += prev_size;
            }
        }
        // Coalesce with the following free block when it starts exactly at
        // the (possibly merged) block's end.
        let after = self.free.partition_point(|&(start, _)| start < block);
        if let Some(&(next_start, next_size)) = self.free.get(after) {
            if block + size == next_start {
                self.free.remove(after);
                size += next_size;
            }
        }
        self.free.insert(after, (block, size));
        Ok(())
    }

    /// Loads the 8-byte word at `addr` (an arena offset).
    ///
    /// Errors: [`RuntimeErrorKind::Misaligned`] for addresses that are not
    /// 8-byte aligned, and [`RuntimeErrorKind::OutOfBounds`] when the word
    /// is not entirely inside a live allocation.
    pub fn load(&self, addr: u64) -> Result<u64, RuntimeError> {
        self.check_access(addr)?;
        // The access is inside a live block, so the sparse backing store
        // always already covers it (alloc marks nothing; only stores grow
        // it, and a load of never-stored bytes reads zero-initialized
        // backing).
        let slice = self
            .memory
            .get(addr as usize..addr as usize + 8)
            .unwrap_or(&[0u8; 8]);
        Ok(u64::from_le_bytes(slice.try_into().unwrap()))
    }

    /// Stores the 8-byte word `value` at `addr` (an arena offset).
    ///
    /// Errors: [`RuntimeErrorKind::Misaligned`] for addresses that are not
    /// 8-byte aligned, and [`RuntimeErrorKind::OutOfBounds`] when the word
    /// is not entirely inside a live allocation.
    pub fn store(&mut self, addr: u64, value: u64) -> Result<(), RuntimeError> {
        self.check_access(addr)?;
        self.ensure_memory(addr, 8);
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

    // ------------------------------------------------------------------
    // String blobs
    // ------------------------------------------------------------------

    /// Allocates a string blob of `size` bytes: a heap block of `8 + size`
    /// bytes whose first word holds the length prefix `size` (little-
    /// endian). The data bytes are zero on fresh bumps and retain old
    /// contents on free-list reuse, matching [`Allocator::alloc`]. A
    /// negative size (an `i64` value with the sign bit set) is rejected
    /// with `E-R08`, mirroring the machine runtime's `StrAlloc` guard.
    /// Errors: the same as [`Allocator::alloc`] (`E-R02`/`E-R03`/`E-R08`).
    pub fn alloc_string(&mut self, size: u64) -> Result<u64, RuntimeError> {
        if size > i64::MAX as u64 {
            return Err(RuntimeError::new(RuntimeErrorKind::InvalidSize, Some(size)));
        }
        let start = self.alloc(size + WORD_SIZE)?;
        self.ensure_memory(start, WORD_SIZE);
        self.memory[start as usize..start as usize + 8].copy_from_slice(&size.to_le_bytes());
        Ok(start)
    }

    /// The byte length of the string blob at `s`: the length prefix of a
    /// live heap block. A pointer that is not the exact start of a live
    /// block (a freed string, an interior pointer, a literal address — the
    /// machine runtime additionally accepts image literals, which the
    /// reference model does not contain) is an out-of-bounds error
    /// (`E-R05`).
    pub fn string_len(&self, s: u64) -> Result<u64, RuntimeError> {
        self.check_string(s)?;
        let slice = self
            .memory
            .get(s as usize..s as usize + 8)
            .unwrap_or(&[0u8; 8]);
        Ok(u64::from_le_bytes(slice.try_into().unwrap()))
    }

    /// The byte at `index` of the string blob at `s`, bounds-checked
    /// against the length prefix. A bad index is `E-R09`; a bad pointer is
    /// `E-R05`.
    pub fn string_byte(&self, s: u64, index: u64) -> Result<u8, RuntimeError> {
        let len = self.string_len(s)?;
        if index >= len {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StringIndexOutOfRange,
                Some(index),
            ));
        }
        Ok(self
            .memory
            .get(s as usize + 8 + index as usize)
            .copied()
            .unwrap_or(0))
    }

    /// Writes the byte `value` at `index` of the string blob at `s`,
    /// bounds-checked against the length prefix. A bad index is `E-R09`; a
    /// bad pointer is `E-R05`.
    pub fn set_string_byte(&mut self, s: u64, index: u64, value: u8) -> Result<(), RuntimeError> {
        let len = self.string_len(s)?;
        if index >= len {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StringIndexOutOfRange,
                Some(index),
            ));
        }
        self.ensure_memory(s + 8 + index, 1);
        self.memory[s as usize + 8 + index as usize] = value;
        Ok(())
    }

    /// Validates that `s` is the exact start of a live heap block large
    /// enough to hold its length prefix, mirroring the machine runtime's
    /// heap-string validation (`E-R05` otherwise).
    fn check_string(&self, s: u64) -> Result<(), RuntimeError> {
        let valid = self
            .table
            .iter()
            .flatten()
            .any(|entry| entry.start == s && entry.size >= WORD_SIZE);
        if valid {
            Ok(())
        } else {
            Err(RuntimeError::new(RuntimeErrorKind::OutOfBounds, Some(s)))
        }
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

    /// The free list, sorted by start offset.
    /// Each entry is (start_offset, size).
    pub(crate) fn free_list(&self) -> &[(u64, u64)] {
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
        // Use ALLOC_ALIGNMENT as the assumed size for corrupted entries.
        self.free.push((offset, ALLOC_ALIGNMENT));
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
    fn free_reuses_the_freed_block() {
        let mut heap = Allocator::new();
        let _a = heap.alloc(16).unwrap();
        let b = heap.alloc(16).unwrap();
        heap.free(b).unwrap();
        // The next allocation reuses b's block.
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
        // Two half-capacity blocks fit; the third does not. The sparse
        // backing store keeps this cheap even at the full logical size.
        heap.alloc(HEAP_MAX_BYTES / 2).unwrap();
        heap.alloc(HEAP_MAX_BYTES / 2).unwrap();
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
        assert_eq!(HEAP_MAX_BYTES % ALLOC_ALIGNMENT, 0);
        assert_eq!(HEAP_BSS_BYTES % ALLOC_ALIGNMENT, 0);
        assert_eq!(
            HEAP_MAX_BYTES % crate::runtime::abi::HEAP_COMMIT_CHUNK,
            0
        );
    }

    #[test]
    fn alloc_string_writes_the_length_prefix() {
        let mut heap = Allocator::new();
        let s = heap.alloc_string(3).unwrap();
        assert_eq!(heap.string_len(s).unwrap(), 3);
        // The block is 8 + 3 → 16 aligned bytes.
        assert_eq!(heap.live_entries().next().unwrap().size, ALLOC_ALIGNMENT);
    }

    #[test]
    fn string_bytes_round_trip() {
        let mut heap = Allocator::new();
        let s = heap.alloc_string(5).unwrap();
        for (index, byte) in b"hello".iter().enumerate() {
            heap.set_string_byte(s, index as u64, *byte).unwrap();
        }
        let mut bytes = Vec::new();
        for index in 0..5u64 {
            bytes.push(heap.string_byte(s, index).unwrap());
        }
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn string_index_out_of_range_is_e_ro9() {
        let mut heap = Allocator::new();
        let s = heap.alloc_string(4).unwrap();
        assert_eq!(
            heap.string_byte(s, 4).unwrap_err().kind(),
            RuntimeErrorKind::StringIndexOutOfRange
        );
        assert_eq!(
            heap.set_string_byte(s, 4, 1).unwrap_err().kind(),
            RuntimeErrorKind::StringIndexOutOfRange
        );
        assert_eq!(
            heap.string_byte(s, 5).unwrap_err().kind(),
            RuntimeErrorKind::StringIndexOutOfRange
        );
    }

    #[test]
    fn string_allocation_of_negative_size_is_e_ro8() {
        let mut heap = Allocator::new();
        // The machine runtime's `StrAlloc` rejects sizes with the sign bit
        // set; the reference mirrors that guard (no wrap-around).
        assert_eq!(
            heap.alloc_string(u64::MAX).unwrap_err().kind(),
            RuntimeErrorKind::InvalidSize
        );
        assert_eq!(
            heap.alloc_string(i64::MAX as u64 + 1).unwrap_err().kind(),
            RuntimeErrorKind::InvalidSize
        );
    }

    #[test]
    fn string_ops_on_freed_or_foreign_blocks_are_e_ro5() {
        let mut heap = Allocator::new();
        let s = heap.alloc_string(4).unwrap();
        let block = heap.alloc(16).unwrap();
        // Any live block start reads as a string: block kinds are not
        // tracked (the type system prevents word blocks from reaching the
        // string intrinsics). The word block's first word is zero-filled.
        assert_eq!(heap.string_len(block).unwrap(), 0);
        // A freed string is dead: reads are out-of-bounds.
        heap.free(s).unwrap();
        assert_eq!(
            heap.string_len(s).unwrap_err().kind(),
            RuntimeErrorKind::OutOfBounds
        );
        assert_eq!(
            heap.string_byte(s, 0).unwrap_err().kind(),
            RuntimeErrorKind::OutOfBounds
        );
        // A pointer that was never allocated is out-of-bounds too.
        assert_eq!(
            heap.string_len(4096).unwrap_err().kind(),
            RuntimeErrorKind::OutOfBounds
        );
    }

    #[test]
    fn string_allocation_is_deterministic() {
        let mut first = Allocator::new();
        let mut second = Allocator::new();
        for size in [0u64, 1, 8, 33] {
            assert_eq!(
                first.alloc_string(size).unwrap(),
                second.alloc_string(size).unwrap()
            );
        }
        // Zero-length strings are valid: an 8-byte length prefix only.
        assert_eq!(first.string_len(0).unwrap(), 0);
    }
}
