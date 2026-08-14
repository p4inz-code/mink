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
//!    bounded by the table size).

use super::abi::{ALLOC_ALIGNMENT, HEAP_SIZE, MAX_LIVE_ALLOCS};
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

    // V-01: the cursor stays within the arena.
    if heap.cursor() > HEAP_SIZE {
        violations.push(Violation::new(
            "V-01",
            format!(
                "bump cursor {} exceeds the arena size {HEAP_SIZE}",
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
    for &block in heap.free_list() {
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
}
