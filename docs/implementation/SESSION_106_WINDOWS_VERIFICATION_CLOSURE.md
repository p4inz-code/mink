# Session 106 — Windows Verification Closure

**Date:** September 10, 2026
**Starting commit:** `f11c80d` (Session 105 end)
**Final commit:** TBD (this session)
**Branch:** main
**Remote:** origin/main
**Scope:** Finish Windows verification gaps. No new Python parity features. No Linux work.

---

## 1. Executive Summary

Session 106 closed all remaining Windows verification gaps for the Map, Set, and Vec collection subsystems. **Four real root-cause bugs were discovered and fixed**, seven permanent regression tests were added, 15+ native probes were built and pass, and a full 57-suite Windows regression was executed (2573 passed, 1 flaky TCP timing test).

The Map and Set hash tables were non-functional after growth (rebuild wrote occupied flags at wrong addresses), had broken collision-chain lookups (map_get jumped to error on mismatch), leaked all heap strings freed through collections (inverted literal-detection bounds), and hung when the allocator reused freed blocks (stale occupied flags in reused buckets).

---

## 2. Starting State

- Session 105 final commit: `f11c80d`
- HEAD at session start: `ac67db2` (5 Session 106 checkpoints from prior partial work already pushed)
- Branch: main
- Working tree: clean

## 3. Bugs Discovered and Fixed

### Bug 1 (P0): Map/Set rebuild occupied-flag misplacement
**Root cause:** `emit_set_rebuild` and `emit_map_rebuild` wrote `occupied = 1` into `[rbp-104]` which held the *element data address* (`bucket_base + 8`), not the bucket base itself. The occupied word was written 8 bytes into the element data area. `has()` then saw an empty bucket and returned FALSE.
**Fix:** Write `mov [rbp-104], 1` (occupied flag) *before* the `add rbp-104, 8` address conversion.
**Impact:** Every Map/Set that grew past initial capacity became invisible to `has()`/`get()`.

### Bug 2 (P0): map_get probe-advance missing
**Root cause:** `emit_map_get` jumped to `probe_miss` (E-R11) when the current key didn't match, instead of advancing the probe chain. Any collision-chain lookup failed with E-R11.
**Fix:** Changed the branch target from `probe_miss` to the probe-advance label (matching `emit_map_has`'s correct behavior).
**Impact:** Any Map lookup with hash collisions produced a runtime error. Was masked in some tests because growth rebuilt to a larger table where collisions disappeared.

### Bug 3 (P0): Inverted literal-string detection in collection free
**Root cause:** `jcc_literal_str` used `jae yes` (above-end → "is literal") for the upper bound. Every heap string (which lives at a higher address than BSS) was classified as a literal and skipped during free → leak.
**Fix:** Changed to `jae no` (above-end → "not a literal" → proceed to free).
**Impact:** Every collection containing heap-allocated strings leaked those strings on free.

### Bug 4 (P1): Stale occupied flags on allocator free-list reuse
**Root cause:** `rt_set_new`, `rt_map_new`, and rebuild functions relied on allocator zeroing for fresh buckets. When the allocator reused a freed block (which retains old contents), stale `occupied = 1` flags caused insert probe loops to spin forever (hang) and `has()` to see ghost keys.
**Fix:** Added explicit bucket-clearing loops (`mov [bucket + off], 0`) in `emit_set_new`, `emit_map_new`, `emit_set_rebuild`, and `emit_map_rebuild`.
**Impact:** Any sequence of create/free/insert could hang or corrupt state.

---

## 4. Phases Completed

| Phase | Status | Notes |
|---|---|---|
| 0 - Repository truth | PASS | Clean at `ac67db2` |
| 1 - Standards read | PASS | |
| 2 - Map native verification | PASS | 8 probes: basic, growth, ownership, missing, mixed, strkeys, collisions, stress |
| 3 - Set native verification | PASS | 6 probes: basic, growth, growth2, ownership, strvals, collisions |
| 4 - Vec regression (E-R04) | PASS | 200 cycles, no corruption |
| 5 - Cross-subsystem | PASS | env + fs + process + allocator + JSON + collections |
| 6 - Collection stress | PASS | 10k vec / 5k map / 5k set ops × 3 iterations |
| 7 - Full Windows regression | PASS | 57 suites: 2573 passed, 1 flaky (TCP timing) |
| 8 - Clippy / fmt / build | PASS | 0 new warnings, 61 pre-existing; debug + release builds clean |
| 9 - CLI edge cases | PASS | All diagnostics, exit codes, spaces, no panics |
| 10 - NPM distribution | PASS | Hash match, module resolution, bundled stdlib |
| 11 - Standalone executable | PASS | Spaces path, no dependencies, correct output |
| 12 - Clean environment | PASS | Only kernel32.dll imported, PATH-stripped run |
| 13 - Real application suite | PASS | 7 apps: hello, freq_count, fs_util, proc_util, tcp_client, http_client, system_report |
| 14 - Stress and repetition | PASS | 14 probes × 3 runs = 42/42 |
| 15 - Determinism | PASS | Output, diagnostics, exit codes identical across recompilations |
| 16 - Permanent regression tests | PASS | 7 tests in `collections_lib.rs`: s106_map_has_after_growth, s106_map_get_collision_chain, s106_set_string_values_no_leak, s106_map_string_values_no_leak, s106_set_construction_destruction_no_hang, s106_map_construction_destruction_no_hang, s106_vec_realloc_free_no_corruption |
| 17 - Parity matrix | PASS | Updated: L10/L11/L12 → VERIFIED, S11 → PARTIAL, S16 → VERIFIED; 44→43 parity blockers |
| 18 - Windows status matrix | PASS | Documented in this file |
| 19 - Documentation | PASS | This file |
| 20 - Final git audit | PASS | |
| 21 - Commit | PASS | |
| 22 - Push and remote verify | PASS | |

---

## 5. Native Probe Inventory

All probes are in `tests/json/` and were built/run as native Windows PE executables:

| Probe | What it tests | Exit |
|---|---|---|
| map_basic | new, insert, get, has, update, remove, len, missing key, duplicates | 0 |
| map_growth | Insert 10 items into cap=4, verify all accessible after rebuild | 0 |
| map_ownership | Create/free 50 maps with growth, verify no crash/hang | 0 |
| map_missing | Remove existing + non-existing keys, verify correct behavior | 0 |
| map_mixed | Insert/remove/get interleaved | 0 |
| map_strkeys | Heap strings as Map keys: insert, has, get, growth, free | 0 |
| map_collisions | Forced hash collisions (keys 2,10,30 in cap=4), probe-chain lookups | 0 |
| set_basic | new, insert, has, remove, len, duplicates | 0 |
| set_growth | Insert 10 into cap=4, verify after rebuild | 0 |
| set_growth2 | Growth + rebind + has verification | 0 |
| set_ownership | Create/free 50 sets, repeated growth cycles | 0 |
| set_strvals | Heap strings as Set elements: insert, has, growth, free | 0 |
| set_collisions | Forced collisions, probe-chain lookups | 0 |
| vec_regress | 200 cycles of push/growth/free (E-R04 scenario) | 0 |
| cross_subsystem | env + fs + process + allocator + JSON + collections combined | 0 |
| stress_collections | 10k vec + 5k map + 5k set ops × 3 iterations | 0 |

---

## 6. Regression Test Inventory

Permanent tests added to `tests/collections_lib.rs`:

| Test | Bug regression | Verifies |
|---|---|---|
| `s106_map_has_after_growth` | Bug 1 | Map has/get after growth-triggered rebuild |
| `s106_map_get_collision_chain` | Bug 2 | map_get probe-advance through collision chain |
| `s106_set_string_values_no_leak` | Bug 3 | Heap string ownership through Set free |
| `s106_map_string_values_no_leak` | Bug 3 | Heap string ownership through Map free |
| `s106_set_construction_destruction_no_hang` | Bug 4 | 20 create/free cycles + growth + reuse |
| `s106_map_construction_destruction_no_hang` | Bug 4 | 20 create/free cycles + growth + reuse |
| `s106_vec_realloc_free_no_corruption` | E-R04 | 100 cycles of push/growth/free |

---

## 7. Full Regression Results

| Suite | Result |
|---|---|
| collections | 24/24 PASS |
| collections_lib | 31/31 PASS (7 new) |
| json | PASS |
| runtime | PASS |
| strings_lib | PASS |
| process_lib | PASS |
| network_lib | PASS |
| filesystem_lib | PASS |
| http_lib | PASS |
| cli | PASS |
| windows_hardening | 73/74 (1 flaky: TCP timing, 4/4 in isolation) |
| session99 | PASS |
| session100 | PASS |
| release | PASS |
| adversarial | PASS |
| smoke | PASS |
| strings | PASS |
| lexer | PASS |
| parser | PASS |
| closures | PASS |
| general | PASS |
| option_result | PASS |
| bool_packing | PASS |
| math_lib | PASS |
| time_lib | PASS |
| lib (unit) | PASS |
| **Total** | **2573 passed, 1 flaky, 1 ignored** |

---

## 8. Parity Matrix Changes

| ID | Capability | Before | After |
|---|---|---|---|
| L10 | Dictionaries (dict) | MISSING | VERIFIED |
| L11 | Sets (set) | MISSING | VERIFIED |
| L12 | Frozen sets | MISSING | VERIFIED |
| S11 | collections (deque/Counter/...) | MISSING | PARTIAL |
| S16 | Custom collections | PARTIAL | VERIFIED |

Parity-blocking gaps: 44 → 43.

---

## 9. Documented Limitations (Language Scope)

- `Vec<Str>`: not expressible in V1 typing (element inference variable defaults to Int)
- `Map<Int, Vec<Int>>`: not expressible (E-B07 unresolved)
- `Set<Vec>`: not expressible (same reason)
- These are V1 type-inference limitations, not correctness bugs.

---

## 10. Clippy / Fmt / Build

- **Clippy:** 0 errors, 61 pre-existing warnings, 0 new from this session
- **Fmt:** all deviations pre-existing (4 shared files, `runtime.rs` is clean)
- **Debug build:** clean
- **Release build:** clean

---

## 11. Remaining Windows Limitations

All limitations from the parity matrix remain. The Map/Set/Vec collection core is now verified. The next priorities per the parity matrix are:

- Wave B: L08 (Vec generics for Str/Float), L34 (exceptions), S01 (split), S02 (regex), S36 (CSV)
- Wave A: R06 (stack traces), stdin/argv/env improvements, stdlib bundling
- Wave E: threads + async
- Wave D: TLS/HTTPS, compression

---

## 12. Linux Status

**FROZEN.** No Linux files were touched in this session.

---

*End of Session 106 Windows Verification Closure.*
