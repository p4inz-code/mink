# MINK: memory and resource model

**Status:** Planning / Specification
**Version:** 0.1.0

## 1. Objective

MINK must provide high performance and low-level capability while making memory and resource failures substantially harder to create.

The memory model must optimize for:

- Safety
- Performance
- Predictability
- Developer ergonomics
- Interoperability
- Long-term maintainability

## 2. Primary direction

MINK should use automatic memory management with deterministic resource-management capabilities rather than requiring ordinary developers to manually manage memory.

The exact implementation must be selected during compiler/runtime architecture based on performance, latency, predictability, safety, and implementation complexity.

## 3. Memory safety

Ordinary MINK code must prevent:

- Use-after-free
- Double-free
- Dangling references
- Invalid ownership transfer
- Buffer overflows where statically preventable
- Out-of-bounds access where statically preventable
- Uninitialized memory use where statically preventable

Undefined memory behavior should be aggressively minimized.

## 4. Developer experience

Developers should not normally need to manually allocate and release memory for ordinary application code.

The language and runtime should automatically handle ordinary object lifetime.

Advanced developers must still have mechanisms for controlling allocation, layout, lifetime, and interoperability when required.

## 5. Resource management

Memory is only one class of resource.

MINK must provide reliable management of:

- Files
- Sockets
- Database connections
- Threads
- Processes
- Locks
- OS handles
- GPU resources where applicable
- Native resources
- Other externally owned resources

Resource cleanup must remain deterministic where external resources require prompt release.

## 6. Deterministic cleanup

MINK should provide a language-level mechanism for deterministic cleanup of resources.

Cleanup must occur reliably when execution leaves the relevant ownership/lifetime scope, including through normal control-flow exits and error propagation.

The exact syntax and implementation are architecture decisions.

## 7. Ownership

MINK should provide a clear ownership model even if ordinary developers rarely need to reason about it explicitly.

The compiler/runtime should understand which component is responsible for the lifetime of a resource.

Ownership information should be available to diagnostics and tooling.

## 8. Borrowing and references

MINK may provide borrowing/reference semantics where they improve safety or performance.

Any borrowing system must remain understandable and must not make ordinary application development unnecessarily difficult.

The final model may use compiler-checked ownership, managed references, scoped borrows, or a hybrid strategy.

## 9. Garbage collection

If garbage collection is used, it must be treated as an implementation mechanism rather than the entirety of MINKs resource-management model.

The runtime must still provide deterministic handling for external resources.

Programs requiring predictable latency should have mechanisms to control or avoid unacceptable GC behavior.

## 10. Allocation

MINK should support multiple allocation strategies where technically justified.

Potential strategies include:

- General heap allocation
- Stack allocation
- Arena/region allocation
- Object pools
- Custom allocators
- Static allocation

The standard language experience should not require developers to understand these mechanisms for ordinary programs.

## 11. Stack and heap

The implementation should distinguish stack-friendly values from dynamically allocated values where beneficial.

The compiler may optimize allocation decisions when semantics remain unchanged.

Developers should not be forced to manually select stack versus heap allocation for ordinary values unless explicit control is required.

## 12. Value semantics

MINK should make value semantics predictable.

Small immutable values should be efficiently representable without unnecessary heap allocation.

Copying a value must have clearly defined semantics.

Large or resource-owning values should be movable or referenceable without unnecessary copying.

## 13. Move semantics

MINK should support efficient transfer of ownership without unnecessary deep copies.

Move operations should preserve safety and should not expose invalid references to moved resources.

The compiler should provide useful diagnostics when a value is used incorrectly after ownership transfer.

## 14. Shared ownership

MINK should support shared ownership where required by application architecture.

Shared ownership must be implemented using a clearly defined lifetime mechanism.

Reference cycles must be detectable, preventable, or explicitly manageable depending on the final memory strategy.

## 15. Weak references

Where shared ownership exists, weak references should be available to break ownership cycles and represent non-owning relationships.

Weak references must never silently become invalid ordinary references.

## 16. Concurrency interaction

The memory model must integrate with MINKs concurrency model.

Shared mutable state must have explicit safety semantics.

The language should prevent common data races where practical through compile-time guarantees or explicit synchronization mechanisms.

## 17. Unsafe operations

MINK may provide an explicit unsafe boundary for operations that cannot be proven safe by the compiler.

Unsafe code should be:

- Explicit
- Searchable
- Tooling-visible
- Diagnosable
- Restricted to clearly defined capabilities

Unsafe code must not silently contaminate ordinary code with undefined assumptions.

## 18. FFI and native memory

Foreign-function interfaces may expose manually managed memory and native resources.

The boundary between safe MINK and unsafe native memory must be explicit.

Tooling should identify ownership expectations at FFI boundaries.

## 19. Memory layout

MINK should eventually provide controlled mechanisms for specifying or querying memory layout when required by:

- Native interoperability
- Binary formats
- Hardware interfaces
- Serialization
- Performance-critical systems code

Layout guarantees must be explicit rather than accidental.

## 20. Resource leaks

MINK tooling should detect likely resource leaks where practical.

Diagnostics should identify:

- The resource that may leak
- Where it was acquired
- Where cleanup was expected
- The likely control-flow path responsible
- Recommended remediation

## 21. Out-of-Memory behavior

Out-of-memory conditions must have defined runtime behavior.

The runtime must not silently corrupt program state when allocation fails.

The exact recovery strategy will be defined by the runtime specification.

## 22. Performance

The memory model must support high-performance workloads without forcing all applications into low-level memory-management complexity.

Compiler optimizations may eliminate unnecessary allocations, copies, reference counting, or synchronization where correctness is preserved.

## 23. Diagnostics

MINK should provide specialized diagnostics for memory and resource problems.

Examples include:

- Possible lifetime violation
- Invalid ownership transfer
- Use after move
- Resource leak
- Invalid borrow
- Unsafe pointer operation
- FFI ownership mismatch
- Potential data race

Diagnostics should explain both the immediate error and the underlying ownership/lifetime relationship.

## 23a. Verified runtime limits (session 277)

These are the implemented, measured limits of the Windows x86_64 runtime, not
aspirations. Session 277 re-measured each one on the current tree; all remain
accurate.

| Limit | Measured behavior | Code / exit |
| --- | --- | --- |
| Live allocation slots | 1,024 live allocations are served; the 1,025th fails | `E-R03`, exit 103 |
| Single allocation | A request of 1 GiB (1,073,741,824 bytes) or more is refused | `E-R02`, exit 102 |
| Largest single block | 1,020 MiB (1,069,547,520 bytes) is served; 16 bytes more is refused | `E-R02`, exit 102 |
| Two half-arena blocks | Two 512 MiB blocks cannot coexist (see the prefix note below) | `E-R02`, exit 102 |
| Free-list reuse | 3,000 allocate/free cycles complete and reuse retired slots | exit 0 |
| Generation retirement | A block reused 65,535 times is retired, not reissued | exit 0, one new block per lifetime |
| Exit-time leak check | Unfreed live allocations at exit are reported | `E-R06`, exit 106 |
| Integer division by zero | Traps instead of faulting | `E-R14`, exit 114 |
| `i64::MIN / -1` and `i64::MIN % -1` | Traps instead of raising a hardware exception | `E-R15`, exit 115 |

Update, 2026-11-18 (Session 282, M7 Phase E) — the boundaries above were
re-measured on the Phase D allocator, and three statements need to be more
precise than the pre-Phase-E table:

* **The usable arena is 1,020 MiB, not 1,024 MiB.** The first 4 MiB of the
  offset space live in the image's `.bss` and the rest in a separate
  `VirtualAlloc` region; the two mappings are not address-contiguous, so a
  block may not span the boundary. A bump that would straddle it is moved to
  the start of the growth region, leaving the prefix tail unused. Two
  consequences, both deterministic `E-R02`: the largest single block is
  1,020 MiB, and two 512 MiB blocks do not fit. The cost is 0.39% of the
  logical ceiling and no allocation is ever mis-sized or overlapped
  (`tests/runtime_boundaries.rs` pe03–pe05).
* **The free list is not a plain bump list.** Since Phase D it is
  address-ordered first fit that splits a reused block at exactly the
  requested size and coalesces a freed block with its address neighbours, so
  released capacity is always reusable (`tests/allocator_conformance.rs`).
* **Generation retirement is a growth source.** A block reused 65,535 times
  is retired instead of reissued, because its 16-bit handle generation must
  never wrap. Under sustained churn that costs one block per 65,535 reuses —
  bounded and deterministic, never corruption (pe01/pe02).

Concurrency boundary: the heap has one process-wide spin lock and every
allocation, free and validated access takes it, so allocation scales
negatively under tight multi-threaded contention. Measured on this machine
with `validation/phase_e/concurrency` (2 s window, 256–7,936 byte blocks,
median of three runs): 28.2 M alloc/free pairs/s with one thread, 6.3 M/s
aggregate with two, 2.8 M/s with four and 1.4 M/s with eight — 0.22×, 0.10×
and 0.05× of the single-thread total. A CPU-only control on the same machine
scales 1.98×/3.93×/7.55× at 2/4/8 threads, so the collapse is the lock, not the
scheduler. Workloads that allocate at a lower rate relative to their own work
scale normally (a four-thread mixed-size loop with three allocations per
iteration completed four times the total work in 1.7× the wall time), which is
why the seven M2 workloads are unaffected. This is a documented performance
limitation, not a correctness one: the lock is correct, and no validated
workload depends on allocator scalability.

Model boundary: the pure-Rust reference allocator (`src/runtime/allocator.rs`)
abstracts two machine details — it has no `.bss`/growth straddle rule and no
generation retirement. Unit tests that assert those behaviours use the
compiled runtime, not the model.

The exit-time leak check means a program that returns 0 has a balanced
allocator: no heap allocation leaked. Every standard-library suite that runs a
compiled program therefore asserts exit 0, which is what makes the limits above
observable rather than merely documented.

## 24. Compatibility

The memory model is part of MINKs long-term language contract.

Changes to observable lifetime, ownership, copying, or resource semantics must be treated as compatibility-sensitive changes.

## 25. Open architecture decisions

The following must be resolved before architecture freeze:

- Primary automatic memory-management strategy
- Whether ownership is compiler-enforced
- Exact borrowing model
- Garbage-collection strategy, if any
- Reference-counting strategy, if any
- Deterministic cleanup syntax
- Move semantics details
- Shared ownership implementation
- Memory-layout guarantees
- Allocator API
- Unsafe-memory model
- Concurrency/memory-model interaction
