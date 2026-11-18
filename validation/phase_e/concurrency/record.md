# M7 Phase E — allocator contention measurement (Windows x86_64)

Date: 2026-11-18. Release compiler built from this tree (`mink 1.0.5`,
unpublished). Linux untouched.

## What is measured

`main.mink` runs N worker threads, each in a fixed-size mixed allocation loop
against the shared heap until a 2 s deadline:

```
allocate 256..7,936 bytes -> write both ends -> re-read both ends -> free
```

Every `rt_alloc` and `rt_free` takes the runtime's single process-wide spin
lock, so the loop measures the lock's behaviour under contention. The clock is
read once per 64 iterations, so timer cost is not part of the number.

## Command

```
target/release/mink.exe build validation/phase_e/concurrency/main.mink
python validation/phase_e/concurrency/run.py
```

`run.py` runs 1, 2, 4 and 8 threads three times each and reports the median
alloc/free pairs per second (total across all threads), plus the per-thread
rate and the speedup over one thread. Any mismatch or failed join exits
non-zero.

## Result (this machine, 32 logical processors)

| Threads | Total alloc/free pairs/s | Per thread | Speedup |
|--------:|-------------------------:|-----------:|--------:|
| 1 | 28,227,680 | 28,227,680 | 1.00× |
| 2 | 6,303,392 | 3,151,696 | 0.22× |
| 4 | 2,837,248 | 709,312 | 0.10× |
| 8 | 1,370,144 | 171,268 | 0.05× |

A CPU-only control (same thread API, same 2 s burst loop, no allocator calls)
scales 1.98× / 3.93× / 7.55× at 2 / 4 / 8 threads, so the scheduler and the
thread runtime scale as expected; the collapse is contention on the heap lock.

## Interpretation

This is a **documented performance limitation**, not a correctness defect:

- Every run joined all threads with zero mismatches; the lock serializes the
  allocator correctly.
- The workload is the worst case: every thread allocates and frees as fast as
  it can, so the lock is the only work in the loop. Real programs spend most
  of their time outside the allocator. At a lower allocation rate the same
  runtime scales normally — `tests/runtime_boundaries.rs::pe08` (three
  allocations plus three frees per iteration, 4 threads × 20,000) completes
  four times the total work in 1.7× the single-thread wall time, and the
  60 s mixed soak does 28.6 M worker operations across four threads without a
  mismatch.
- No validated M2 workload depends on allocator scalability.

The measured curve is the evidence a future synchronization change would need.
A redesign (per-thread arenas, a backoff hint in the spin loop, or a
lock-free free list) is **out of scope for M7 Phase E**: the phase brief
forbids replacing the synchronization architecture without a separate,
justified design and verification effort, and the existing design is adequate
for the workloads MINK has validated.

## Status

Allocator concurrency correctness: **validated** within the documented
guarantees. Allocator concurrency scalability: **known limitation**, recorded
in `docs/runtime/MEMORY_MODEL.md` §23a.
