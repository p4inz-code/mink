# Session 282 — M7 Phase E: runtime boundaries, failure paths and engineering closeout

Date: 2026-11-18. Platform: Windows x86_64 (Linux frozen, untouched). Version
stays `1.0.5`, unpublished; no tag, no release, no credential change. M6 remains
outstanding and separate; M8 remains gated.

Baseline: `7b9a56b`, local `main` equal to `origin/main`, four untracked user
files in the worktree (`MINK UI SETS OFFICIAL.zip`, `nl1.exe`, `nl1.mink`, a
pasted chat log) left untouched and outside the evidence.

## Verdict

**M7 Phase E: PASS.** All eight documented runtime boundaries were investigated
against executed code; no unresolved correctness or safety defect was found.
The allocator's lifecycle limits (generation retirement, the 1 GiB ceiling, the
1,024-live-allocation table, fragmentation) are deterministic and recoverable
by freeing, concurrency is correct within the documented guarantee, and both M2
capability limitations are classified and documented. Two measurement/documentation
defects were found and fixed: a Phase D soak assertion that was one clock tick
too strict, and several user-facing comments that still described the
pre-Phase-D heap. One measured limitation is newly quantified: allocation
throughput degrades under heavy multi-thread contention (0.05× the
single-thread total at eight threads). It is a performance limit of the
documented single-lock design, not a defect, and no validated workload depends
on allocator scalability.

## 1. Gate A — repository and evidence reconciliation

Inspected: the roadmap and its M7 A–D status entries; the M7 phase reports
(Sessions 277–281); the M1 report and capability matrix; the M2 report, audit
and per-workload records; the allocator reference model, machine emitter,
verifier and the Phase D corpus; the release-acceptance runner; the M6 external
package. `git status` shows only the four pre-existing untracked user files,
`HEAD == origin/main == 7b9a56b`.

The M2 PARTIAL verdict and the Phase D evidence reconcile cleanly: the only
open M2 defect (M2-D1, mixed-size churn exhausting the arena) was closed in
Phase D, and Phase E re-ran every M2 workload against the current tree rather
than relying on the earlier run.

### Consolidated engineering status matrix

Classification follows the Phase E brief. "Defect?" means: is there a
correctness or safety defect that must be fixed for engineering readiness?

| # | Documented boundary | What Phase E measured | Classification | Defect? |
|---|---------------------|-----------------------|----------------|---------|
| 1 | Generation retirement after 65,536 block reuses | One 64-byte block serves 65,535 allocate/free cycles, then is retired and never reissued; 400,000 cycles consume exactly 7 blocks, high-water 400 B, values intact; a stale collection handle is `E-R05` across two retirement boundaries | Intentional design constraint (keeps the 16-bit handle generation from wrapping) | No |
| 2 | O(n) address-ordered free-list walks | The list stays short because adjacent blocks coalesce; a 64-hole fragmented list is walked without error; the 60 s soak does 31.1 M mixed operations with a 3.9 MiB high-water mark | Performance limitation | No |
| 3 | First-fit fragmentation | 64 non-adjacent 4 KiB holes serve 64 subsequent 4 KiB blocks; a 256 KiB request correctly bumps; after release the coalesced region serves it from the freed area | Performance limitation (best fit would leave different remainders; no capacity failure observed) | No |
| 4 | 1 GiB arena ceiling | Largest single block is 1,020 MiB; 16 bytes more is `E-R02`; two 512 MiB blocks cannot coexist; the block is reused at the same address and splits at its front | Capacity limitation (0.39% of the logical ceiling is unavailable at the `.bss`/growth boundary) | No |
| 5 | 1,024-live-allocation ceiling | 1,024 live blocks are served, 511 freed slots are reused by 511 new blocks, then the 1,025th is `E-R03` | Capacity limitation | No |
| 6 | Single spin lock / concurrency | Correct under 4-thread mixed-size stress and the 60 s soak; throughput collapses under maximal fine-grained contention (see §3) | Performance limitation (documented serialization) | No |
| 7 | Child stdin capped at 4,096 of 5,000 bytes | 4,095/4,096 arrive whole; 4,097/5,000/65,536 arrive as 4,096; the child sees EOF and exits 0; write loop handles short writes and cannot deadlock at the cap | Intentional design constraint (one pipe buffer) | No |
| 8 | SQLite `COUNT` available, `SUM` unavailable | `SELECT SUM(...)` and `GROUP_CONCAT` return a structured error; the database is unchanged and the next `COUNT(*)` is correct | Missing capability (deliberate SQL subset) | No |

No entry in this matrix is a genuine defect. The two entries that were
previously described as open (1 and 4 in the M2/M1 registers) are intentional
or capacity limits, not correctness problems.

## 2. Gate B — allocator lifecycle boundaries

New permanent corpus: [`tests/runtime_boundaries.rs`](../../tests/runtime_boundaries.rs),
eight executed cases compiled by the real CLI and run as native PEs. A clean
exit also runs the leak check.

| Case | Establishes |
|------|-------------|
| pe01 | Generation-retirement cadence is exact and monotonic: 400,000 churn cycles → 7 distinct blocks, high-water 400 B, 0 mismatches, contents sum correct |
| pe02 | A stale collection handle is `E-R05`/105 after **two** retirement boundaries — retired blocks are never reissued |
| pe03 | A 1,020 MiB block round-trips at both ends, is reused at the same address, keeps retained contents, and a 16-byte allocation splits off its front |
| pe04 | One 16-byte allocation after the maximum block is `E-R02`/102 (deterministic exhaustion, no wrap, no overlap) |
| pe05 | Two 512 MiB blocks cannot coexist: `E-R02`/102. Pins the `.bss`/growth straddle cost |
| pe06 | 1,024 live blocks, 511 freed and 511 replaced by slot reuse with values intact, then the 1,025th is `E-R03`/103 |
| pe07 | Fragmented first fit: 64 non-adjacent holes reused, live neighbours untouched, a 256 KiB request bumps, and the coalesced region serves it after release |
| pe08 | Four threads × 20,000 mixed-size iterations, 80,000 completions, 0 mismatches, all joins, clean exit |

Also added to the verifier's own tests:
`runtime::verify::tests::long_first_fit_sequence_keeps_every_invariant` drives
20,000 deterministic mixed-size operations over 64 slots and checks V-01…V-09
after **every** operation, plus address order, the no-two-adjacent rule and a
16 MiB growth bound. Result: 5 verifier tests pass.

### The `.bss`/growth boundary (newly documented)

A block may not span the 4 MiB `.bss` prefix and the separate `VirtualAlloc`
growth region, so a bump that would straddle the boundary is moved to the start
of the growth region and the prefix tail is left unused. Consequences, both
measured and deterministic:

* the largest single allocation is 1,020 MiB, not 1 GiB (`ceiling2`: succeeds;
  1,020 MiB + 16: `E-R02`);
* two 512 MiB blocks do not fit, although each is half the logical ceiling
  (`ceiling3`: `E-R02` on the second).

No allocation is ever mis-sized or overlapped, and the cost is 0.39% of the
logical ceiling. Recorded as a capacity limitation in
[`docs/runtime/MEMORY_MODEL.md`](../runtime/MEMORY_MODEL.md) §23a.

### Reference-model fidelity (technical debt, not runtime behaviour)

The pure-Rust reference allocator abstracts two machine details: it has no
`.bss`/growth straddle rule (it can allocate offsets the machine cannot) and no
generation retirement (a model churn never retires a block). Its own
`out_of_memory_is_reported` unit test therefore assumes two 512 MiB blocks fit,
which the machine refuses. This is a specification-model gap, not runtime
incorrectness, and every test that asserts the real boundary uses a compiled
program. Recorded as technical debt and added to MEMORY_MODEL §23a.

## 3. Gate C — concurrency and synchronization

Design inspected: one `BSS.rt_lock` word, acquired with `xchg` in a tight loop
and released with a plain store; every `rt_alloc`, `rt_free`, `rt_mem_load`/
`rt_mem_store` and handle validation takes it. No recursive acquisition, no
second lock.

Correctness evidence: `pe08` (4 threads × 20,000 mixed-size iterations, 80,000
completions, 0 mismatches), the 60 s soak (§5) and the M2 `threads` workload
(12 × 5,000 locked increments = 60,000 exactly; 100 spawn/join cycles lose no
work; return codes propagate). No deadlock, no corrupted free-list state, no
lost join.

Throughput, measured with the committed harness
[`validation/phase_e/concurrency`](../../validation/phase_e/concurrency/record.md)
(2 s window, 256–7,936 byte blocks, median of three runs, 32 logical
processors):

| Threads | Total alloc/free pairs/s | Per thread | Speedup vs 1 |
|--------:|-------------------------:|-----------:|-------------:|
| 1 | 28,227,680 | 28,227,680 | 1.00× |
| 2 | 6,303,392 | 3,151,696 | 0.22× |
| 4 | 2,837,248 | 709,312 | 0.10× |
| 8 | 1,370,144 | 171,268 | 0.05× |

A CPU-only control on the same machine scales 1.98×/3.93×/7.55× at 2/4/8
threads, so the scheduler scales; the allocator is the bottleneck. Two
observations keep this in proportion: the workload allocates in a tight loop
with no other work, and at a lower allocation rate the runtime does scale — a
four-thread loop with three allocations per iteration completed four times the
total work in 1.7× the single-thread wall time (measured during the audit), and
the soak does 28.6 M worker operations across four threads without a mismatch.

**Decision: no synchronization change in Phase E.** The brief forbids replacing
the architecture without a separate justified effort, the lock is correct, and
no validated workload depends on allocator scalability. The measured curve is
the evidence a future backoff, per-thread-arena or lock-free design would have
to beat.

## 4. Gate D — remaining practical capability limitations

### Child-process stdin (M2 L1)

Measured with [`validation/phase_e/stdin`](../../validation/phase_e/stdin/record.md):
0/1/4,095/4,096 bytes arrive whole; 4,097, 5,000 and 65,536 bytes arrive as
4,096; the child sees EOF and exits 0; the parent's return value is the child's
exit code. The write loop advances by the bytes `WriteFile` reports and stops at
the cap, so a short write is handled; the cap is exactly one pipe buffer, which
is why it cannot deadlock (`tests/process_lib.rs` pins the non-deadlock property
with an 8 KiB offer to a child that never reads stdin).

**Classification: intentional design constraint**, not a defect. The only
weakness is that truncation is silent; `stdlib/process.mink` now says so and
names the supported route for larger input (a file redirect, or several
children). A write-all API is not added: doing it safely needs a concurrent
drain (feeder thread or overlapped I/O) and its own lifecycle/failure tests —
a new concurrency surface, not a correction — and no validated workload needs
more than one pipe buffer.

### SQLite aggregates (M2 L4)

The SQLite-shaped engine is a MINK standard-library module, not a binding to
sqlite3, so `SUM` is not "unsupported by SQLite" — it is outside the engine's
documented subset (`SELECT *`, `SELECT COUNT(*)`, with `WHERE`; no `JOIN`,
`GROUP BY`, `ORDER BY`). Phase E measured the rejection: `SELECT SUM(id)` and
`SELECT GROUP_CONCAT(name)` return
`ERROR: only SELECT * / SELECT COUNT(*) is supported`, leave the database
unchanged, and the following `COUNT(*)` is correct — now pinned by
`sqlite_lib::s41_sum_is_rejected_as_a_documented_subset`.

**Classification: missing capability.** Not implemented: no validated workload
needs it (the M2 integrated app sums in MINK), and adding `SUM` properly means
deciding integer/float/`NULL`/`STR` semantics and the aggregate return shape —
a deliberate engine change, not a small safe library addition. The O(N²)
per-statement copy remains the engine's real practical limit; the module comment
now records the current heap (1 GiB, coalescing) and the measured 4.1 s for 500
row inserts, replacing a stale "1 MiB arena / ~220 rows" claim.

## 5. Gate E — regression and practical revalidation

All commands run from the repository root on this machine. Every result below
is from an actual run; nothing was skipped or weakened.

| Check | Command | Result |
|-------|---------|--------|
| Debug regression | `cargo test --all-targets --no-fail-fast` | **100 binaries, 3,640 passed, 0 failed, 10 ignored**, exit 0 |
| Release regression | `cargo test --release --all-targets --no-fail-fast` | **100 binaries, 3,640 passed, 0 failed, 10 ignored**, exit 0 |
| Allocator corpus | `cargo test --test allocator_conformance` | 20 passed, 0 failed, 1 ignored |
| Extended soak | `cargo test --release --test allocator_conformance -- --ignored --nocapture` | **2,517,059 main + 28,626,749 worker operations in 60.00 s, 0 mismatches, high-water 4,091,920 B**, all 4 threads joined |
| Capacity corpus | `cargo test --test capacity` | 9 passed |
| `tests/zlib_lib.rs` | `cargo test --test zlib_lib` | 15 passed; `git diff -- tests/zlib_lib.rs` is **empty** (unchanged) |
| Interior-pointer validation | `ac11` (allocator corpus) + `pe07` | interior words round-trip; one word past a live block is `E-R05`/105; neighbour blocks are untouched |
| Runtime verifier | `cargo test --lib runtime::` | 25 passed, including the 5 verifier tests and the new 20,000-op invariant sequence |
| M2 big data/files/stdin/JSON | `python validation/m2_bigio/verify.py` | **25/25** |
| M2 process automation | `python validation/m2_procauto/verify.py` | **8/8** |
| M2 networking under load | `python validation/m2_net/verify.py` | **11/11** |
| M2 HTTP server | `python validation/m2_server/verify.py` | **PASS** (120 requests over 16 workers + 40 over 4) |
| M2 threads | `main.exe contend 12 5000` / `cycles 100 7` / `failure 8` | `threads=12 counter=60000 returned=60000`; `counter=700`; `joined=8 seven=8`; exit 0 |
| M2 soak | `main.exe 10 soak_work` | 10.008 s, **1,595,081 loops, 159,510 thread lifecycles**, exit 0 |
| M2 integrated | `python validation/m2_integrated/verify.py` | **PASS** (100 rows, 100 records, totals match) |
| Mixed-size churn reproduction | `churn_repro.exe 1000000` | `ok`, exit 0, **0.038 s** |
| Child-stdin boundary | `python validation/phase_e/stdin/verify.py` | **PASS** (table in §4) |
| Concurrent alloc/free stress | `pe08` + soak above | 80,000 concurrent iterations, 0 mismatches |
| Release acceptance | `python tools/release_acceptance.py` | see §8 (run on the committed tree) |

### Defects found and fixed in Phase E

1. **Phase D soak assertion was one clock tick too strict** (test defect, not a
   runtime defect). `ac16_mixed_size_soak_60s` required `elapsed >= 60.0`, but
   the program's deadline is `time_millis() + 60_000` and the tick is truncated
   to whole milliseconds, so a full soak can finish at 59.99 s. The assertion is
   now `>= 59.5` with a comment; the workload and its 60 s window are
   unchanged. Measured after the fix: 60.00 s, both counters non-zero, 0
   mismatches, high-water below the 256 MiB bound.
2. **Stale allocator documentation** (documentation defect). `stdlib/sqlite.mink`,
   `stdlib/zlib.mink`, `stdlib/zip.mink` and `stdlib/tls.mink` still described a
   fixed 4 MiB (or 1 MiB) bump heap with no reuse;
   `docs/implementation/RUNTIME_IMPLEMENTATION.md` §9 still said "fixed 1 MiB"
   and single-threaded. All now describe the current 1 GiB, coalescing,
   threaded runtime (with dated update notes where the text is historical).
   The npm bundle was re-synced; `tests/release.rs` enforces byte equality.

No runtime correctness or safety defect was found. No test was weakened: the
one assertion changed concerns wall-clock rounding, and the 60 s workload is
unchanged.

### Environment note (not a source defect)

Release-mode `rustc` crashed intermittently during this session with
`STATUS_ACCESS_VIOLATION` (0xC0000005) or `STATUS_ILLEGAL_INSTRUCTION`
(0xC000001D). Retrying the identical command succeeded, no source change was
involved, and the same failure mode is already recorded in the project history
(Sessions 205–210 and 212 as a transient host fault). Every build/suite result
reported here is from a completed run; the retries are not counted as evidence.

## 6. Current runtime architecture (as audited)

- **Arena:** 1 GiB logical; first 4 MiB in `.bss`, the rest `VirtualAlloc`-reserved
  and committed in 1 MiB steps; usable 1,020 MiB; no block spans the two mappings.
- **Allocation:** 16-byte alignment, address-ordered first fit, exact split of a
  reused block, bump when nothing fits; `E-R02` above 1 GiB or on commit failure.
- **Free:** exact-start, 16-aligned live block only; insert in address order and
  coalesce with exact-address neighbours; `E-R04`/`E-R07` otherwise.
- **Liveness table:** 1,024 slots × 24 B; slot size is the request, never the
  reused block's capacity — that is the interior-access boundary. `E-R03` when
  full. 16-bit generation per block, advancing per reuse; retirement instead of
  wrap.
- **Validation:** `rt_mem_load`/`rt_mem_store` require an 8-byte aligned word
  wholly inside one live block (`E-R05`); strings validate their pointer
  (image range or live block) and bounds-check every byte index.
- **Synchronization:** one spin lock around allocation, free, validated access
  and handle checks.
- **Exit:** a live allocation at exit is `E-R06`.

## 7. Remaining limitations, technical debt and risks

Accepted limitations (not defects):

1. Generation retirement costs one block per 65,535 reuses (bounded,
   deterministic).
2. Free-list operations are O(n); coalescing keeps n small in practice.
3. First fit, not best fit.
4. 1,020 MiB usable arena / 1,024 live allocations; `E-R02`/`E-R03`.
5. Single spin lock: allocation throughput degrades under contention (0.05× at
   8 threads in the worst-case loop).
6. Child stdin capped at 4,096 bytes, silently.
7. SQL subset: no `SUM`/`GROUP BY`/`JOIN`/`ORDER BY`/on-disk database; O(N²)
   per-statement copy.
8. 8 MiB stdio capture windows; one HTTP connection at a time.

Technical debt / known inaccuracies:

1. The reference allocator does not model the `.bss`/growth boundary or
   generation retirement (see §2).
2. `cargo fmt --check` is still dirty for 15 pre-existing files; every file
   touched by this session is rustfmt-clean (0 remaining diffs in touched
   files). No unrelated file was reformatted.
3. The 116 pre-existing Clippy warnings are unchanged; no warning was added or
   suppressed.

Outstanding risks:

1. Only Windows x86_64 is supported; Linux is frozen and unvalidated.
2. The runtime's allocator is a single-lock bounded arena — fine for the
   validated workloads, not a scalability claim.
3. M6 external validation has not happened; the engineering evidence here is
   internal, by the original developer.
4. The intermittently crashing release `rustc` (environment) can waste a build;
   it does not affect artifacts that complete.

## 8. Release acceptance, formatting and Clippy

- Release acceptance on the committed tree (`316f7eb`):
  `python tools/release_acceptance.py --session 282` → **verdict PASS**, runner
  exit 0, report
  `target/acceptance/282/20261118T172415Z-316f7eb/report.md`. Build 19.7 s,
  test compilation 12.5 s, regression 63.9 s; artifact `mink 1.0.5`,
  4,337,664 bytes, SHA-256
  `ac20e837980a0e967fd16301f80e710190cede32576dfce93e3c1c93918500c4`;
  regression **101 suites, 3,640 passed, 0 failed, 10 ignored**. One
  limitation, unchanged from Phase D: the four pre-existing untracked user
  files, which are deliberately outside the acceptance evidence.
- The runner does not retry, and several acceptance attempts aborted before the
  build stage completed in the host-level `rustc` fault described in §5 (exit
  101, no test stage run; their report directories remain under
  `target/acceptance/282/`, which is git-ignored). The recorded run is the
  first attempt whose build completed. No source, configuration or command
  changed between attempts — the retry is the same command twice.
- `cargo fmt --check`: 15 pre-existing files are not rustfmt-clean; the touched
  files (`tests/runtime_boundaries.rs`, `src/runtime/verify.rs`,
  `tests/allocator_conformance.rs`, `tests/sqlite_lib.rs`) are clean.
- `cargo clippy --all-targets`: **0 errors, 116 warnings** — exactly the Phase D
  count.

## 9. What was *not* done

No version bump, no npm publication, no credential change, no tag, no GitHub
release, no Linux work, no allocator or synchronization redesign, no new
language or library feature. M6 remains outstanding and M8 remains gated.

## 10. Recommendation

**C — MINK has crossed the practical Windows-use threshold; begin M6 external
developer validation.**

M7's engineering scope is complete (option A also holds): every documented
runtime boundary is measured, the allocator's lifecycle and capacity failures
are deterministic and recoverable, concurrency is correct within the documented
single-lock guarantee, and the M2 workloads all pass. The remaining items are
accepted limitations or missing capabilities, not blockers. The only milestone
that can add new information now is M6: a developer who did not build MINK
attempting realistic tasks. M8 stays gated until that evidence exists.

## 11. Roadmap position

M7 Phase E: **PASS**. M2: re-scored **PASS** with accepted limitations. M6:
**OUTSTANDING** (external, not started). M8: **gated**. Linux: **frozen**.
1.0.5: **unpublished**.

## 12. Commits and remote verification

Staged file by file (never `git add .` or `-A`), five content commits on top of
`7b9a56b`, followed by this commit-set record:

| Commit | Content |
|---|---|
| `b5b814e` | `test(runtime)` — Phase E boundary corpus, verifier invariant sequence, SQL-subset pin, soak assertion bound |
| `6d5f556` | `docs(runtime)` — measured heap boundaries in four stdlib modules, `RUNTIME_IMPLEMENTATION`, `MEMORY_MODEL`; npm bundle re-sync |
| `2ecdee7` | `test(validation)` — reproducible stdin-boundary and contention harnesses |
| `316f7eb` | `docs(phase-e)` — Phase E report, M2 re-score, workload records, roadmap closeout |
| `10db5f2` | `docs(phase-e)` — the release-acceptance PASS recorded |

Two documentation commits follow the table (`docs(phase-e): pin the session
commit set` and `docs(phase-e): correct the commit-set wording`). The four
untracked user files were left untouched and are not part of the evidence. The
branch was pushed to `origin/main`; local `HEAD` equals `origin/main` after the
push (verified with `git rev-parse HEAD origin/main`).
