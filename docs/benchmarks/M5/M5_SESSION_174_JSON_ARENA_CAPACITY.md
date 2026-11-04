# Session 174 — M5 blocker closure: JSON arena capacity

**Date:** 2026-11-04
**Status:** **COMPLETE** (M5 closure session; M6 Session 174 test design is
NOT STARTED and deferred)
**Starting HEAD:** `d3f2211` (M5 final report commit)
**Scope:** close the single remaining M5 contract blocker: `hookbox` could not
accept valid JSON bodies of 4,097-8,192 bytes.

## 1. Reproduction

The committed hookbox suite reproduced the gap before any change:

```text
PASS  JSON body 4096 bytes -> 201 (largest accepted body)
PASS  JSON body 4097 bytes -> 400 (MINK stdlib arena limit)
PASS  JSON body 8192 bytes -> 400 (contract maximum refused by MINK)
PASS  JSON body 8193 bytes -> 413
```

The frozen contract (selection section 6) accepts JSON objects up to 8,192
bytes, so this was a genuine capability gap, not a test gap.

## 2. Root cause

`stdlib/json.mink` used one fixed geometry:

- input bytes occupy arena slots `0 .. input_len-1` (one Int slot per byte);
- slot `input_len` is a separator;
- the value tree starts at slot `input_len + 1`;
- the persistent write pointer lives in **slot 8191**, the last slot of a
  64 KiB (8192-slot) arena;
- `json_parse` refused any tree whose end reached slot 8191.

Two consequences follow from that layout:

1. A larger caller arena did not help, because the pointer slot was
   hardcoded, not derived from the allocation.
2. An 8,192-byte body fills slots `0..8191` by itself, so it collides with the
   pointer slot before parsing even starts. No arena size could fix this
   without a capacity-aware entry point.

This is a standard-library API limitation, not a compiler or runtime defect.

## 3. Fix

`stdlib/json.mink` now exposes:

```mink
fn json_parse_in(input: Str, arena: Ptr<Int>, slots: Int) -> Int
```

- the persistent write pointer is stored in slot `slots - 1`;
- the parse is refused if `slots < 2`, if the input itself would reach the
  pointer slot, or if the value tree would reach it;
- `json_parse(input, arena)` remains the backwards-compatible entry point and
  delegates with `slots = 8192`, preserving the historical behavior for every
  existing caller.

`hookbox` now allocates 256 KiB (32,768 slots), resets slot `32767` before each
parse, and calls `json_parse_in(body, arena, 32768)`.

The npm mirror `npm/mink/stdlib/json.mink` carries the identical change.

Note: this commit also lands the pending (uncommitted) Session 138 JSON arena
guard and serializer-base corrections in the same files. The capacity fix
depends on that guard, and the accompanying `tests/json.rs` cases were already
written for it; keeping them together makes one coherent, tested JSON arena
change rather than a mixed worktree.

## 4. Permanent regressions

`tests/json.rs` (68 tests total, all passing):

- `parse_8192_byte_document_with_caller_capacity` builds an exactly
  8,192-byte object, parses with `json_parse_in(..., 32768)` in a 256 KiB
  arena, and requires a byte-exact round trip.
- `capacity_aware_parse_refuses_document_larger_than_arena` passes an 8,192-byte
  document with only 512 slots and requires a refusal (`json_parse_in` returns
  0), so the guard cannot silently write past a smaller allocation.
- The existing `parse_rejects_document_that_would_reach_the_write_pointer`
  still passes, confirming `json_parse` keeps its 64 KiB behavior.

`benchmarks/m5/hookbox/tests/driver.py` now requires:

| Body | Expected |
| ---: | --- |
| 4,096 bytes | 201 + exact event bytes |
| 4,097 bytes | 201 + exact event bytes (was 400) |
| 8,192 bytes | 201 + exact event bytes (was 400) |
| 8,193 bytes | 413, no event |

Each accepted case compares the event file against the independent oracle, so
the boundary is verified end to end, not just by status code.

## 5. Verification

| Check | Result |
| --- | --- |
| `target/release/mink.exe check benchmarks/m5/hookbox/app/main.mink` | exit 0 |
| `cargo test --test json` | 68 passed, 0 failed |
| `python benchmarks/m5/hookbox/tests/driver.py --no-build` | 60/60, three consecutive runs, exit 0 |
| `python benchmarks/m5/repoaudit/tests/run_tests.py --no-build` | 50/50, exit 0 |
| `python benchmarks/m5/releasepack/tests/driver.py --no-build` | 53/53, exit 0 |

New artifact: `hookbox.exe`, 231,424 bytes, SHA-256
`a7c4be44a67f7f0dbb54372e4386fd6b862a842f3593d83e0a00add078a48e25`.
The suite's standalone case ran it outside the source tree.

## 6. Remaining boundaries

- Input bytes plus the parsed tree must fit below the final arena slot; the
  serializer then writes after the tree. `hookbox`'s 256 KiB arena accommodates
  an 8,192-byte body with margin, but this is not an unbounded parser.
- `json_parse` callers with the historical 64 KiB arena keep the old ~3.5 KiB
  practical limit by design.
- No Unicode path or universal UTF-8 guarantee was added.

## 7. Commit

`aeceee1` — `fix: make JSON arena capacity caller-selectable`.

Changed: `stdlib/json.mink`, `npm/mink/stdlib/json.mink`, `tests/json.rs`,
`benchmarks/m5/hookbox/app/main.mink`, `benchmarks/m5/hookbox/tests/driver.py`.
No compiler or runtime source changed. No push, tag or publish.
