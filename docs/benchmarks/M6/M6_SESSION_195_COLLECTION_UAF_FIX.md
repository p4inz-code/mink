# Session 195 — Collection Use-After-Free Detection

Windows x86_64, 2026-11-06. Starting HEAD
`41f678949aca42573767baf3db9ed1e09f41d92f`.

## 1. Defect

Using a freed `Vec`, `Map` or `Set` handle returned stale bytes and exited 0.
The runtime detects double-free (`E-R04`), out-of-bounds collection access
(`E-R10`) and leaks (`E-R06`), but the collection services read their header
directly from the handle without consulting the liveness table.

Reproduced before the fix with the release compiler:

| Program | Expression | Before | After |
| --- | --- | --- | --- |
| `uaf_vec_len` | `rt_vec_len` after free | `48`, exit 0 | `E-R05`, exit 105 |
| `uaf_vec_get` | `rt_vec_get` after free | `77`, exit 0 | `E-R05`, exit 105 |
| `uaf_vec_push` | `rt_vec_push` after free | `33`, exit 0 | `E-R05`, exit 105 |
| `uaf_map_get` | `rt_map_get` after free | `42`, exit 0 | `E-R05`, exit 105 |
| `uaf_map_len` | `rt_map_len` after free | `144`, exit 0 | `E-R05`, exit 105 |
| `uaf_set_len` | `rt_set_len` after free | `96`, exit 0 | `E-R05`, exit 105 |
| `uaf_set_has` | `rt_set_has` after free | `0`, exit 0 | `E-R05`, exit 105 |
| double free | `rt_vec_free` twice | `E-R04`, 104 | unchanged |
| out of bounds | `rt_vec_get(v, 5)` | `E-R10`, 110 | unchanged |
| freed-then-reused | free `v`, allocate `w`, read `v` | aliases `w`, exit 0 | aliases `w`, exit 0 |

The stale values were deterministic (5/5 and 3/3 repeats): the LIFO free list
writes a fixed link word into the freed block, and the collection services read
that word as `length`.

## 2. Root cause

`emit_vec_len`, `emit_vec_get`, `emit_vec_push`, `emit_vec_set`, `emit_vec_pop`,
`emit_vec_remove`, the seven `Map` accessors and the five `Set` accessors all
begin by loading `[handle+0]`, `[handle+8]` or `[handle+16]`. The liveness table
was consulted only by `rt_free`, `rt_mem_load` and `rt_mem_store`. The gap was
shared by all three collection types and documented as an unsupported contract
(`STANDARD_LIBRARY_REFERENCE.md` §5.2), never fixed.

## 3. Implementation

Added `emit_require_live_handle` in `src/backend/emit/runtime.rs`. It:

1. takes the runtime lock,
2. scans the liveness table for a live entry whose `start` equals the handle,
3. releases the lock on success and returns,
4. calls `fail(code, 5)` (`E-R05`, exit 105) when no live entry matches.

It is invoked immediately after `prologue` in **18 services** — all collection
operations that read or write the handle:

| Type | Services guarded |
| --- | --- |
| `Vec` | `push`, `get`, `set`, `pop`, `remove`, `len` |
| `Map` | `insert`, `get`, `has`, `remove`, `len`, `keys`, `values` |
| `Set` | `insert`, `has`, `remove`, `len`, `elements` |

`*_new` and `*_free` are deliberately unchanged: the free path already
validates exact live starts and must keep reporting double-free as `E-R04`, not
`E-R05`. No runtime service was added, so no frozen Linux emitter was touched.

## 4. Regression tests

Eleven tests in `tests/collections_lib.rs`:

- nine use-after-free cases: `Vec` len/get/push, `Map` get/len/keys, `Set`
  has/len/elements — each asserts exit **105** and `E-R05`;
- `s195_double_free_still_reports_invalid_free` — exit **104**, `E-R04`;
- `s195_out_of_bounds_still_reports_110` — exit **110**, `E-R10`.

## 5. Verification

| Check | Result |
| --- | --- |
| Reproductions with the release compiler | all seven now `E-R05`/105; double-free 104; OOB 110 |
| `cargo test --test collections_lib` | **60 passed, 0 failed** (49 before + 11 new) |
| Full `cargo test` | exit 0 — **3,194 passed, 0 failed, 1 ignored, 78 suites** |
| `cargo build --release` | exit 0 — `target/release/mink.exe`, 4,030,976 bytes |
| `invsync` verifier | 12/12 PASS |
| `git diff --check` | exit 0 |
| `rustfmt --check` on `tests/collections_lib.rs` | clean |
| `rustfmt --check` on `src/backend/emit/runtime.rs` | pre-existing `NetSendTo` formatting drift only; the new helper is formatted consistently |

## 6. Limitation that remains

The liveness table has no generation counter. If a handle is freed and the same
block is then reused by a new collection, the stale handle aliases the new
object and no error is raised (`uaf_after_reuse` still exits 0). This is
documented in the reference. Fixing it would require per-handle generations or
a handle table and is a separate design decision, not a regression of this fix.

## 7. Documentation

- `docs/language/STANDARD_LIBRARY_REFERENCE.md` §5.2 and §23: the limitation is
  replaced by the diagnosed behaviour and the residual alias limitation.
- `docs/implementation/RUNTIME_IMPLEMENTATION.md` §2: collection handles are
  validated against the liveness table.
- `M6_DEFERRED_SESSIONS.md`: dated closure note for item 6.
- No `CHANGELOG.md` exists in this repository, so no changelog entry was added.

## 8. Next action

Session 196: independently verify the committed fix, rerun the full matrix and
bounded Windows stress scenarios, audit the diff, and close the milestone in the
roadmap and systems-readiness audit without overclaiming M6.
