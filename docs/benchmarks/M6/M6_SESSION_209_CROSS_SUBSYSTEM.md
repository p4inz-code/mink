# Session 209 — Independent Cross-Subsystem Correctness Audit

Windows x86_64, 2026-11-07. Starting commit
`a6f07323f9b288219e06214697dd1940b8d591ee` (Session 208, local == origin/main
verified before any change).

## 1. Objective

Look for correctness failures at the seams between compiler, runtime and
standard library — cases that per-subsystem suites cannot see — without adding
speculative features or a random test framework.

## 2. Code paths inspected

- **Compiler → runtime:** intrinsic lowering (`src/backend/lower.rs`), the
  `RuntimeService` ABI, argument/return conventions, and the filesystem/alloc/
  collection service emitters in `src/backend/emit/runtime.rs`.
- **Runtime → standard library:** ownership transfer in
  `stdlib/filesystem.mink` (intrinsics borrow, user calls consume), string
  length ownership, collection handles and generations, and the arena/live
  table limits in `src/runtime/abi.rs`.
- **Seams chosen from prior defects:** filesystem reads holding data across
  other allocations (Sessions 197/198/203/204/206), generation retirement
  (204), failed `ReadFile` recovery (203), and live-table exhaustion (204).

## 3. New tests — `tests/cross_subsystem.rs` (4)

| Test | Interaction exercised | Result |
| --- | --- | --- |
| `cs01_file_reads_live_across_collection_churn` | 8 × 64 KiB reads held live through recursion and per-level `Vec` create/push/get/free; every length and first/last byte checked | **pass** (exit 0, leak check) |
| `cs02_stale_handle_survives_filesystem_churn` | freed `Vec` generation checked after 8 file read/free cycles | **pass** (`E-R05`/105) |
| `cs03_failed_read_leaves_runtime_usable` | locked file ⇒ owned empty string, then `Vec` work and a successful 4 KiB read in the same process | **pass** (0, 4096, 1) |
| `cs04_table_exhaustion_with_live_file_data` | 64 KiB file read held live while 1,024 `rt_alloc` blocks are made | **pass** (`E-R03`/103) |

No new defect was found. One test expectation was corrected during authoring
(the 65,536th fixture byte is 252, not 255); the product code was not changed.

## 4. Independent rechecks of previous fixes

The full regression reruns, from their own suites, the Sesions 203/204/206
reproductions: `fs_read` locked-file behavior (6 tests), large-file exactness
and refusal (5), generation exhaustion and table limits (7), collection
generation behavior (`collections_lib`), ownership (`stdlib_ownership`), and
the TLS suite (21 + 1 ignored). All passed; see the totals below.

## 5. Results

| Check | Result |
| --- | --- |
| `cargo test --test cross_subsystem` | **4 passed, 0 failed** (0.04 s) |
| Full regression, all suites | **85 suites, 3,262 passed, 0 failed, 9 ignored**, 0 non-zero suite exits |
| `rustfmt --check tests/cross_subsystem.rs` | clean |

Reconciliation from the Session 206 baseline (83 suites / 3,257 / 8 ignored):
Session 207 added one ignored soak test, Session 208 one signature test,
Session 209 four cross-subsystem tests. Nothing was removed or skipped.

## 6. Limitations

- No new defect means no new fix; the value is the four permanent seam tests.
- Release-mode re-runs remain blocked by the environmental `rustc` crash
  (Session 205); debug is used.
- The filesystem API remains whole-file, so offset-level seams cannot be
  exercised.

## 7. Documentation

- Roadmap and readiness-audit addenda.
- This record.

## 8. Verdict

**PASS — 4/4 cross-subsystem tests, no new defect, previous fixes independently
re-exercised, full regression clean at 3,262/0/9 across 85 suites.** Release
verification remains environment-blocked and is reported as not run.
