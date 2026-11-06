# Session 196 — Full Windows Reliability Verification and Milestone Closure

Windows x86_64, 2026-11-06. Session 194 audit
`41f678949aca42573767baf3db9ed1e09f41d92f`; Session 195 fix
`a8acc5cd27120b611ea632d3c777393353ca35c4`; documentation
`0c56cde75e0b39a095a2f12da50e7438f62ee15f`. Starting HEAD for this session is
`0c56cde` (fix + record), with local, tracking and remote equal.

## 1. Committed state verified

| Check | Result |
| --- | --- |
| `git log --oneline` | `0c56cde` docs · `a8acc5c` fix · `41f6789` audit |
| `a8acc5c` files | `src/backend/emit/runtime.rs` (+59), `tests/collections_lib.rs` (+176) |
| `0c56cde` files | four documentation files only |
| Tracked diff vs HEAD | empty — no uncommitted implementation change |
| `git ls-remote origin refs/heads/main` | `0c56cde…` |
| Ahead/behind vs `origin/main` | `0 0` |
| Untracked user files | all four present and untouched |
| Build artifact cleanup | a stray `benchmarks/m6/session180/invsync.exe` from earlier builds was removed; it was generated output, not a user file |

The fix commit contains no unrelated source changes, and the documentation
commit contains no source changes.

## 2. Independent defect verification

Rebuilt from the committed tree with the release compiler. Thirteen
use-after-free programs, each freeing the handle and then calling one accessor:

| Program | Operation after free | Result |
| --- | --- | --- |
| `v_cap2` | `rt_vec_len`, capacity 2 | exit 105, `E-R05`, empty stdout |
| `v_cap64` | `rt_vec_len`, capacity 64 | exit 105, `E-R05`, empty stdout |
| `v_partial` | `rt_vec_get` after three pushes | exit 105, `E-R05`, empty stdout |
| `v_set` | `rt_vec_set` | exit 105, `E-R05`, empty stdout |
| `v_pop` | `rt_vec_pop` | exit 105, `E-R05`, empty stdout |
| `v_remove` | `rt_vec_remove` | exit 105, `E-R05`, empty stdout |
| `m_partial` | `rt_map_get` after insert/remove | exit 105, `E-R05`, empty stdout |
| `m_insert` | `rt_map_insert` | exit 105, `E-R05`, empty stdout |
| `m_remove` | `rt_map_remove` | exit 105, `E-R05`, empty stdout |
| `m_has` | `rt_map_has` | exit 105, `E-R05`, empty stdout |
| `m_values` | `rt_map_values` | exit 105, `E-R05`, empty stdout |
| `s_insert` | `rt_set_insert` | exit 105, `E-R05`, empty stdout |
| `s_remove` | `rt_set_remove` | exit 105, `E-R05`, empty stdout |

Controls:

| Case | Expected | Observed |
| --- | --- | --- |
| double free (`rt_vec_free` twice) | 104 / `E-R04` | 104 / `E-R04` |
| out of bounds (`rt_vec_get(v, 5)`) | 110 / `E-R10` | 110 / `E-R10` |
| leak (`rt_vec_new`, exit without free) | 106 / `E-R06` | 106 / `E-R06` |
| valid Vec/Map/Set round trip | 0, stdout `1521` | 0, stdout `1521` |
| freed-then-reused handle | documented alias, exit 0 | exit 0, output `00` |

Ten consecutive executions of `v_cap2`, `m_partial` and `s_remove` each returned
105 on every trial (`105` ×10 for each). No trial printed stale data.

The residual freed-then-reused alias is the limitation Session 195 documented:
the liveness table has no generation counter. It is unchanged and not presented
as fixed.

A first probe script in this session used `git rev-parse` after changing
directory, so every probe was reported as a build failure. That run is discarded;
the results above come from the corrected run, which passed 42/42 assertions.

## 3. Cross-subsystem regression

| Check | Result |
| --- | --- |
| `cargo test` | exit 0 — **3,194 passed, 0 failed, 1 ignored, 78 suites** |
| `collections_lib` | 60 passed |
| `filesystem_lib` | 47 passed |
| `strings_lib` | 88 passed |
| `runtime` | 28 passed |
| `json` | 68 passed |
| `stdlib_ownership` | 11 passed |
| `crypto_lib` | 18 passed |
| `encoding_lib` | 75 passed |
| `hashing_lib` | 29 passed |
| `math_lib` | 106 passed |
| `random_lib` | 15 passed |
| `time_lib` | 29 passed |
| `release` | 69 passed |
| `cargo build --release` | exit 0 — 4,030,976 bytes |
| `invsync` verifier | all cases passed |
| `git diff --check` | exit 0 |
| `rustfmt --check tests/collections_lib.rs` | clean |

The 3,183 historical total is not used as the current result; 3,194 is a fresh
run that includes the eleven Session 195 tests.

## 4. Windows stress verification

Bounded workloads, release compiler, post-fix:

| Workload | Observed |
| --- | --- |
| 20,000 alloc/free pairs | exit 0, output `20000`, 39 ms |
| 50,000 Vec pushes | exit 0, output `50000`, 39 ms |
| 5,000 Vec create/free cycles | exit 0, output `5000`, 41 ms |
| 200 filesystem create/write/read/remove cycles | exit 0, output `200`, 76 ms |
| 256 KiB write/read | exit 0, output `262144`, 40 ms |
| 30 child processes with captured stdout | exit 0, output `30`, 671 ms |
| 5 repeated `invsync` builds | 5/5 succeeded, 157 ms |

No crash, no unexpected `E-R*`, no leak-check failure and no stale handle output
within the tested workloads. Timings are observed wall-clock values on this
machine, not benchmarks.

## 5. Final audit

- Changed source paths inspected: the liveness check is emitted once and called
  by 18 collection services; `*_new`/`*_free` remain unguarded so double-free
  stays `E-R04`.
- New tests assert exit codes and `E-R*` codes for nine use-after-free cases and
  both preserved error paths.
- Documentation states the `E-R05` contract and the residual alias limitation.
- No secret material or machine-specific paths found in the changed files.
- `rustfmt --check src/backend/emit/runtime.rs` still reports the pre-existing
  `NetSendTo` drift noted in Session 195; the new helper is unaffected. That
  drift was not introduced here and was left alone rather than reformatting
  unrelated code.

## 6. Milestone status at close

| Area | Status |
| --- | --- |
| Selected defect (collection use-after-free) | **FIXED AND VERIFIED** |
| Double-free / out-of-bounds / leak behaviour | **VERIFIED WITHOUT CHANGES** |
| Freed-then-reused handle alias | **PARTIALLY VALIDATED** — limitation documented, not fixed |
| Full regression | 3,194 passed, 0 failed, 1 ignored, 78 suites |
| Release build | 4,030,976 bytes, exit 0 |
| Windows runtime stress | bounded workloads clean, no soak claim |
| Linux | **FROZEN** |
| M6 | **NOT COMPLETE** — real outside developer validation still required |
| M7 | **NOT STARTED** — remains demand-gated |
| M8 | not reachable; not declared ready |

## 7. Deferred work

- Per-handle generations (or a handle table) to make freed-then-reused handles
  fault instead of aliasing. This is a design change, not a regression fix.
- Long-running soak testing; the bounded workloads here do not establish it.
- M6 external developer validation, which requires a developer who did not
  build MINK.
- npm publication, `v1.0.5` tag and GitHub release remain separately
  unauthorized.

## 8. Next action

No further implementation is selected by this milestone closure. The next open
roadmap item is M6 external developer validation, which cannot be performed
internally. M7 must not start without demand evidence.
