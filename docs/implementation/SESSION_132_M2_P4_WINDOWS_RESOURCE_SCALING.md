# Session 132 — M2 P4: Windows resource/scaling validation

**Scope:** establish the current Windows x86_64 scaling envelope for large files,
large strings, JSON, HTTP bodies and sustained allocation churn. This was a
validation-first session: measure first, classify limits, and fix only a proven
defect whose repair is small and safe. Linux was not touched; the version stays
1.0.4.

**Outcome:** the runtime itself (heap growth, file I/O, allocation/free, handles)
held up through every workload tested, including 64 MiB file round-trips and a
58.8-second, 20,000,000-cycle steady-state run with flat RSS. JSON is the real
scaling hole: its documented 64 KiB arena cannot parse or serialize anywhere
near its advertised ~8000-byte input, and with a caller-enlarged arena it
silently corrupts parse and serialization results above roughly 4 KiB. The HTTP
receive loop had two boundary defects at its own advertised 4 MiB body size and
on the overflow path; both were fixed with a minimal change and a permanent
regression test.

## 1. Baseline: limits as they actually are

Read from source, not from older audit prose:

| Limit | Value | Source |
| --- | --- | --- |
| Heap `.bss` prefix | 4 MiB | `src/runtime/abi.rs`, `HEAP_BSS_BYTES` |
| Heap logical maximum | 1 GiB | `HEAP_MAX_BYTES` |
| Heap growth | lazily reserved, committed in 1 MiB chunks | `HEAP_GROWTH_RESERVE`, `HEAP_COMMIT_CHUNK`, `emit_alloc` |
| Simultaneously live allocations | 1024 slots (`E-R03` beyond) | `MAX_LIVE_ALLOCS` |
| Allocation alignment | 16 bytes | `ALLOC_ALIGNMENT` |
| stdio capture window | 8 MiB per stream, grows on demand (`E-R02` beyond) | `STDIO_CAPTURE_WINDOW_BYTES` |
| JSON arena | documented 64 KiB / ~8000-byte input; actual geometry is hardcoded (see §4) | `stdlib/json.mink` |
| HTTP body / receive budget | `MAX_BODY()` = 4 MiB; budget `MAX_BODY()+8 KiB` of whole response | `stdlib/http.mink` |

`rt_fs_read` uses `GetFileSize` (DWORD) and one `ReadFile` for the whole file;
`rt_fs_write` writes the string's length-bounded data. A missing file returns an
empty string, not an error.

Focused pre-probe tests (release, current source):

- `tests/runtime.rs` — 25 passed (includes `heap_grows_beyond_the_initial_arena`
  and `heap_exhaustion_is_e_ro2`).
- `tests/filesystem_lib.rs` — 46 passed.
- `tests/json.rs` — 63 passed.

`tests/http_lib.rs` was run after the receive-path repair (39 passed, including
the new boundary test in §9); its 38 pre-existing cases are unaffected by the
change.

## 2. Large-file stress

Deterministic fixture bytes `b[i] = (i*131 + 17) & 0xFF`, generated under
`tmp/m2_p4/`. Each case reads the file into a `Str`, verifies the exact length,
recomputes XOR and SUM over every byte, writes it back through `rt_fs_write`,
checks `rt_fs_file_size` of the output, and the runner compares SHA-256 of the
original and the MINK-written file.

| Size | read len | write len | XOR | SUM | SHA match |
| --- | --- | --- | --- | --- | --- |
| 1 MiB | 1048576 | 1048576 | 0 | 133693440 | yes |
| 4 MiB | 4194304 | 4194304 | 0 | 534773760 | yes |
| 8 MiB | 8388608 | 8388608 | 0 | 1069547520 | yes |
| 16 MiB | 16777216 | 16777216 | 0 | 2139095040 | yes |
| 32 MiB | 33554432 | 33554432 | 0 | 4278190080 | yes |
| 64 MiB | 67108864 | 67108864 | 0 | 8556380160 | yes |

All six sizes ran in one process in 1.7 s total, sequentially allocating and
freeing each string: no `E-R02`, no truncation, no corruption, no degradation.
This is the strongest positive result of the session — the Session 125 heap
growth works far beyond the old 4 MiB arena.

No architectural ceiling was reached. The only hard ceiling is the 1 GiB logical
heap (a single string plus its source must fit), reached deliberately in §3;
`rt_fs_read`'s 32-bit file-size path caps practical single-read files at 4 GiB−1
(not probeable in this timebox).

## 3. Growing strings / allocation / fragmentation

Three isolated workloads (each run in a fresh process, so an `E-R02` is
attributable) plus a combined run:

**A — pathological free order.** Per cycle: allocate 2 MiB, allocate 32 KiB,
free the 2 MiB block first, then the 32 KiB block. The next 2 MiB allocation
must walk past the small block at the free-list head, reuses the large block,
and drops the skipped small block. Net effect: 32 KiB of bump growth per cycle
(the documented first-fit behavior), bounded and not a leak.

| Cycles | Result |
| --- | --- |
| 1,000 / 20,000 / 30,000 / 31,000 / 31,500 / 32,000 | exit 0 |
| 33,000 | exit 102, `E-R02` |

Ceiling ≈ 32,700 cycles, exactly the 1 GiB heap divided by the 32 KiB/cycle
waste. Memory over 20,000 cycles: working set rose to ~800 MB (committed
growth), handles flat at 59. No `E-R06`, no corruption.

**B — varied small-block churn.** 40,000-160,000 allocations of 16-4016 bytes
with boundary writes, reads and length checks, freed each iteration.
5,000 and 20,000 cycles: exit 0, status 0. No degradation.

**C — growing string.** Each round starts at 8 bytes and doubles via
`rt_str_concat` 17 times to 1 MiB, verifying length and both boundary bytes at
every level, then frees the final string.

| Rounds | Result |
| --- | --- |
| 400 / 500 | exit 0 |
| 600 / 700 / 800 | exit 102, `E-R02` |

Memory during 400 rounds grows linearly (WS ~780 MB at exit; handles constant at
58). Root cause of the growth is free-list behavior, not ownership: when a
larger freed block is reused for a smaller allocation, the table records the
*requested* size, so the block's remainder is permanently lost, and skipped
head blocks are dropped. Every block is freed and no `E-R06`/`E-R04` fires; the
heap simply bumps ~1.8-2.1 MiB per round until `E-R02`.

**Combined A+B+C in one process** exhausted the heap inside C after A+B had
consumed most of it (`E-R02` at the concat line), confirming the ceiling is the
shared arena, not a per-workload counter.

**Classification: B (allocator fragmentation/design behavior), deliberate and
explicit.** The first-fit walk, skipped-block drop and non-splitting reuse are
the Session 93 design implemented identically in both emitters and the Rust
reference model. It is safe (no corruption, no leak, explicit `E-R02`) but not
documented as a capacity rule. Do not change the allocator in M2 P4; record the
behavior for M2/M3 and document it when allocator work is scheduled.

## 4. JSON scaling

Two configurations: the documented 64 KiB arena (`rt_alloc(65536)`), and a
caller-enlarged arena sized generously for the payload. Fixtures were compact
integer arrays; parse correctness was checked by array length and first/last
element, serialization by exact length and SHA-256.

Documented 64 KiB arena:

| Input | Parse | Serialize |
| --- | --- | --- |
| 1999 B | OK (n=527, last=526) | `E-R05` |
| 2999 B | OK (n=777, last=776) | `E-R05` |
| 3499 B | OK (n=902, last=901) | `E-R05` |
| 3998 B and above | `E-R05` during parse | — |

Caller-enlarged arena:

| Input | Parse | Serialize |
| --- | --- | --- |
| 4094 B | correct (n, first, last) | returns 2961 B for a 4094 B document — silent truncation |
| 6997 B | length correct, last element wrong (0 vs 803) | 1885 B, silent corruption |
| 7999 B | correct | length correct, bytes differ in the middle |
| 8191 B | correct | round-tripped byte-identically in this case |
| 16383 B - 1048573 B | returns 0 (parse failure) | — |

Serializer exactness was also verified byte-identical for payloads of 511, 1023
and 2047 bytes; the failure begins once serialized output or parsed values reach
the fixed arena offsets.

Root cause is baked-in arena geometry in `stdlib/json.mink`, not the runtime:

- the persistent write pointer is stored at slot **8191**, which overlaps parsed
  values as soon as `input_len + 1 + value_words` passes that slot;
- serialization always writes to slot **7168** with one `Int` slot per output
  byte (a 1024-byte output area), regardless of the caller's arena size;
- there is no arena-capacity parameter, so the library cannot detect overflow.

Consequences: with the documented 64 KiB arena, parse stops at ~3.5-4 KiB (not
~8000) with an explicit `E-R05`, and serialization fails at >1 KiB with
`E-R05`. With a larger arena, failures become **silent wrong data** (parse and
serialization both), which is category **D**. The documented `~8000-byte input`
claim in the library header is not supported by either measurement. No JSON
change was made: a correct fix needs a capacity parameter and/or a serialize-into-
`Str` path, which is a design change outside this session's rules.

## 5. HTTP body scaling

A deterministic loopback fixture (Python `http.server`, probe-only under
`tmp/m2_p4/`; no HTTP server subsystem was added to the project) served
`b[i] = (i*131+17) & 0xFF` bodies on `127.0.0.1`. `http_client_get` +
`http_body` were driven against 8 KiB, 64 KiB, 1 MiB, 3 MiB and 4 MiB bodies.

Before the fix:

| Served body | `http_last_error()` | Extracted body |
| --- | --- | --- |
| 8 KiB / 64 KiB / 1 MiB / 3 MiB | 0 | exact length, SHA-identical |
| 4 MiB | −4 `TOO_LARGE` | 4,190,208 B (4 KiB short) |

Raw response length for the 4 MiB case was 4,190,384 bytes with a 176-byte
header. Root cause: `http_recv_all` stopped after **1024 reads of at most 4096
bytes** — 4,194,304 bytes total, which also has to carry the headers. That cap
was left over from the 1 MiB `MAX_BODY` era and was not raised when Session 125
lifted `MAX_BODY` to 4 MiB. A complete, advertised-sized response was therefore
truncated and misreported.

Fixing the cap exposed a second defect on the overflow path: the last chunk can
push `total` past `max_size`, while writes are clipped at `max_size`; the result
copy then read `total` bytes from a `max_size`-capacity buffer and the process
died with `E-R09` (exit 109) instead of returning the documented `TOO_LARGE`
prefix.

After both fixes the same probe reports:

| Served body | `http_last_error()` | Extracted body |
| --- | --- | --- |
| 8 KiB / 64 KiB / 1 MiB / 3 MiB / 4 MiB | 0 | exact length, SHA-identical |
| 4 MiB + 1 byte | 0 | 4,194,305 B (inside the 8 KiB headroom) |
| 4 MiB + 16 KiB | −4 `TOO_LARGE` | 4,202,320 B pattern-correct prefix, no crash |

Effective ceiling: the receive budget is `MAX_BODY() + 8 KiB` of **whole
response** bytes, headers included. A 4 MiB body is fully supported; a response
whose body exceeds roughly `4 MiB + 8 KiB − header_len` (here ~4,202,320 body
bytes with 176 bytes of headers) is truncated at the budget and flagged
`TOO_LARGE`. Bodies between `MAX_BODY` and the budget are accepted with error 0
(measured: 4 MiB + 1 byte → error 0, body length 4,194,305). That is the precise
meaning of the budget and is now documented in the library.

## 6. Sustained execution

One representative steady-state workload (alloc 256 KiB + 4 KiB, concatenate
the 4 KiB twice, verify length and boundary bytes, free in reverse allocation
order) ran **20,000,000 cycles in 58.8 s**.

Process samples every ~100 ms for the whole run:

```
t=0.01  ws=3MB    private=4.5MB handles=54
t=8.92  ws=3MB    private=4.5MB handles=54
t=26.76 ws=3MB    private=4.5MB handles=54
t=44.65 ws=3MB    private=4.5MB handles=54
t=58.83 ws=3MB    private=4.5MB handles=54
```

No progressive memory growth, no handle growth, no stale allocations, output
`cycles=20000000 bad=0`, and throughput flat (~8.9 s per 3 M cycles). The
allocator's intended LIFO steady state is leak-free and stable over a minute of
continuous execution.

## 7. Defect classification

| Observation | Class | Root-cause class | Action |
| --- | --- | --- | --- |
| 1 GiB heap ceiling, 1024 live-allocation table, 8 MiB stdio window | A — expected resource limit | deliberate and documented | none |
| 32 KiB/cycle bump waste with a small block ahead of a reused large block; reused-block remainder lost | B — allocator fragmentation behavior (no leak, explicit `E-R02`) | deliberate but poorly documented | documented here; no code change |
| JSON parse/serialize corruption above ~4 KiB with an enlarged arena; `~8000-byte` claim unsupported | D — data corruption | accidental limitation that is a reliability defect | recorded, not fixed (needs design change) |
| HTTP 4 MiB response truncated by the 1024-chunk cap and reported `TOO_LARGE` | D — data truncation (explicit error, silent size loss) | accidental limitation that is a reliability defect | **fixed** |
| HTTP oversized response crashing with `E-R09` instead of returning the documented prefix | E — unexplained runtime failure until traced | reliability defect | **fixed** |

No `E-R06` (leak) or `E-R04` (invalid free) was observed in any workload.

## 8. Fixes

`stdlib/http.mink` (mirrored to `npm/mink/stdlib/http.mink`), all inside
`http_recv_all`:

1. Removed the `chunk_count < 1024` loop cap. Termination is already guaranteed
   by `total < max_size` (each open chunk adds at least one byte) and EOF
   turning `open` off, so the cap was redundant and truncated valid responses.
2. Clamped `total` to `max_size` before copying the exact-length result. The
   final chunk may overshoot the budget while the buffer's capacity is the
   budget; without the clamp the copy reads past the buffer (`E-R09`).
3. Updated the `MAX_BODY()` and `http_recv_all` comments to state the real
   contract: `MAX_BODY() + 8 KiB` of whole response, headers included.

No runtime, allocator, JSON, compiler, or Linux file was changed.

## 9. Tests

`tests/http_lib.rs` gained `h43_client_get_receives_full_max_body_and_reports_overflow`
(Windows-gated because a 4 MiB body plus the accumulation buffer and extracted
copy exceed the fixed 4 MiB Linux arena). It runs a loopback fixture in the test
process and checks:

- 4 MiB body: `http_last_error() == 0`, exact length, first/last byte;
- 4 MiB + 16 KiB: `http_last_error() == -4`, prefix body between `MAX_BODY` and
  the budget, pattern-correct boundaries.

Verified against the pre-fix library: the test fails (exit 106). With the fix it
passes, and the full `tests/http_lib.rs` suite is **39 passed, 0 failed**.
`tests/runtime.rs` (25), `tests/filesystem_lib.rs` (46) and `tests/json.rs` (63)
were rerun on the final tree and still pass.

## 10. Probe inventory (git-ignored, `tmp/m2_p4/`)

| Probe | Purpose |
| --- | --- |
| `gen_large.py` → `large_file.mink` | 1-64 MiB read/XOR/SUM/write/SHA cases |
| `alloc_stress.mink`, `gen_alloc.py` → `alloc_{a,b,c}_*.mink` | isolated fragmentation/ceiling workloads |
| `sustained.mink`, `sample.ps1` | 20 M-cycle steady-state run + RSS/handle sampling |
| `gen_json.py` → `json_scale.mink` | 4 KiB-1 MiB parse/serialize sweep with enlarged arenas |
| `gen_json_boundary.py` → `json_boundary.mink` | parse/serialize boundary sweep |
| `gen_json_parseonly.py` / `gen_json_doc_arena.py` | parse-only and documented-arena sweeps |
| `http_server.py`, `gen_http.py` → `http_scale.mink` | deterministic loopback body-scaling sweep |

## 11. Remaining evidence gaps

- No single file larger than 64 MiB was tested; the 1 GiB heap is the known
  ceiling, but the exact file-size failure path (including `GetFileSize`'s
  32-bit limit and failed `StrAlloc` mid-read) was not driven.
- Fragmentation ceilings are shape-dependent; the measurements give per-cycle
  waste for two specific patterns, not a free-list census or a general model.
- JSON has no fix and no boundary test in the suite; the ceiling table above is
  probe evidence only.
- HTTP was measured over loopback only, without TLS, chunked encoding, real
  network segmentation, or concurrent clients.
- Multi-threaded allocation/free stress and concurrent HTTP + file I/O were not
  measured.
- Stdio capture limits were validated in M2 P3 and were not re-measured here.

## 12. Repository state and audit

- Intended session changes: `stdlib/http.mink`, `npm/mink/stdlib/http.mink`,
  `tests/http_lib.rs`, and this document.
- All pre-existing modified and untracked files (Session 128/129/130/131 docs,
  the roadmap archive, `nl1.exe`, `nl1.mink`, `stdlib/httpserver.mink`,
  `npm/mink/stdlib/httpserver.mink`, `tests/httpserver_lib.rs`, the pasted chat
  transcript, and the earlier modified docs/source files) are untouched.
- Probe artifacts live under `tmp/m2_p4/` and are ignored
  (`git check-ignore` → `.gitignore:14:tmp/`).
- `Cargo.toml` and `npm/mink/package.json` are still **1.0.4**; tags are still
  `v1.0.0`–`v1.0.4`; no npm/release/publish/tag command was run; nothing was
  committed; no Linux file was touched.

## 13. Recommendation for M2 P5

The runtime envelope is healthy: heap growth, large file I/O, steady-state
allocation and handle counts are all solid on Windows. The next priority is the
JSON arena design, which is the only place where data can be silently wrong:

1. Give JSON an explicit arena-capacity contract (pass the slot count, or move
   serialization onto a heap `Str` with a size/recheck pass), return a documented
   error instead of corrupting, and extend tests through 8 KiB-256 KiB.
2. Document the allocator's non-splitting reuse and skipped-block drop as a
   capacity rule; only consider splitting/coalescing when an allocator session is
   explicitly scheduled.
3. For the next validation session, prefer a cross-subsystem stress run
   (files + HTTP + JSON + threads) over pushing any single size higher; the
   remaining risk is interaction, not one-shot capacity.
