# Session 197 — Generation-Safe Collection Handles

Windows x86_64, 2026-11-06. Starting HEAD
`42e8caa54e9d368981723bcef694750c9e44af65` (verified before any change).

## 1. Defect

Session 195 made every `Vec`/`Map`/`Set` service validate its handle against
the liveness table, so a freed handle faults `E-R05` (105). That fix could not
distinguish a stale handle from a *new* object that reuses the same block: the
handle was the block address, and nothing in the value identified which
allocation it was issued for. After

```mink
let v = rt_vec_new(4);
rt_vec_push(v, 7);
rt_vec_free(v);
let w = rt_vec_new(4);
rt_vec_push(w, 9);
rt_print_int(rt_vec_len(v));   // printed w's length, exit 0
```

the freed handle `v` aliased `w`. This was recorded as a known limitation, not
a resolved defect.

## 2. Root cause

A collection handle is a single 64-bit word. Every runtime service read the
collection header directly from that word (`[handle+0]`, `[handle+8]`,
`[handle+16]`), so the word had to be the buffer address. The liveness table
stored `(start, size, live)`; `start` is the address, which is exactly the
information the handle already carries. Once a block is freed and handed to a
new allocation, the table entry is live again at the same start, and the old
handle is indistinguishable from the new one. No in-band header check can
help: the new owner rewrites the header it reads.

## 3. Design options evaluated

| Option | Identity in handle | Compatibility | Verdict |
| --- | --- | --- | --- |
| Document the limitation | no | none needed | rejected — the defect is reachable from safe MINK source |
| Quarantine freed collection blocks | no | none | rejected — unbounded memory growth for create/free loops |
| 16-byte handles (pointer + generation) | yes | breaks the 8-byte word ABI and every collection call site | rejected |
| Indirect handle table (slot + generation) | yes | new table, slot allocation/free at every mint/free site, extra failure mode | rejected as larger than necessary |
| **Generation tag in the handle's high bits** | yes | 8-byte handle preserved; handles are already opaque | **chosen** |

Windows x86-64 user-mode addresses occupy the low 47 bits, so bits 48..63 of
the word cannot be part of a real address. The runtime is the only component
that dereferences a collection handle, and it now strips the tag before any
header access. MINK source cannot observe a handle's numeric value (a `Vec` has
no cast to `Int` and no equality or hashing contract), so this is not a
user-visible representation change. The MINK ABI — 8-byte opaque word, same
services, same calling convention — is unchanged.

## 4. Implementation

### 4.1 Generation storage

The liveness entry's third word was documented as a `u64` live flag but only
its low byte is ever read. It is now
`(generation << 32) | live`:

- `LIVE_ENTRY_GENERATION_SHIFT = 32` (`src/runtime/abi.rs`);
- entry size stays 24 bytes and the table layout is unchanged;
- `TableExhausted`, leak-scan and free-scan reads (byte at `+16`) are unchanged.

### 4.2 Generation lineage

A generation must follow the **block**, not the table slot: the allocator
records each allocation in the first dead slot, so a reused block usually gets a
different slot than its previous occupant (recording it in a fresh slot would
restart that slot's counter at 1 and let a stale handle match). The free list
has two words of metadata per block (`+0` next link, `+8` size); a third
(`+16`) is now used when the block is at least 24 bytes — exactly the minimum
size of a collection buffer, which is the only object that needs identity.

- `rt_free` writes the block's current generation to `block+16` before linking
  it into the free list.
- `rt_alloc` reads `block+16`, increments it, masks to 16 bits, and records
  `(generation << 32) | live` in whatever slot it selects. Fresh blocks are
  loader-zeroed, so their first generation is 1.

Every allocation that reuses a block — collection or raw — therefore advances
that block's lineage and invalidates every handle issued for a previous
occupant.

### 4.3 Handle tagging

Handles are `(generation << 48) | address`. `HANDLE_ADDRESS_MASK` recovers the
address; `HANDLE_GENERATION_SHIFT` is 48.

- `emit_tag_collection_handle` stamps a freshly created buffer from its live
  entry. It is applied at every site that hands a collection buffer to MINK
  code: `rt_vec_new`, `rt_vec_push` (including growth), `rt_vec_set`,
  `rt_vec_remove`, `rt_map_new`, all three `rt_map_insert` returns,
  `rt_map_remove`, `rt_map_keys`, `rt_map_values`, `rt_set_new`, all three
  `rt_set_insert` returns, `rt_set_remove`, `rt_set_elements`, and the three
  deep-clone paths in `CollCloneValue`.
- `emit_require_live_handle` (18 services, unchanged call sites) now masks the
  address, requires a live entry at that address **and** an equal generation,
  then writes the untagged address back to `[rbp+16]`. The service body below
  it is untouched and still works with a plain buffer pointer.
- `emit_unpack_handle_for_free` unpacks the handle in `rt_vec_free`,
  `rt_map_free` and `rt_set_free`. A live entry with a newer generation is a
  stale handle (`E-R05`, 105). A dead entry is left to the existing free path,
  which keeps reporting a double free as `E-R04` (104).
- `CollFreeValue` and `CollCloneValue` strip the tag from nested collection
  handles before reading a header, and `CollCloneValue` tags the buffer it
  creates.

`rt_alloc`, `rt_free`, string services and every raw pointer service are
otherwise unchanged: tags exist only on collection handles.

## 5. Residual limits

- The tag is 16 bits. A stale handle can alias a replacement only after the
  same block has been reallocated 65,536 times, and then only if the stale
  handle is still being used. Valid handles survive wraparound (tested).
- Cross-type misuse (`rt_map_len` on a `Vec` handle) is not diagnosed because
  the tag identifies an allocation, not a type. The MINK type checker rejects
  it statically (`E-T01`), and only the untyped `rt_*` intrinsics could bypass
  that. Unchanged from before this session.
- A forged or laundered handle value cannot be produced from safe MINK source;
  an arbitrary integer is a type error, so invalid-handle cases are exercised
  through freed and reused handles.
- The Rust reference allocator (`src/runtime/allocator.rs`) models the
  allocator core (allocation, free, liveness); generation tagging is an emitter
  concern and is not modelled there.

## 6. Regression tests

Eleven tests appended to `tests/collections_lib.rs`
(`s197_*`), all native-PE executions:

| Test | Asserts |
| --- | --- |
| `s197_vec_stale_after_reuse_is_an_error` | freed `Vec` after reuse → 105 / `E-R05` |
| `s197_vec_get_stale_after_reuse_is_an_error` | stale `rt_vec_get` → 105 |
| `s197_map_stale_after_reuse_is_an_error` | freed `Map` after reuse → 105 |
| `s197_set_stale_after_reuse_is_an_error` | freed `Set` after reuse → 105 |
| `s197_free_after_reuse_reports_a_stale_handle` | stale free does not release the replacement → 105 |
| `s197_repeated_cycles_keep_valid_handles` | 200 alloc/use/free cycles, exact sum → exit 0 |
| `s197_repeated_cycles_then_stale_handle_is_an_error` | 200 cycles then first handle → 105 |
| `s197_nested_vec_collections_survive_tagging` | `Vec<Vec<Int>>` push/get/free → 1, 5, exit 0 |
| `s197_map_vec_values_survive_tagging` | `Map<Int, Vec<Int>>` lifecycle → 1, 11, exit 0 |
| `s197_generation_wraparound_keeps_valid_handles` | 70,000 cycles, fresh handle valid → exit 0 |
| `s197_concurrent_collection_cycles` | 4 threads × 100 cycles, exact total 400 → exit 0 |

Session 195's eleven tests (freed handles, double free 104, out-of-bounds 110)
still pass unchanged.

## 7. Verification

| Check | Result |
| --- | --- |
| `cargo build` | exit 0 |
| `cargo build --release` | exit 0 — `target/release/mink.exe`, 4,041,216 bytes (was 4,030,976; +10,240 for the generation paths) |
| `cargo test --test collections_lib` | **71 passed, 0 failed** (60 before + 11 new) |
| Full suite, every target except `tls_lib` | **75 suites, 3,185 passed, 0 failed, 0 ignored** |
| Baseline reconciliation | 3,194 recorded − 20 `tls_lib` passes + 11 new = 3,185 |
| `tls_lib` | not runnable on this machine: `python3` is the Microsoft Store alias stub, so the TLS test server cannot start (2 passed, 18 failed at server start, 1 ignored). Environmental, pre-existing, unrelated to this change. |
| Manual probes | stale `Vec`/`Map`/`Set` → 105; double free → 104; nested `Vec<Vec<Int>>`/`Map<Int, Vec<Int>>` and 100-entry map growth → exit 0 |
| `rustfmt --check` | clean on the changed files |

One observation for Session 199: the first full-suite run produced a single
failure in `hashing_lib` (`h05_djb2_empty`, printed `64`). The same test passes
in isolation and in four repeated runs; that harness names its temporary files
from a nanosecond timestamp and invokes `cargo run` from 29 parallel test
threads, so a name collision is the likely cause. It is a test-harness
reliability issue, not compiler behaviour, and is left for the soak session.

## 8. Documentation

- `docs/implementation/RUNTIME_IMPLEMENTATION.md` §2: generation validation and
  the handle tag.
- `docs/language/STANDARD_LIBRARY_REFERENCE.md` §5.2 and §23: the
  freed-then-reused limitation is replaced by the diagnosed behaviour and the
  residual 16-bit bound.
- `docs/roadmap/IMPLEMENTATION_ROADMAP.md`: dated Session 197 status.
- `docs/benchmarks/M6/M6_DEFERRED_SESSIONS.md`: dated correction.
- `docs/audits/SYSTEMS_READINESS_AUDIT.md`: limitation replaced with the
  generation-safe result.
- No `CHANGELOG.md` exists in this repository.

## 9. Verdict

Defect fixed, tested and documented. M6 external validation remains
outstanding; M7 remains gated; Linux remains frozen and untouched (the frozen
Linux emitter was not modified — the change is in the Windows runtime emitter
and the shared ABI constants only).
