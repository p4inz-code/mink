# Session 204 — Runtime Capacity, Allocation Limits and Handle Exhaustion

Windows x86_64, 2026-11-07. Starting commit `a553a78` (Session 203, local ==
origin/main verified before any change).

## 1. Objective

Inspect the three reported runtime capacity limits — the 1 GiB no-coalesce
arena, `MAX_LIVE_ALLOCS = 1024`, and the 16-bit collection-handle generation —
establish their real boundary behaviour, and correct any confirmed defect
without blindly raising constants or changing the handle ABI.

## 2. Files inspected

- `src/runtime/abi.rs`: `HEAP_MAX_BYTES`, `HEAP_BSS_BYTES`,
  `HEAP_GROWTH_RESERVE`, `MAX_LIVE_ALLOCS`, `LIVE_ENTRY_BYTES`,
  `HANDLE_GENERATION_SHIFT`, `HANDLE_GENERATION_BITS`.
- `src/backend/emit/runtime.rs`: `emit_alloc` (bump, LIFO reuse, growth,
  table scan), `emit_free` (lineage, free-list link), `emit_str_alloc`,
  `emit_require_live_handle`, `emit_tag_collection_handle`,
  `emit_unpack_handle_for_free`.
- `src/runtime/allocator.rs` and `src/runtime/verify.rs`: the reference model
  and its ABI consistency checks.
- `tests/collections_lib.rs` (Session 197 generation tests), `tests/soak.rs`,
  `tests/resource_lifecycle.rs`.
- Win32 `VirtualAlloc` growth path and `E-R02`/`E-R03` reporting.

## 3. Findings

### 3.1 Arena ceiling (1 GiB, no coalescing)

Intentional and safe. The arena is a fixed logical space of 1 GiB; the first
`.bss` prefix is mapped by the image, anything beyond is reserved lazily and
committed in chunks. A request whose aligned end exceeds `HEAP_MAX_BYTES` jumps
to `E-R02` (102) **before** any commit, so the failure is immediate and does
not touch memory. Alignment cannot wrap into a small successful allocation:
`i64::MAX + 15` becomes `0x8000_0000_0000_0000`, which is above the ceiling as
an unsigned value, so it is rejected. The free list is LIFO and does not
coalesce, so a workload that alternates large and small blocks can exhaust the
arena early; that is the documented architectural ceiling, not an accounting
bug. Session 199's 342-iteration failure at 64 KiB payloads remains the
boundary evidence; this session did not reproduce it and instead tests the
failure path with a request above the ceiling.

### 3.2 Live allocation table (`MAX_LIVE_ALLOCS = 1024`)

Intentional ABI limit: 1,024 slots × 24 bytes = 24 KiB of `.bss`, sized for
1,000-entry `Vec<Str>`/`Map<Str, V>` workloads. Allocation scans for the first
dead slot; with all slots live the 1,025th allocation is `E-R03` (103). The
failure happens before the new block is linked into the table, and the process
exits through `rt_fail`, so no live metadata can be corrupted or half-written.
Freeing a slot and allocating again reuses it. No change was made.

### 3.3 Generation exhaustion (16 bits) — confirmed defect

A collection handle encodes `generation << 48`; the table entry stores the
generation at bits 32..47. Each reuse of a block increments its lineage word
(`block + 16`) and masks it to 16 bits. After 65,536 reuses the generation
wraps through 0 and back to 1, so a stale handle issued for the block's first
occupant compares equal to the new occupant's generation. The stale handle is
then accepted as live: **ABA aliasing**.

Red check with retirement disabled (old behaviour):

```
thread 'cap07_generation_exhaustion_retires_the_slot' panicked at
tests\capacity.rs:271:5:
a stale handle must stay stale after generation exhaustion, got 110
stderr="mink: runtime error[E-R10]: array index out of range ..."
  left: 110
 right: 105
```

The stale handle passed the liveness/generation check (`E-R05` would be 105)
and only failed later with an out-of-bounds index against the new empty
vector — proof that it aliased the replacement.

## 4. Fix

`src/backend/emit/runtime.rs`, `emit_free`:

- after writing the block's lineage, a block whose generation is `0xFFFF` is
  **retired**: its table entry is marked dead and it is never linked into the
  free list. Generation 0 is therefore never issued, and the block's address
  can never be handed out again in the process;
- blocks below 24 bytes keep the plain free-list path. They cannot carry a
  collection handle (a collection buffer's header starts at 24 bytes), so
  retirement is unnecessary and their lineage is never read;
- cost: one 16-byte-aligned block is abandoned per 65,536 reuses of that
  block. Retirement does not touch the live count (the entry is marked dead)
  and does not change the handle encoding or the ABI.

`HANDLE_GENERATION_BITS` stays 16; widening the tag would change the handle
representation and is not needed once wraparound is made impossible.

## 5. Tests added

`tests/capacity.rs` (7 tests, native PE built by the real CLI; clean exits run
the leak check):

| Test | Asserts |
| --- | --- |
| `cap01_request_above_arena_ceiling_is_er02` | 2 GiB request → `E-R02`/102 before committing |
| `cap02_huge_size_does_not_wrap` | `i64::MAX` request → `E-R02`/102, no wrapped small allocation |
| `cap03_eight_mib_block_roundtrip_and_reuse` | 8 MiB block, first/last byte writes, free, reuse |
| `cap04_live_table_accepts_1024` | recursive chain holds 1,024 live blocks, frees all, exit 0 |
| `cap05_1025th_live_allocation_is_er03` | 1,025th live block → `E-R03`/103 |
| `cap06_slot_reuse_at_the_limit` | 1,023 live + allocate/free/allocate at the limit, exit 0 |
| `cap07_generation_exhaustion_retires_the_slot` | 65,535 reuse cycles, stale first handle → `E-R05`/105 |

## 6. Verification

| Check | Result |
| --- | --- |
| `cargo test --test capacity` (debug) | **7 passed, 0 failed** (0.05 s) |
| `cargo test --release --test capacity` | **7 passed, 0 failed** (0.05 s) |
| Red check (`cap07`, retirement disabled) | **failed as expected**: exit 110, stale handle accepted |
| Session 197 generation test (70,000 valid cycles) | still passes inside the full suite |
| Full regression, every target except `tls_lib` | **81 suites, 3,231 passed, 0 failed, 7 ignored** |
| Baseline reconciliation | 3,224 + 7 = 3,231; 80 + `capacity` = 81 suites |
| `tls_lib` | not runnable here (`python3` is the Microsoft Store alias stub) |
| `rustfmt --check tests/capacity.rs` | clean |
| `rustfmt --check src/backend/emit/runtime.rs` | one pre-existing diff at line 380 (net emitter, untouched) |

Resource measurements: the largest test allocation is 8 MiB (written at both
ends); the table test holds exactly 1,024 live blocks and frees them all; the
generation test performs 65,535 reuse cycles in a bounded run (~0.05 s for the
suite). Retirement overhead is bounded by one abandoned block per 65,536
reuses; no unbounded table growth or memory overhead was introduced.

## 7. Known limitations

- The 1 GiB ceiling and the no-coalesce free list are unchanged and intentional;
  fragmentation-induced early `E-R02` remains possible for alternating large
  and small workloads.
- `MAX_LIVE_ALLOCS` remains 1,024; `E-R03` is fatal (the process exits), so
  there is no post-failure recovery path by design.
- Retired blocks are dead arena space for the rest of the process. With normal
  workloads this is one block per 65,536 reuses of the same block.
- No changelog file exists in this repository; the roadmap block, the
  readiness-audit addendum, the runtime document and this record are the
  change log.

## 8. Documentation

- `src/runtime/abi.rs`: generation-retirement note on `HANDLE_GENERATION_BITS`.
- `docs/implementation/RUNTIME_IMPLEMENTATION.md` §2: retirement replaces the
  65,536-reuse alias window.
- `docs/language/STANDARD_LIBRARY_REFERENCE.md` §5.2/§23: stale-handle limit
  now permanently closed, cost documented.
- `docs/roadmap/IMPLEMENTATION_ROADMAP.md`: dated Session 204 status.
- `docs/audits/SYSTEMS_READINESS_AUDIT.md`: dated Session 204 addendum.

## 9. Verdict

**PASS — one confirmed ABA defect fixed; the two capacity ceilings are
intentional, documented and fail deterministically.** Generation 0 is never
issued, retired blocks are never reused, and stale handles stay `E-R05`
permanently. Seven permanent boundary tests pass in debug and release, and the
full regression is 3,231 passed / 0 failed / 7 ignored across 81 suites.
