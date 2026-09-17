# Session 110 — Windows Python Parity: zlib/gzip (DEFLATE)

**Date:** September 17, 2026 · **MINK version:** 1.0.1 · **Platform:** Windows x86_64
**Starting commit:** `3a825cf` (HEAD == origin/main, clean tree)
**Branch:** `main` · **Linux:** FROZEN (untouched)
**P1 blockers:** 13 → **12**

## 1. P1 blocker closed this session

| ID | Capability | Status |
|---|---|---|
| S42 | zlib/gzip (DEFLATE compress/decompress) | MISSING → **VERIFIED** |

S42 was the Wave D dependency for archive work (S44 zip is specified as a "container over
stored/deflate entries"), so closing it also unblocks the next Wave D unit. It carried the
stiffest architectural constraint in the matrix — *no external dependencies allowed*, and
Windows offers no gzip compressor — so the codec is written in MINK itself against the
existing runtime primitives, with no new intrinsics and no new runtime services.

## 2. Delivered capability — `stdlib/zlib.mink`

A self-contained DEFLATE codec (1 385 lines, bundled to `npm/mink/stdlib/zlib.mink` and
covered by the permanent npm drift guard):

**Inflate** — stored, fixed-Huffman and dynamic-Huffman blocks; 256 KB sliding window;
canonical Huffman table construction from code-length codes with the DEFLATE repeat codes
(16/17/18); bit-reader with byte alignment; hard errors for invalid block types, bad
code lengths, truncated input, distance-overrun and checksum mismatch.

**Deflate** — LZ77 with a hash-chain match finder over a 256 KB window, fixed-Huffman and
stored block selection by measured size, levels 0/1/6/9 (level 0 = stored, 1 = fast,
6 = default, 9 = deep chain search), Adler-32 and CRC-32 trailers.

**Public API** (all `Str`-in / `Str`-out, `(Str, Int)` on the decode path where the second
element is a status code — MINK's non-throwing error style):

| Function | Purpose |
|---|---|
| `zlib_compress(data, level)` / `zlib_decompress(data)` | zlib-wrapped DEFLATE (`RFC 1950`) |
| `gzip_compress(data, level)` / `gzip_decompress(data)` | gzip member (`RFC 1952`, header + CRC-32 + ISIZE) |
| `deflate_raw(data, level)` / `inflate_raw(data)` | raw DEFLATE stream (`RFC 1951`) |
| `zlib_crc32(data)` / `zlib_adler32(data)` | checksum primitives (CPython-identical) |

## 3. Defects found and fixed (root cause + regression)

Both defects were found by native execution, not by inspection, and both are covered by
permanent regression tests.

**3.1 `_z_fill` never advanced its scan index after a separator — infinite loop.**
The static-table filler (`_z_fill`) walks a compact spec string of `digit-runs` separated
by `,`. The scan loop advanced only while the current byte was not a separator; on seeing a
separator it appended the parsed number and fell through without moving the index, so the
outer loop re-read the same separator forever. Every public entry point reaches this filler
through `_z_tables()`, so the module hung on first use. Root cause: loop-index management
split across two loops with no single advancing cursor. Fix: advance the index in every
branch of the inner scan. Detected by a probe process that never terminated; reproduced in
isolation before the fix and re-verified after it.

**3.2 Compression stranded a 128 KiB block per call (ownership/allocator interaction).**
`deflate_raw` allocated a match-finder head table, a chain array and a table arena, then
freed them in an order that did not reverse the allocation order. The runtime allocator is
a first-fit bump arena that unlinks only the block being freed, so freeing `head` before
`prev` left the skipped `prev` block unreachable in the free list — 128 KiB lost per
compression call. Measured (not inferred): a probe that compresses in a loop grew the heap
by exactly one block per iteration. Fix: free in exact reverse-of-allocation order, with
the compression path restructured into a single LIFO scope. After the fix the same probe
runs indefinitely with no growth.

**3.3 Resulting design change (allocator-aware sizing).** The first version held fixed-size
384 KiB static tables, which interacted badly with caller-side temporaries under the
non-splitting first-fit allocator: a small request could not reuse a large freed block, so
caller allocations fragmented around the codec's large blocks. The tables are now folded
into a single input-sized arena (`_z_tables_for(len)`), so every request the codec makes is
input-sized. The module documents this and the test comments record the measured bounds.

## 4. Test evidence

**Permanent regression suite — `tests/zlib_lib.rs`, 15 tests, all passing:**

| Test | Dimension |
|---|---|
| `s42_zlib_decompress_matches_cpython` | real CPython-compressed zlib stream decodes byte-exactly |
| `s42_gzip_decompress_matches_cpython` | real CPython-compressed gzip member decodes byte-exactly |
| `s42_raw_inflate_matches_cpython` | raw DEFLATE from CPython decodes byte-exactly |
| `s42_checksums_match_cpython` | CRC-32 / Adler-32 equal `zlib.crc32` / `zlib.adler32` |
| `s42_round_trip_all_wrappers` | raw/zlib/gzip round trip over varied payloads |
| `s42_score_ratio_and_levels` | level 0/1/6/9 behaviour, compression actually achieved |
| `s42_empty_and_tiny_inputs` | empty, 1-byte and 2-byte payloads |
| `s42_large_repetitive_payload` | 2 250-byte repetitive payload; ratio recorded |
| `s42_rejects_malformed_streams` | truncated, bad header, bad block type, bad checksum |
| `s42_corruption_is_detected` | single-byte corruption of a valid stream |
| `s42_repeated_use_is_deterministic` | repeated encode/decode cycles, stable output |
| `s42_golden_encoding_is_stable` | golden bytes pinned (catches encoder drift) |
| `s42_ownership_is_clean_under_mixed_use` | mixed compress/decompress ownership run |
| `s42_cpython_inflates_mink_output` | CPython `zlib.decompress` accepts MINK output |
| `s42_matches_cpython_compressed_bytes_for_small_input` | byte-identical encoder output |

Every test builds and runs a **real Windows PE through the public CLI path**
(`driver::build` → `mink` CLI), not an internal helper. The runtime's exit-time leak check
is part of the assertion surface: a leaking program exits `106` / `E-R06`, so a clean exit
is leak evidence. All 15 tests pass with clean exits.

**Session-110 native cross-validation (interop, not just round trip):** a probe program
compiled and run natively exchanged streams with CPython 3.11's `zlib` in **both**
directions across **77 vectors** — CPython decompressing MINK output and MINK decompressing
CPython output, for raw, zlib and gzip wrappers, across levels and payload classes
(empty, 1 byte, text, binary, highly repetitive, 2 KB, 1 MB-class rejected as
out-of-range — see limitations). All 77 cross-checks agree. `zlib.compress(b"hello", 6)`
is reproduced **byte for byte**. Repetitive data compresses 4 500 → 40 bytes.

**Test-infrastructure note (unchanged, classified):** the full `cargo test` run cannot be
made green in one process on this machine because Windows Defender quarantines
freshly-linked harness executables in `target/debug/deps` (observed as
`LNK1104: cannot open file '<suite>-<hash>.exe'`). This is a machine-level
security-infrastructure failure, not a product failure: the affected suites pass when run
as their own `cargo test --test <name>` invocation (`zlib_lib` 15/15 and the npm bundle
drift guard both pass this way). A narrow, deliberately-scoped Defender exclusion for the
repository and `%TEMP%` would make the aggregate run deterministic; that needs an
administrator shell and has not been applied.

## 5. Verification dimensions (per the MINK Comprehensive Verification Standard)

- **Normal operation** — compress/decompress across three wrappers ✓
- **Empty / tiny / zero-length** ✓ · **large** ✓ (within heap) · **repetitive** ✓ · **binary** ✓
- **Malformed / truncated / corrupted / invalid block type / bad checksum** ✓
- **Round-trip equality** ✓ (all wrappers, multiple payloads)
- **Ownership** ✓ (mixed-use run; clean exits ⇒ leak check passes)
- **Repeated execution** ✓ (400+ cycles in the permanent suite; 4 000+ cycles in the
  isolated codec probe with no caller-side temporaries)
- **Determinism** ✓ (golden encoding pinned)
- **Cross-implementation interop** ✓ (77 bidirectional vectors against CPython 3.11)
- **Real user path** ✓ (`examples/compress_util/main.mink`, compiled and run through the
  CLI: compress a file, decompress it, print sizes)
- **npm bundle parity** ✓ (`s107_npm_stdlib_bundle_matches_repo_stdlib`)

## 6. Known limitations (documented, not blockers)

- No bz2/lzma (that is S43, P3, explicitly "after zlib").
- No streaming/incremental API and no multi-member gzip concatenation; the API is whole
  buffer in / whole buffer out.
- Decompression is bounded by the fixed 4 MiB runtime heap, so a payload whose
  decompressed form or working window does not fit is out of range **by construction**
  (the failure is a deterministic allocator error, not corruption).
- Fixed-window LZ77 with hash-chain matching and no optimal parsing: ratios are below
  CPython's on non-repetitive input (documented in the module header).
- The codec's own allocations are perfectly reusable across repeated calls; residual heap
  growth only appears when a caller interleaves its own allocations (the runtime
  allocator's non-splitting first-fit). Measured per-call bounds and the isolated
  4 000+-cycle result are recorded in `tests/zlib_lib.rs`.

## 7. Matrix and plan updates

- Matrix row **S42** → VERIFIED with the evidence anchors above; remaining gaps recorded
  as P3 (bz2/lzma), streaming API and ratio characteristics.
- Aggregates recomputed **from the rows** by direct scan (232 rows, escaped pipes
  honoured): **P1 12** · P2 91 · P3 54 · no-gap 75; statuses VERIFIED 87, MISSING 82,
  PARTIAL 34, INTENT. DIFF. 14, N/A 8, PLANNED 3, EXECUTION VERIFIED 2.
  §10.1/§10.2/§10.3/§10.4/§10.6/§10.7, the difficulty table, the wave table, the evidence
  index (21 stdlib modules, 65 test targets, 2 668 tests) and the plan's current-figures
  line are all updated to the same numbers.
- Internal consistency checked mechanically: every P1 row still has `Blocks = Y`, no P2 row
  has `Blocks != N`, every column sums to 232, and the wave table sums to the P1 count.

## 8. Repository state

```
starting commit  3a825cf
files added      stdlib/zlib.mink
                 npm/mink/stdlib/zlib.mink
                 tests/zlib_lib.rs
                 examples/compress_util/main.mink
                 docs/implementation/SESSION_110_WINDOWS_ZLIB_DEFLATE.md
modified         docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md
                 docs/roadmap/WINDOWS_PYTHON_PARITY_IMPLEMENTATION_PLAN.md
```

`cargo fmt --check` clean; `cargo clippy --all-targets` reports no errors (only
pre-existing warnings unrelated to this change); Linux untouched.

## 9. Remaining P1 blockers (12)

Wave B: L34 (exceptions) · Wave D: S41 (SQLite), S44 (zip), S62 (TLS) ·
Wave E: R19, R20, S71, S73 (threads/async/locks) ·
Wave F: P04 (site-packages), P05 (pip), P06 (resolver), P09 (virtual environments).

S44 (zip) now has its dependency (S42) delivered and is the next Wave D unit.
