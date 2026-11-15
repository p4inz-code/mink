# Session 281 — M7 Phase D: runtime memory management & allocator reliability

Date: 2026-11-15. Platform: Windows x86_64 (Linux frozen, untouched). Version
stays 1.0.5, unpublished; no tag, no release, no credential change. M6 remains
outstanding and separate; M8 remains gated.

Baseline: `5f73cd7`, local `main` equal to `origin/main`, four untracked user
files in the worktree (left untouched).

## Verdict

**Phase D: PASS.** The mixed-size allocator defect (M2-D1) is corrected at its
root cause, the original reproduction now completes 1,000,000 cycles (the
pre-fix limit was ~16,000), and free capacity is demonstrably preserved and
reused rather than merely "not crashing". `tests/zlib_lib.rs` passes unchanged,
a 20-case permanent conformance corpus passes (9 of its cases fail on the
pre-fix allocator), all seven M2 workloads were rerun and pass, and the debug
and release suites are 3,630 passed / 0 failed each. Two latent memory-safety /
ABI defects that the corrected allocator exposed were fixed at their own root
cause (stdlib zlib code-length indexing, emitted `process_run` register use).

## 1. Gate A — allocator architecture audit

### Arena layout

| Component | Where | Notes |
|-----------|-------|-------|
| `.bss` prefix arena | `BSS.arena` (offset 1488 in `.bss`) | 4 MiB (`HEAP_BSS_BYTES`), pre-committed |
| growth region | lazily `VirtualAlloc`-ed | reserved 1 020 MiB once, committed in 1 MiB steps (`HEAP_COMMIT_CHUNK`) |
| logical capacity | `HEAP_MAX_BYTES` = 1 GiB | `E-R02` when the bump cursor would cross it, or a commit fails |
| liveness table | `BSS.table`, 1,024 entries × 24 B = 24 KiB | `(start, size, live<<0 | generation<<32)` |
| free list head | `BSS.free_head` | absolute address, 0 = empty |
| bump cursor | `BSS.cursor` | **offset** from the arena base; blocks are absolute |

A block lives at `arena + offset` below 4 MiB and at `growth_base + (offset -
4 MiB)` above it. A bump that would straddle the boundary is relocated to the
start of the growth region, so no block ever spans the two mappings.

### Allocation / deallocation paths

`rt_alloc(size)` (`emit_alloc`, `src/backend/emit/runtime.rs`): take the
runtime spin lock → reject `size <= 0` (`E-R08`) → align to 16 → walk the free
list → reuse or bump → record a liveness slot → unlock. `rt_free(ptr)`
(`emit_free`): lock → image-literal no-op → reject null (`E-R04`), misaligned
(`E-R07`) → find the live slot whose start equals `ptr` (exactly one exists, or
`E-R04`) → mark dead, carry the 16-bit generation into the block → link the
block into the free list → unlock. `rt_str_alloc` is `rt_alloc(size + 8)` plus
a length prefix; `rt_str_free`, collections and every stdlib module funnel
through those two services. `rt_mem_load`/`rt_mem_store` validate against the
same table (8-byte words, 8-byte aligned, entirely inside one live block).

### Block metadata and the liveness table

A free block carries its own metadata in its first words: `[+0]` next link,
`[+8]` size, `[+16]` generation lineage (written only when the block is at
least 24 bytes, the minimum size that can hold a collection buffer). Live
blocks have no header — the liveness slot is the only record, and **interior
memory access is validated by that slot's `(start, size)` pair**. That is why
the allocator may not record anything other than the request in the slot.

### Why first-fit reuse lost capacity

Both implementations had the same two bugs on the reuse path:

1. **Unlink dropped the walk prefix.** The machine runtime unlinked the chosen
   block with `free_head = fit.next`, i.e. it discarded every free block it had
   walked over to reach the fit. The reference model did the equivalent with
   `free.truncate(i + 1); pop()`, discarding the other half of the list. A
   reordered (address-ordered) walk is not enough by itself: the predecessor
   must be tracked so the fit can be unlinked in place.
2. **The remainder was discarded.** The reused block was recorded at the
   *requested* size, so `[fit, fit+requested)` became live and
   `[fit+requested, fit+old)` belonged to neither a live block nor the free
   list. It was unreachable forever. For the 64 KiB → 1 KiB churn that is
   64 512 lost bytes per cycle: `1 GiB / 64 KiB ≈ 16 384` cycles, which is the
   reported `E-R02` at ~16,000. Measured on the pre-fix tree: 8,000 cycles
   exit 0, 20,000 and 100,000 exit 102.

The defect is therefore **lost free-list regions plus internal truncation**,
not ordinary external fragmentation: no block was ever double-allocated and no
access was ever mis-validated.

### Why the earlier prototypes produced `E-R05`

`tests/zlib_lib.rs` fails with `E-R05` (out-of-bounds access) whenever a live
slot's recorded size is *larger* than the memory the allocator actually gave
the caller. That is exactly what a "capacity recording" prototype does: it
keeps the reused block's original size in the slot so the whole block can be
returned later. Interior writes are then validated against the overstated
extent, the remainder gets handed to another allocation, and the first block
and the second overlap. The corruption surfaces later as an access outside
whichever block still owns the address — `E-R05` in the decoder. Recording
capacity is therefore not a viable direction: the slot size *is* the safety
boundary, and it must equal the request. Splitting with exact records changes
nothing about what the table can validate, which is why the final design keeps
`tests/zlib_lib.rs` byte-for-byte unchanged.

### Call sites and invariants (before the change)

Every heap user goes through `rt_alloc` / `rt_free` / `rt_str_*`: strings,
`Vec`/`Map`/`Set`, JSON, CSV, ZIP, zlib, SQLite, HTTP, process capture,
threads. The invariants the runtime relies on are: 16-byte alignment; sizes
positive multiples of 16; slot `start + size` inside `[0, cursor)`; no two live
slots sharing a start; free-list entries aligned, inside the cursor, not live,
unique; double/interior/misaligned frees rejected; exhaustion is `E-R02`.

## 2. Gate B — design selection

| Candidate | Allocation | Free | Fragmentation | Metadata | Verdict |
|---|---|---|---|---|---|
| Split + preserve remainder, keep LIFO list | O(n) | O(1) | mixed-size churn still grows: the 1 KiB sliver and the 63 KiB remainder are adjacent but never rejoin, so the next 64 KiB request cannot use them | none | Rejected — does not fix the reproducer |
| **Address-ordered list + split + immediate coalescing** | O(n) | O(n) | released neighbours merge, so a carved block returns to the block it came from; keeps the "no two adjacent free blocks" invariant | intrusive, no growth | **Chosen** |
| Capacity recording | O(n) | O(1) | n/a | none | Rejected — overstates the liveness extent (the `E-R05` mechanism above) |
| Best fit | O(n) | O(1) | slightly better leftovers than first fit, but still loses the remainder and never rejoins neighbours | none | Rejected — does not address the cause |
| Segregated size classes | O(1) | O(1) | internal waste up to a class step; arbitrary sizes (ZIP/JSON/SQLite allocate exact byte counts) still need a large-object path with splitting | per-class heads in `.bss` | Rejected — larger change, still needs the chosen design underneath |
| Boundary tags | O(1) neighbours | O(1) | best constant factors | +8 B per block, payload shifted, every offset assumption re-derived | Rejected — the address-ordered list already gives O(n) neighbour discovery with zero per-block overhead |

Chosen design, in one sentence: **keep the intrusive free list, order it by
address, split a reused block at exactly the requested size, and coalesce a
freed block with its address neighbours on insertion.** It is the smallest
change that makes released capacity reusable, it preserves the liveness-table
contract exactly, and it adds no metadata, no lock and no ABI field.

Coalescing is restricted to exact address adjacency (`left.start + size ==
right.start`), which also proves the two ranges are contiguous, so a merge can
never join blocks across a gap — including across the `.bss`/growth boundary,
which no block straddles.

## 3. Gate C — implementation

- `src/backend/emit/runtime.rs` — `emit_alloc`: walk with a predecessor
  register, unlink the fit in place (`prev.next` or `free_head`), split at the
  request and push the remainder as an independent free block. `emit_free`:
  insert in address order, coalesce with predecessor and successor. Both
  services keep their historical register footprint (alloc: `rax`, `rcx`,
  `rdx`, `r8`, `r9`; free: `rax`, `rcx`, `rdx`, `r8` plus frame spills), so
  behaviour that depended on untouched volatile registers is unchanged.
- `src/runtime/allocator.rs` — the reference model mirrors it exactly
  (address-ordered `Vec`, split on reuse, coalesce on free) so tests and
  documentation keep describing the machine code.
- `src/runtime/verify.rs` — three new invariants: **V-07** live blocks never
  overlap, **V-08** free blocks are aligned, in-range, non-overlapping with
  any live or free block, **V-09** the list is address-ordered with no two
  adjacent entries (the coalescing invariant).
- `src/runtime/abi.rs` — the ABI narrative now states the new policy.

Preserved: 16-byte alignment; slot size == request (interior validation
unchanged); exact-start frees (`E-R04`/`E-R07`); `E-R02`/`E-R03`/`E-R08` on the
same conditions; the generation-retirement rule; the single non-recursive spin
lock, taken by the same operations; intrusive metadata only (no growth).

### Defects found while implementing (fixed at their own root cause)

1. **`stdlib/zlib.mink` indexed a Huffman table with a rejected code length.**
   `_z_h_build` set `res = -5` when a length was outside `0..15` and then used
   that length as an index anyway (`_z_put(h, ln, _z_get(h, ln) + 1)`), reading
   and writing outside the 16-word count table. The old allocator hid it by
   handing out freshly zeroed memory; capacity-preserving reuse returns the
   previous owner's bytes, so a malformed stream reached the bad index and
   died with `E-R05`. Fixed by never indexing with an out-of-range length.
2. **`_z_block_dynamic` built tables from a length arena whose decode had
   failed**, reading words the failed decode never wrote — a result that
   depended on heap history. The tables and the decode are now skipped once an
   error is recorded, so the reported code is the first real error.
   Both are pinned by `tests/allocator_conformance.rs::
   ac14_zlib_malformed_stream_after_prior_decode` (the minimal two-call
   reproduction: `zlib_decompress("78da0300000000")` then
   `inflate_raw("050000000000000000")`).
3. **`rt_process_run` kept the command length in `r11` across `rt_alloc`.**
   `r11` is volatile under the runtime's own calling convention, so this worked
   only by accident; the split code uses `r11` for the remainder address, and
   the truncated command line made `cmd.exe /c` run an empty command (39 of 40
   captured runs produced nothing, `tests/resource_lifecycle.rs`). The length
   now lives in the frame, and the allocator no longer touches `r10`/`r11` at
   all — the fix is belt and braces because the call-site bug is real.
4. `npm/mink/stdlib/zlib.mink` re-synced with `stdlib/zlib.mink` (the bundle
   must be byte-identical; `tests/release.rs` enforces it).

## 4. Gate D — permanent corpus

`tests/allocator_conformance.rs`, 20 tests (19 default + 1 ignored 60 s soak).
Each program is compiled by the real CLI into a native PE and executed; a clean
exit also runs the leak check. Reuse is measured, not assumed: every block's
**arena offset** is computed from a marker allocated first (offset 0), and for
handles (strings, collections) the offset of a following probe allocation is
used instead.

| # | Case | Asserts |
|---|------|---------|
| ac01 | repeated 64 KiB / 1 KiB churn, 20,000 cycles | every cycle lands on offset 16: one 64 KiB block serves the whole run |
| ac01b | same churn, 200,000 cycles | growth stays under 512 KiB (only generation retirement); used to die at ~16,000 |
| ac02 | alternating 64 B / 4 KiB / 256 KiB, 200 rounds | round 200 uses the same three addresses as round 1 |
| ac03 | five sizes freed in a varied order | every re-request starts inside the released extent |
| ac04 | adjacent free blocks | an 8 KiB request reuses the coalesced pair; both ends writable |
| ac05 | non-adjacent free blocks | an 8 KiB request is *not* served from `a + c`; the live middle block is untouched |
| ac06 | partial split | the 64 KiB block is reusable after a 1 KiB carve; then two 32 KiB halves at `start` and `start+32768` |
| ac07 | nine size classes × 2,000 rounds | the high-water mark never rises after round 1 |
| ac08 | 40,000 deterministic pseudo-random alloc/free ops over 64 slots | bounded high-water, clean exit, a 64 KiB request still succeeds afterwards |
| ac09 | 512 MiB round trip, free, reuse + 2 GiB request | both ends intact, same address on reuse, `E-R02`/102 above the ceiling |
| ac10 | duplicate / interior / misaligned free | `E-R04`/104, `E-R04`/104, `E-R07`/107 |
| ac11 | interior word access, then one word past the end | interior round-trips; past-the-end is `E-R05`/105 |
| ac12 | 20,000 string alloc/mutate/free cycles | contents round-trip, probe stays low |
| ac13 | 5,000 collection churn cycles with 32 elements | values survive, probe stays low |
| ac14 | zlib round trip after 5,000 churn cycles | `e == 0`, payload identical |
| ac14b | zlib malformed streams after a prior decode | `-7`, `-5` — structured errors, no out-of-bounds access |
| ac14c | ZIP round trip after 5,000 churn cycles | entry count, status, payload |
| ac15 | SQLite create/insert/select after 5,000 churn cycles | row round-trips |
| ac16 | *ignored* 60 s mixed-size soak, 4 worker threads + main | joins, work on both sides, 0 mismatches, bounded high-water |

**The corpus is a real barrier:** against the pre-fix tree 9 of the 20 fail
(ac01, ac01b, ac02, ac03, ac04, ac06, ac07, ac12, ac13) and the run takes
29.8 s instead of 0.23 s — the time itself is the arena repeatedly growing.

## 5. Mixed-size allocation and soak results

Release compiler, Windows x86_64, this machine.

| Measurement | Result |
|---|---|
| `churn_repro.mink 8000` (pre-fix) | `ok`, exit 0 |
| `churn_repro.mink 20000` / `100000` (pre-fix) | exit 102, `E-R02` |
| `churn_repro.mink 20000` (post-fix) | `ok`, exit 0 |
| `churn_repro.mink 200000` (post-fix) | `ok`, exit 0 |
| `churn_repro.mink 1000000` (post-fix) | `ok`, exit 0, 0.040 s |
| 60 s mixed soak (5 threads, 16 B…64 KiB) | 687,919 main ops + 23,182,412 worker ops, 0 mismatches, high-water 4,105,552 B, exit 0, 60.00 s |
| 60 s soak, before the change | not runnable to 60 s: the same mix died with `E-R02` (see churn) |

The soak's high-water plateaus at ~3.9 MiB for ~23.9 M operations; the only
growth left is generation retirement (a block is retired after 65,536 reuses,
one block per retirement, documented in Session 204), which the 200,000-cycle
case bounds explicitly.

### Practical comparison (same machine, same session, 10 s window)

| Runtime | `m2_soak` loops | thread lifecycles | exit |
|---|---|---|---|
| pre-fix (`5f73cd7`) | 219,352 | 21,936 | 0 |
| post-fix | 1,512,201 | 151,222 | 0 |

6.9× more work in the same wall clock. The pre-fix figure reproduces the M2
record (238,152 / 23,816) within machine noise, so the comparison is
apples-to-apples. This is reported as a measurement of this workload on this
machine, not as a general performance claim.

## 6. zlib and interior-pointer compatibility

- `cargo test --test zlib_lib`: **15 passed, 0 failed** with the test file
  unchanged (no line of `tests/zlib_lib.rs` was edited).
- `cargo test --test capacity`: 9 passed, including
  `cap08_single_size_churn_reuses_the_free_list` and the 65,536-reuse
  generation-retirement case.
- Interior access: `ac11` writes and reads at offsets 0, 512 and 1016 of a
  1 KiB block, reuses the block for a 32-byte request, and confirms that one
  word past a live block is `E-R05`. The liveness table is unchanged in size
  (1,024 × 24 B) and in meaning (slot size == request), so the validation that
  `zlib_lib` depends on is untouched.
- `ac14b` is the minimal reproduction of the defect that the old allocator
  masked; it now reports the structured errors.

## 7. M2 workload revalidation (release compiler)

| Workload | Command | Result |
|---|---|---|
| big data / files / stdin / JSON | `mink build …/m2_bigio/main.mink`; `python …/verify.py` | **25/25** cases match the Python reference (24.2 s) |
| process automation | `python …/m2_procauto/verify.py` | **8/8** (1.0 s) |
| networking under load | `python …/m2_net/verify.py` | **11/11** (2.9 s) |
| HTTP server, 16 clients | `python …/m2_server/verify.py` | **PASS** (1.2 s) |
| threads | `main.exe contend 12 5000` → `threads=12 counter=60000 returned=60000`; `cycles 100 7` → `counter=700`; `failure 8` → `joined=8 seven=8` | all invariants hold, exit 0 |
| soak | `main.exe 10 soak_work` | exit 0, 1,512,201 loops, 151,222 thread lifecycles, no leak report |
| integrated SQL + JSON + threads | `python …/m2_integrated/verify.py` | **PASS** (100 rows, 100 records, totals match) |

The original `validation/m2_bigio/churn_repro.mink` was run unchanged; its
comment block still documents the pre-fix behaviour, which is now historical.

## 8. Debug / release regression, acceptance

| Check | Command | Result |
|---|---|---|
| debug | `cargo test --all-targets --no-fail-fast` | **99 binaries, 3,630 passed, 0 failed, 10 ignored** |
| release | `cargo test --release --all-targets --no-fail-fast` | **99 binaries, 3,630 passed, 0 failed, 10 ignored** |
| release acceptance | `python tools/release_acceptance.py` | see §12 (run on the committed tree) |

3,610 of those tests are the pre-existing suite (identical to the M2 baseline)
and 20 are the new corpus; the ignored count went 9 → 10 because of the 60 s
soak, which is opt-in.

## 9. Formatting and Clippy

- `cargo fmt --check`: the tree is **not** rustfmt-clean at the baseline either
  (15 pre-existing files, including `tests/http_lib.rs`, `tests/network_lib.rs`
  and unrelated hunks in `src/runtime/abi.rs`, `src/backend/emit/runtime.rs`
  and `src/runtime/allocator.rs`). Every region touched by this session is
  formatted (`tests/allocator_conformance.rs` was run through `rustfmt`;
  `src/runtime/verify.rs` produced no diff). No unrelated file was reformatted.
- `cargo clippy --all-targets`: **0 errors, 116 warnings**, exactly the
  pre-fix count measured on the stashed tree (the pre-existing
  `unused variable: offsets` family and dead-code notes). No new warning, no
  `allow` added.

## 10. Remaining limitations and risks

1. **Generation retirement grows the arena.** A block reused 65,536 times is
   retired rather than reused, to keep the 16-bit handle generation from
   wrapping (Session 204). Under sustained churn this costs one block per
   65,536 reuses — bounded, but not zero. The 200,000-cycle test pins the bound.
2. **O(n) free-list walks.** Allocation and free walk the list. The list stays
   short because adjacent blocks coalesce, but a workload with many
   simultaneously free, non-adjacent blocks pays proportionally. No measurement
   in this session showed it as a bottleneck (the soak improved by 6.9×).
3. **First fit, not best fit.** A large block can be split for a small request,
   leaving a smaller remainder; that remainder is reusable and coalesces back
   with its neighbours, but a pathological order can still fragment more than a
   best-fit policy would. No test showed a capacity failure from this.
4. **1 GiB ceiling and 1,024 live allocations are unchanged.** They are
   intentional, documented limits: `E-R02`/`E-R03` remain loud and
   deterministic.
5. **No asynchronous or system-level allocator.** The heap is still a fixed
   arena with one spin lock; threads serialize on it. Fine for the measured
   workloads, not a scalability claim.
6. **Linux runtime untouched.** Its emitter (`emit/linux_runtime.rs`) was not
   modified; the Linux runtime is frozen and unsupported.

## 11. What was *not* done

No version bump, no npm publication, no credential change, no tag, no GitHub
release, no Linux work, no unrelated feature work. M6 external developer
validation remains outstanding; M8 remains gated.

## 12. Commits and remote verification

Four commits on top of `5f73cd7`, staged file by file (never `git add .`):

| Commit | Content |
|---|---|
| `0e7d348` | `fix(runtime): preserve freed heap capacity when reusing a block` — split/coalesce allocator, reference model, verifier invariants, ABI narrative, `process_run` register spill |
| `482a9ab` | `fix(stdlib): never index a Huffman table with a rejected code length` — zlib bounds, deterministic malformed-stream handling, npm bundle re-sync |
| `1b0fde1` | `test(allocator): add the mixed-size allocator conformance corpus` — the 20 executed cases |
| `6f8e6cf` | `docs(phase-d): record the allocator audit, fix and revalidation evidence` — this report, roadmap status, per-workload records, M2 defect closure |

`python tools/release_acceptance.py` on `6f8e6cf` (clean checkout of the
tracked tree): **verdict PASS**, exit 0, report
`target/acceptance/unlabelled/20261115T113207Z-6f8e6cf/report.md`, regression
**100 suites, 3,630 passed, 0 failed, 10 ignored**, one limitation — the four
pre-existing untracked user files, which are deliberately not part of the
acceptance evidence. (Before committing, the same run reported PARTIAL for the
only reason it should: tracked files differed from the committed tree.)

Local `main` equals `origin/main` after the push; both resolve to the final
commit on this branch.

## 13. Roadmap position

M7 Phase D: **PASS**. M2's single open runtime defect (M2-D1) is closed and all
seven M2 workloads were revalidated against the modified runtime, so the
milestone's documented reason for PARTIAL no longer applies; the M2 record is
annotated accordingly. M6 remains **OUTSTANDING** and separate, M8 remains
**gated**, Linux stays frozen, 1.0.5 stays unpublished.
