//! Runtime verifier: invariant checks over allocator state.
//!
//! The machine-level runtime enforces its invariants operationally — every
//! operation validates its inputs against the liveness table before
//! touching memory. This module states those invariants explicitly and
//! checks them over a reference [`Allocator`], so tests and tooling can
//! verify that a heap state is internally consistent: a valid state passes
//! every check, and every deliberate corruption is caught.
//!
//! The invariants (mirroring the machine runtime's assumptions):
//!
//! 1. the bump cursor never exceeds the arena size;
//! 2. every live allocation is 16-aligned, positive, a multiple of 16, and
//!    lies entirely within the committed region (`start + size <= cursor`);
//! 3. no two live allocations share a start offset;
//! 4. every free-list offset is 16-aligned, within the committed region,
//!    and not the start of a live allocation (freed blocks are removed
//!    from the table before reuse);
//! 5. the free list contains no duplicate offsets and no cycles (it is
//!    bounded by the table size);
//! 6. no two live blocks overlap, and every free block lies inside the
//!    committed region and overlaps neither a live block nor another free
//!    block — together these make live and free blocks a partition of the
//!    allocated regions, so a split remainder can never be handed out
//!    twice;
//! 7. the free list is sorted by address and holds no two adjacent blocks
//!    (the coalescing invariant: a released block that touches another
//!    free block must have been merged with it, so no released capacity is
//!    left fragmented behind a neighbour).

use super::abi::{ALLOC_ALIGNMENT, HEAP_MAX_BYTES, MAX_LIVE_ALLOCS};
use super::allocator::Allocator;

/// A single invariant violation, rendered for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// A stable identifier of the violated invariant (e.g. `V-01`).
    pub code: &'static str,
    /// A human-readable description.
    pub message: String,
}

impl Violation {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Checks every runtime invariant over `heap`, returning the violations in
/// a stable order (empty when the state is valid).
pub fn check(heap: &Allocator) -> Vec<Violation> {
    let mut violations = Vec::new();

    // V-01: the cursor stays within the logical arena capacity.
    if heap.cursor() > HEAP_MAX_BYTES {
        violations.push(Violation::new(
            "V-01",
            format!(
                "bump cursor {} exceeds the arena capacity {HEAP_MAX_BYTES}",
                heap.cursor()
            ),
        ));
    }

    // V-02: every live allocation is aligned, positive, a multiple of the
    // alignment, and inside the committed region.
    for (slot, entry) in heap.table().iter().enumerate() {
        let Some(entry) = entry else { continue };
        if entry.start % ALLOC_ALIGNMENT != 0 {
            violations.push(Violation::new(
                "V-02",
                format!(
                    "live slot {slot}: start {} is not {ALLOC_ALIGNMENT}-aligned",
                    entry.start
                ),
            ));
        }
        if entry.size == 0 || entry.size % ALLOC_ALIGNMENT != 0 {
            violations.push(Violation::new(
                "V-02",
                format!(
                    "live slot {slot}: size {} is not a positive multiple of {ALLOC_ALIGNMENT}",
                    entry.size
                ),
            ));
        }
        if entry.start + entry.size > heap.cursor() {
            violations.push(Violation::new(
                "V-02",
                format!(
                    "live slot {slot}: block [{}, {}) extends beyond the cursor {}",
                    entry.start,
                    entry.start + entry.size,
                    heap.cursor()
                ),
            ));
        }
    }

    // V-03: no two live allocations share a start offset.
    let starts = heap
        .table()
        .iter()
        .flatten()
        .map(|entry| entry.start)
        .collect::<Vec<_>>();
    for (index, start) in starts.iter().enumerate() {
        if starts[..index].contains(start) {
            violations.push(Violation::new(
                "V-03",
                format!("live start {start} is recorded more than once"),
            ));
        }
    }

    // V-04/V-05: free-list entries are aligned, inside the committed
    // region, not live, and unique (a cycle would repeat an offset).
    let mut seen = std::collections::HashSet::new();
    for &(block, _size) in heap.free_list() {
        if block % ALLOC_ALIGNMENT != 0 {
            violations.push(Violation::new(
                "V-04",
                format!("free-list offset {block} is not {ALLOC_ALIGNMENT}-aligned"),
            ));
        }
        if block >= heap.cursor() {
            violations.push(Violation::new(
                "V-04",
                format!("free-list offset {block} is outside the committed region"),
            ));
        }
        if starts.contains(&block) {
            violations.push(Violation::new(
                "V-04",
                format!("free-list offset {block} is the start of a live allocation"),
            ));
        }
        if !seen.insert(block) {
            violations.push(Violation::new(
                "V-05",
                format!("free-list offset {block} appears more than once"),
            ));
        }
    }

    // V-06: the table size is exactly the ABI bound.
    if heap.table().len() != MAX_LIVE_ALLOCS {
        violations.push(Violation::new(
            "V-06",
            format!(
                "liveness table has {} slots, expected {MAX_LIVE_ALLOCS}",
                heap.table().len()
            ),
        ));
    }

    // V-07: live blocks do not overlap. Splitting makes this load-bearing: a
    // remainder that still covers the live block's bytes would hand the same
    // memory out twice.
    let live = heap
        .table()
        .iter()
        .flatten()
        .map(|entry| (entry.start, entry.start + entry.size))
        .collect::<Vec<_>>();
    for (index, &(start, end)) in live.iter().enumerate() {
        for &(other_start, other_end) in &live[index + 1..] {
            if start < other_end && other_start < end {
                violations.push(Violation::new(
                    "V-07",
                    format!("live block [{start}, {end}) overlaps live block [{other_start}, {other_end})"),
                ));
            }
        }
    }

    // V-08: free blocks lie inside the committed region and overlap no other
    // block, and their sizes are legal allocation units.
    for &(block, size) in heap.free_list() {
        if size == 0 || size % ALLOC_ALIGNMENT != 0 {
            violations.push(Violation::new(
                "V-08",
                format!("free block at {block} has an illegal size {size}"),
            ));
        }
        let end = block + size;
        if end > heap.cursor() {
            violations.push(Violation::new(
                "V-08",
                format!(
                    "free block [{block}, {end}) extends beyond the cursor {}",
                    heap.cursor()
                ),
            ));
        }
        for &(live_start, live_end) in &live {
            if block < live_end && live_start < end {
                violations.push(Violation::new(
                    "V-08",
                    format!(
                        "free block [{block}, {end}) overlaps live block \
                         [{live_start}, {live_end})"
                    ),
                ));
            }
        }
        for &(other, other_size) in heap.free_list() {
            if other == block {
                continue;
            }
            if block < other + other_size && other < end {
                violations.push(Violation::new(
                    "V-08",
                    format!(
                        "free block [{block}, {end}) overlaps free block [{other}, {})",
                        other + other_size
                    ),
                ));
            }
        }
    }

    // V-09: the free list is address-ordered and coalesced. Two adjacent
    // free blocks mean a freed block was never merged with its neighbour,
    // i.e. capacity that could serve a larger request is stranded.
    for pair in heap.free_list().windows(2) {
        let (start, size) = pair[0];
        let (next_start, _) = pair[1];
        if next_start < start {
            violations.push(Violation::new(
                "V-09",
                format!("free list is not address-ordered: {start} precedes {next_start}"),
            ));
        } else if start + size == next_start {
            violations.push(Violation::new(
                "V-09",
                format!("free blocks [{start}, {}) and [{next_start}, …) are adjacent and were not coalesced", start + size),
            ));
        }
    }

    violations
}

/// The verifier as a named type, for the public surface.
#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeVerifier;

impl RuntimeVerifier {
    /// Checks `heap`'s state, returning every [`Violation`] in stable
    /// order (empty when the state is valid).
    pub fn verify(&self, heap: &Allocator) -> Vec<Violation> {
        check(heap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_heap_is_valid() {
        let heap = Allocator::new();
        assert!(check(&heap).is_empty());
    }

    #[test]
    fn live_allocations_are_valid() {
        let mut heap = Allocator::new();
        let a = heap.alloc(16).unwrap();
        let b = heap.alloc(48).unwrap();
        assert!(check(&heap).is_empty());
        heap.free(a).unwrap();
        heap.store(b, 7).unwrap();
        assert!(check(&heap).is_empty());
    }

    #[test]
    fn duplicate_live_start_is_caught() {
        let mut heap = Allocator::new();
        let a = heap.alloc(16).unwrap();
        // Corrupt the state: record the same start twice.
        let second = heap.table().iter().position(|slot| slot.is_none()).unwrap();
        heap.corrupt_set_live(
            second,
            super::super::allocator::LiveEntry { start: a, size: 16 },
        );
        let violations = check(&heap);
        assert!(
            violations.iter().any(|v| v.code == "V-03"),
            "{violations:?}"
        );
    }

    #[test]
    fn free_list_pointing_at_a_live_block_is_caught() {
        let mut heap = Allocator::new();
        let a = heap.alloc(16).unwrap();
        heap.corrupt_push_free(a);
        let violations = check(&heap);
        assert!(
            violations.iter().any(|v| v.code == "V-04"),
            "{violations:?}"
        );
    }

    /// M7 Phase E: a long deterministic mixed-size sequence with far more live
    /// blocks than can stay coalesced. Every operation must leave a state that
    /// satisfies V-01…V-09, the free list must stay address-ordered with no two
    /// adjacent entries, and the arena must not grow beyond what the live set
    /// needs — the fragmentation/first-fit half of the Phase D contract.
    #[test]
    fn long_first_fit_sequence_keeps_every_invariant() {
        let mut heap = Allocator::new();
        let mut slots = [None::<(u64, u64)>; 64];
        let mut rng: u64 = 0x2545_F491_4F6C_DD1D;

        for op in 0..20_000u64 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let slot = (rng % 64) as usize;
            if let Some((block, _size)) = slots[slot].take() {
                heap.free(block)
                    .unwrap_or_else(|e| panic!("op {op}: valid free rejected: {e:?}"));
            } else {
                let size = 16 + (rng % 128) * 512;
                let block = heap
                    .alloc(size)
                    .unwrap_or_else(|e| panic!("op {op}: {size}-byte allocation rejected: {e:?}"));
                heap.store(block, op).expect("first word of a live block");
                heap.store(block + size - 16, op)
                    .expect("last word of a live block");
                assert_eq!(heap.load(block).unwrap(), op, "op {op}: first word");
                assert_eq!(
                    heap.load(block + size - 16).unwrap(),
                    op,
                    "op {op}: last word"
                );
                slots[slot] = Some((block, size));
            }
            let violations = check(&heap);
            assert!(
                violations.is_empty(),
                "op {op}: invariants violated: {violations:?}"
            );
            let free = heap.free_list();
            assert!(
                free.windows(2)
                    .all(|pair| pair[0].0 + pair[0].1 < pair[1].0),
                "op {op}: the free list must stay address-ordered and coalesced: {free:?}"
            );
            assert!(
                heap.cursor() <= 16 * 1024 * 1024,
                "op {op}: 64 slots of at most 64 KiB must not grow past 16 MiB; cursor={}",
                heap.cursor()
            );
            assert_eq!(
                heap.live_count(),
                slots.iter().filter(|slot| slot.is_some()).count(),
                "op {op}: live slots must match the live set"
            );
        }
    }
}
