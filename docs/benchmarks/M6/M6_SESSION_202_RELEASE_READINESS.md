# Session 202 — Release Readiness Audit and Consolidated Report (Sessions 197–202)

Windows x86_64, 2026-11-07. Starting HEAD
`68aec0fabc500e74a0189e3cb7a174dc61558e88` (Session 201, local == origin/main
verified before any change).

This session verifies the six-session reliability programme as a release
candidate and closes it with one consolidated report. The sections below are
the report's **A–G** structure; every count is measured on this machine.

---

## A. Scope and method

The programme audited and hardened the Windows x86_64 runtime, compiler and
standard library across six sessions (197–202), each with its own commit,
push, and local == `origin/main` check before the next began. Session 202 run
the following release checks against the final tree:

| Check | Command | Result |
| --- | --- | --- |
| Full regression, every target except `tls_lib` | per-target `cargo test` (79 targets) | **3,218 passed / 0 failed / 7 ignored / 79 suites** |
| Debug library tests | `cargo test --lib` | included in the 79 suites |
| Release build | `CARGO_INCREMENTAL=0 cargo build --release` | exit 0, `target/release/mink.exe`, **4,043,776 bytes** |
| Release diagnostics | `cargo test --release --test diagnostics` | **24 passed / 0 failed** |
| Release soak | `cargo test --release --test soak -- --ignored` | **7 passed / 0 failed** (8.34 s) |
| Release lifecycle | `cargo test --release --test resource_lifecycle` | **7 passed / 0 failed** (29.3 s) |
| Working tree | `git status --short` | only the four pre-existing untracked files |
| Tags / publishing | `git tag` | 5 pre-existing tags; **no tag, release or npm publish created** |

The seven ignored tests are the bounded soak workloads, run explicitly with
`--ignored` (their result is quoted separately above).

## B. Session-by-session results

| Session | Work | Evidence |
| --- | --- | --- |
| 197 | Generation-tagged collection handles (`Vec`/`Map`/`Set`); stale handles fault `E-R05`/105 | 11 `s197_*` tests; 75 suites, 3,185 passed; binary 4,041,216 B |
| 198 | Runtime resource-lifecycle audit with OS-handle sampling + negative control | 7 tests, 7/7 debug and release; flat handles across 3,000 fs cycles, 3,000 failing calls, 40 processes, 3,000 threads, 200,000 allocations |
| 199 | Bounded soak: compiler, launches, collections, filesystem, processes, error paths, combined | 7 workloads, 7/7 release in 8.34 s; 400,000 collection cycles, 3,000 fs cycles + 5 × 1 MiB reads, 100 processes, 20,000 error-path iterations |
| 200 | Compiler correctness and diagnostics audit | 24-case battery, 24/24 debug and release; full regression 3,217 passed / 78 suites |
| 201 | Standard-library surface and CLI DX review | 700 declarations, 462 public, coverage 397/462 → **0 missing**; guard test passes; CLI contract verified |
| 202 | Release readiness (this record) | 3,218 passed / 0 failed / 7 ignored / 79 suites; release build clean |

Session totals: **51 new tests** (11 + 7 + 8 + 24 + 1), **3 real defects fixed**
(§C), **1 documented risk left unpatched**, **6 benchmark records** added.

## C. Confirmed defects found and fixed

| # | Session | Defect | Root cause | Fix | Regression |
| --- | --- | --- | --- | --- | --- |
| 1 | 197 | Freed `Vec`/`Map`/`Set` handles read the replacement block's header after the address was reused, returning stale values with exit 0 | Handles carried only an address; the liveness table had no lineage, so a freed-then-reused address was indistinguishable from a live handle | Collection handles now carry the allocation generation in bits 48..63; all 18 collection services validate address + generation (`E-R05`, 105) | 11 `s197_*` tests (reuse, repeated cycles, nesting, wraparound, concurrency) |
| 2 | 200 | Integer literals outside `Int` silently wrapped: `9223372036854775808` printed `-9223372036854775808`, `999999999999999999999999` printed `2003764205206896639`, both exit 0; the same happened in match patterns | Both literal decoders used `wrapping_mul`/`wrapping_add` + `as i64`; the AST stores only spans, so nothing checked the source text against the 64-bit range | Type checking decodes with checked `u64` arithmetic and reports **`E-T43`** for expressions and recursively for patterns; exactly `2^63` is allowed only as `-9223372036854775808` | `d14`–`d19`, `d23`–`d24` |
| 3 | 200 | Every aggregate type diagnostic rendered `` `` is not a valid key type: …`` (the `E-T42` prefix), so `E-T11` array-index errors were mislabelled | `Display for TypeError` grouped aggregate kinds with `InvalidCollectionKey` and reused its format string | Aggregate kinds print their stored message; `InvalidCollectionKey` keeps its own arm | `d20`, `d21` |

No confirmed defect was found in the Session 198 lifecycle paths, the Session
199 soak paths, or the Session 201 standard-library surface; those sessions
add evidence, not fixes.

## D. Tests, evidence and documentation

* **New suites:** `tests/resource_lifecycle.rs` (7), `tests/soak.rs` (8, seven
  `#[ignore]`d and run explicitly), `tests/diagnostics.rs` (24),
  `tests/stdlib_docs.rs` (1), and 11 `s197_*` tests in `tests/collections_lib.rs`.
* **Final regression:** 3,218 passed / 0 failed / 7 ignored / 79 suites.
* **Release artefacts:** optimized CLI 4,043,776 bytes; release-mode runs of
  the diagnostics, soak and lifecycle suites all green.
* **Records:** one benchmark record per session
  (`M6_SESSION_197…202_*.md`), each with exact measurements and a verdict.
* **Durable documentation:** roadmap dated status blocks for 197–202; systems
  readiness audit addenda for 198–202; standard-library reference §26 now
  covers the complete public surface (462/462) and is guarded by a test.

## E. Residual risks and limitations (recorded, not fixed)

1. **`rt_fs_read` short-read risk (Session 198, unpatched).** The emitter
   ignores `ReadFile`'s BOOL and byte count and returns a string at the
   requested length, so a short or failed read could expose stale heap bytes.
   No deterministic trigger exists in the current API; documented with impact
   and a follow-up rather than patched without a failing test.
2. **Arena ceiling (1 GiB, no coalescing).** A repeated large-allocation
   pattern fails loudly with `E-R02` (102). Session 199 reproduced it at
   iteration 342 with 64 KiB payloads (peak WS 1027.1 MiB) and resized the
   soak inside the documented limit instead of changing the allocator.
3. **`MAX_LIVE_ALLOCS = 1024`** simultaneously live allocations (`E-R03`/103
   beyond); unchanged architectural limit.
4. **16-bit handle generation.** A stale handle can alias a replacement only
   after 65,536 reallocations of the same block; valid handles survive
   wraparound (tested).
5. **Backend literal decoder still wraps by design.** Safe because type
   checking rejects out-of-range literals first; any new entry point that
   bypasses type checking must apply the same rule.
6. **Bounded, not open-ended, evidence.** Soak budgets are 180 s and the whole
   soak run is under 10 s; this does not establish multi-hour stability.
7. **Documentation depth.** §26 is mention-complete, not signature-executed;
   §24 of the reference still lists modules whose edge cases are source-only.
8. **Pre-existing `cargo fmt --check` diffs (50)** in files untouched by these
   sessions are unchanged; every changed file is `rustfmt`-clean.

## F. Environment limitations

* `tls_lib` could not run: `python3` on this machine is the Microsoft Store
  alias stub, so the TLS test server exits before MINK code runs (2 passed,
  18 server-start failures, 1 ignored). Reported as **not run**, not a defect.
* One release rebuild crashed `rustc` with
  `STATUS_ILLEGAL_INSTRUCTION (0xc000001d)` in the incremental-compilation
  path; `CARGO_INCREMENTAL=0` builds cleanly and was used for every recorded
  release result. Toolchain/environment event, not a source defect.
* All measurements are from Windows x86_64; Linux remains frozen.

## G. Verdict

**PASS — release-ready within the documented Windows x86_64 claim boundary.**
The full suite is 3,218 passed / 0 failed / 7 explicitly ignored across 79
suites, the release build and release-mode audit suites are green, three
confirmed defects were fixed with permanent regression tests, and every
residual limit is recorded above. This does not close M6: no external
developer has attempted the predefined validation tasks, M7 remains gated on
demonstrated demand, and Linux remains frozen.
