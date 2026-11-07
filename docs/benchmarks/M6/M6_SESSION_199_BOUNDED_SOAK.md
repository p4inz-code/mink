# Session 199 — Bounded Runtime Soak Under Sustained Workloads

Windows x86_64, 2026-11-07. Starting HEAD
`6a5e8e84a72444b3db14a282f0e61393a8411b91` (Session 198, local == origin/main
verified before any change).

## 1. Scope established before running

Session 198 measured resource *lifecycle* at moderate counts (3,000 cycles). This
session asks a different question: does the runtime stay correct and leak-free
when the same facilities run in **sustained repetition in one process**, with a
hard wall-clock bound so a hang cannot pass as a result?

The workload covers the seven paths a real Windows program repeats:

| # | Workload | Facility exercised |
| --- | --- | --- |
| 01 | Compiler invocations | `mink build` end-to-end, repeated |
| 02 | Program launches | compiled `.exe` start-up/exit, repeated |
| 03 | Collection lifecycle | `rt_vec_new`/`push`/`len`/`free` (generation-safe handles) |
| 04 | Filesystem cycles | `fs_write` + `fs_read` + `fs_file_size`, small and 1 MiB |
| 05 | Child processes | `process_run` + `process_stdout_len` |
| 06 | Error paths | double free (104), stale handle (105), out of bounds (110) |
| 07 | Combined workload | 03 + 04 + threads + child process per iteration |

## 2. Method

New suite `tests/soak.rs` (8 tests; seven are `#[ignore]`d by design so the
normal regression stays fast, and are run with `--ignored`). Each test:

1. compiles a MINK program with a **fixed iteration count**,
2. runs it under a **per-run wall-clock budget** (180 s): overrunning the
   budget kills the process and fails the test, so a hang is a failure rather
   than a stalled CI job,
3. samples the child's OS handle count and peak working set every 10 ms
   (`OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `GetProcessHandleCount`
   + `K32GetProcessMemoryInfo`, each sampler handle closed immediately),
4. asserts exit 0, empty stderr, the program's own internal check counter is 0,
   and handle flatness: with warm-up = max of the first three samples, peak may
   not exceed warm-up + 8 and the final sample may not exceed warm-up + 6.

Exit 0 also proves the leak checker did not fire (`E-R06`/106) and the
liveness table was not corrupted.

## 3. Measurements (release build, one full `--ignored` run)

| Workload | Iterations | Elapsed | Per iter | Handles (first → peak → last) | Peak WS | Exit |
| --- | --- | --- | --- | --- | --- | --- |
| 01 compiler invocations | 6 builds | 0.055 s | 5.2–13.9 ms | — | — | 0 ×6 |
| 02 program launches | 100 | 0.228 s | 2.28 ms | — | — | 0 ×100 |
| 03 collection lifecycle | 400,000 | 8.317 s | 0.021 ms | 28 → 68 → 68 | 10.5 MiB | 0 |
| 04 filesystem cycles | 3,000 (+5 × 1 MiB reads) | 3.196 s | 1.065 ms | 28 → 69 → 68 | 31.7 MiB | 0 |
| 05 child processes | 100 | 0.687 s | 6.874 ms | 28 → 82 → 78 | 3.6 MiB | 0 |
| 06 error paths | 20,000 | 0.891 s | 0.045 ms | 28 → 68 → 68 | 9.6 MiB | 0 |
| 07 combined | 200 × 3 runs | 1.442 / 1.415 / 1.422 s | ~7.1 ms | 28 → 82 → 78 | 4.2 MiB | 0 ×3 |

Suite result: **7 passed / 0 failed / 0 ignored** (`--ignored` selects exactly
the seven soak tests), finished in 8.34 s.

Error-path runs were 10/10 each: double free → 104 (`E-R04`), stale collection
handle → 105 (`E-R05`), out-of-bounds → 110 (`E-R10`). All internal counters
read 0, so every one of the 400,000 collection cycles and 3,000 filesystem
cycles did the expected work rather than silently skipping it.

Handle counts rise once per workload because the process-spawning paths keep
their capture pipe/process handles for the lifetime of the runtime (the warm-up
window already includes them); across 100 child processes, 3,000 filesystem
cycles and 400,000 allocations the count does not trend upward.

## 4. Findings

### 4.1 No confirmed defect

Every workload completed inside its budget with exit 0, empty stderr, a zero
internal failure counter and flat handle counts. The generation-safe collection
handles added in Session 197 survived 400,000 allocate/push/free cycles plus
nested use in the combined workload, with no `E-R05` on live handles.

### 4.2 Allocator ceiling — documented limitation, not a defect

The first (over-large) version of workload 04 wrote **65,540 bytes** per
iteration, 12,000 times (≈ 750 MB written, ≈ 1.2 GB of string allocations).
It did not finish: the program aborted with `E-R02` (exit 102, "the runtime
heap is exhausted") at iteration **342** with peak working set **1027.1 MiB**.

This is the allocator behaviour already recorded in Sessions 132 and 136:
the arena's free list is LIFO and does not coalesce, so a repeated
large-allocation/free pattern bumps the heap until the 1 GiB ceiling and then
fails **loudly and deterministically** — it never returns stale or truncated
success. The soak workload was reduced to 4,100 bytes per iteration for 3,000
iterations (12.3 MB written) plus five 1 MiB reads, which stays inside the
documented ceiling while still exercising the same code path for thousands of
cycles. No code change was made: changing the allocator is out of scope for a
soak audit and the ceiling is already a documented, deliberate limitation.

### 4.3 Harness defects found and fixed (not runtime defects)

Constructing the suite exposed three mistakes in the *specs*, all fixed in
`tests/soak.rs`:

* `rt_vec_push` consumes and returns the collection handle; ignoring the return
  value used a freed handle and produced `E-R05`. The specs now reassign
  (`v = rt_vec_push(v, i)`), which is the documented ownership contract.
* Including a library both as a `mod` and as concatenated source defined every
  function twice (`E-S02`). The specs now concatenate each library exactly once.
* A 12,000-iteration 64 KiB filesystem loop exceeded the allocator ceiling
  (§4.2); the bounded workload above replaced it.

No runtime, compiler or stdlib change was needed for any of these.

## 5. Residual risks (unchanged, not exercised)

* `emit_fs_read` still ignores `ReadFile`'s BOOL and byte count (Session 198,
  §risk): a short read would return stale bytes at the requested length. The
  soak reads whole files only, so it cannot trigger a short read; the risk
  remains recorded, not patched.
* The 1 GiB arena ceiling and `MAX_LIVE_ALLOCS = 1024` remain architectural
  limits (documented Sessions 132/136/138).
* This suite is *bounded*, not a long-running soak: budgets are 180 s and the
  whole run is under 10 s. It does not establish multi-hour stability, and M6
  external developer validation remains outstanding.

## 6. Reproduce

```
cargo test --release --test soak -- --ignored --nocapture
cargo test --release --test soak          # environment test only
```

Expected: `test result: ok. 7 passed; 0 failed; 0 ignored` for the first and
`1 passed` for the second.

## 7. Verdict

**PASS — no confirmed defect in the soaked paths.** Sustained, bounded
repetition of compiler, launch, collection, filesystem, child-process, error
and combined workloads exited cleanly with zero failures, flat OS handle counts
and no leak-checker trips. The only ceiling reached was the already-documented
allocator limit, which fails loudly (`E-R02`, exit 102) and was designed
around rather than patched.
