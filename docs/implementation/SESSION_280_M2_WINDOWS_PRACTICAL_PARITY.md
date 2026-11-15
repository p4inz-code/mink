# Session 280 — Milestone 2: Windows Practical-Parity Validation

Date: 2026-11-15. Platform: Windows x86_64 (Linux frozen). Version stays 1.0.5,
unpublished; no tag, no release, no credential change, M6 still outstanding, M8
still gated.

## Why this session exists

Milestone 1 validated features. This session validated work: seven applications
that resemble real Windows programs, each executed and compared against an
independent reference. The brief for M2 is explicit that compiling is not
evidence, that every failure must be classified, and that no missing capability
may be disguised as a defect (or the reverse).

## What was built

| Directory | Workload |
|-----------|----------|
| `validation/m2_bigio` | 5 MiB file generation/scan/copy, 1 MiB stdin, 12,000-record JSON, 20,000-cycle string churn, error paths |
| `validation/m2_procauto` | child capture (64 KiB out, stderr, exit 7), 4,096-byte stdin, 50 runs, exit codes, failed launch |
| `validation/m2_net` | 1 MiB TCP echo, 200 round trips, 8 KiB UDP, HTTP GET, refusal, bounded connect timeout |
| `validation/m2_server` | MINK HTTP server driven by 16 concurrent Python clients |
| `validation/m2_threads` | locked counter under 12 threads, 100 spawn/join cycles, return-code propagation |
| `validation/m2_soak` | 10 s continuous mixed work in one process |
| `validation/m2_integrated` | SQL rows + JSON file round trip + threaded arithmetic in one program |

Each directory carries `record.md`; the full analysis and the capability matrix
are in [`docs/validation/VALIDATION_MILESTONE_2_REPORT.md`](../validation/VALIDATION_MILESTONE_2_REPORT.md).

## Defects

**Fixed — string access validated in O(liveness table) per byte.**
`rt_str_validate` scanned all 1,024 liveness entries on every `rt_str_byte`,
`rt_str_set_byte` and `rt_str_len`, so building a 64 KiB string byte by byte
cost ~16 ms (8,000 builds: 132 s). The runtime now recognises image literals by
their known range, and caches the liveness entry that last validated a heap
string, re-checking both the live flag and the address before trusting the hint
(`BSS.str_validate_slot` / `str_validate_start`). Measured 132 s → 3.1 s (43×)
for the same loop. Regression:
`tests/capacity.rs::cap09_validation_fast_paths_alternate_strings`.

**Found, left open — mixed-size churn exhausts the arena.**
`validation/m2_bigio/churn_repro.mink` alternates a 64 KiB and a 1 KiB
allocation, freeing each: after ~16,000 iterations the run dies with `E-R02`.
The first-fit free list serves the small request from the large block,
re-records it at the small size and drops the blocks ahead of it, so the arena
grows by 64 KiB per iteration. Two prototyped fixes (capacity recording,
best-fit) both broke `tests/zlib_lib.rs` with `E-R05`, which proves the
liveness-table size is load-bearing for interior memory access; a correct fix
needs coalescing or size classes and a dedicated allocator change.
`tests/capacity.rs::cap08_single_size_churn_reuses_the_free_list` pins the
behaviour that does work (single size class, 200,000 cycles).

## Limits reconfirmed

Child stdin is capped at 4,096 bytes by `process_run_input` (documented,
measured in app B), stdio capture is 8 MiB per stream, program stdin 8 MiB−8,
the heap 1 GiB with 1,024 live allocations, the SQL engine copies its database
per statement and has no `SUM`, and the HTTP server serves one connection at a
time. None of these was disguised as a defect.

## Verification

| Check | Result |
|-------|--------|
| `cargo test --all-targets` (debug) | 109 blocks, 3,610 passed, 0 failed, 9 ignored |
| `cargo test --release --all-targets` | 109 blocks, 3,610 passed, 0 failed, 9 ignored |
| app harnesses A, B, C | 25/25, 8/8, 11/11 |
| app harnesses D, E, G | PASS, invariants held, PASS |
| app F soak (10 s) | 238,152 loops, 23,816 threads, exit 0 |
| `tools/release_acceptance.py` | PASS |

## Verdict

**M2: PARTIAL.** All seven workloads execute and their behaviour matches the
reference; the largest runtime performance defect was found and fixed with a
regression. One genuine runtime defect (mixed-size arena growth) is documented
with a minimal reproduction and an explicit plan, so the milestone is not
complete. M6 remains outstanding and M8 gated; the next milestone should start
with the allocator.
