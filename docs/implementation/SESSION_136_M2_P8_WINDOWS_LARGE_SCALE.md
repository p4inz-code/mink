# Session 136 — M2 P8: Windows large-scale and boundary stress validation

**Scope:** close the practical Windows scaling-evidence gaps left by P4/P7 —
large files, large strings and allocation churn, allocator fragmentation,
large JSON, large HTTP bodies, large process output, a sustained mixed
workload, and resource stability. Windows x86_64 only, Linux frozen, version
1.0.4, no npm/release/tag activity. This was a validation-first session:
measure, classify every ceiling, and change product code only for a proven
defect.

**Outcome:** the runtime envelope is stronger than the previous records
suggested and every ceiling is loud. Files round-trip at 64 MiB with matching
SHA-256; a 5,000,000-cycle allocation/test/free loop runs with flat 4.5 MiB
private memory and 44 handles; HTTP bodies are byte-exact through the full
advertised 4 MiB; process capture payloads are byte-exact up to 8,388,600
bytes (8 MiB window minus its 8-byte length prefix) with `E-R02` above.
Three findings need follow-up:

1. **SQLite misparses heap-built statements** (genuine defect, reproduced and
   minimized): `sql_exec(db, "INSERT ...")` with a literal works;
   `sql_exec(db, heap_stmt)` with byte-identical text returns
   `ERROR: unsupported statement`. Root cause not yet established; the
   minimized reproducer is recorded below.
2. **HTTP still reports a truncated non-empty body as a completed request**
   (P7's finding, now reproduced deterministically: `Content-Length: 524288`
   with 200,000 bytes sent → status 200, body 200,000, `http_last_error() == 0`).
   The smallest safe fix is determined but not applied (see §11).
3. The sustained mixed workload is correct for **10,201 iterations** and then
   hits the 1 GiB heap with an explicit `E-R02` (allocator fragmentation at
   ~100 KiB of net growth per iteration of this shape).

No product code, test, or runtime file was changed in this session.

## 1. Baseline: the limits as they actually are

Read from source before probing (P4's table re-verified; the stdio and HTTP
numbers are the ones P8 needed):

| Limit | Value | Source |
| --- | --- | --- |
| Heap `.bss` prefix | 4 MiB | `src/runtime/abi.rs` `HEAP_BSS_BYTES` |
| Heap logical maximum | 1 GiB | `HEAP_MAX_BYTES` |
| Heap growth | lazily reserved, committed in 1 MiB chunks | `HEAP_GROWTH_RESERVE`, `HEAP_COMMIT_CHUNK` |
| Simultaneously live allocations | 1024 slots (`E-R03` beyond) | `MAX_LIVE_ALLOCS` |
| Allocation alignment / free-list | 16 B, LIFO first-fit, no split/coalesce | `ALLOC_ALIGNMENT`, `src/runtime/allocator.rs` |
| stdio capture window | 8 MiB per stream, three streams | `STDIO_CAPTURE_WINDOW_BYTES` |
| stdio usable payload | window − 8 B length prefix = **8,388,600 B** | `STDIO_WINDOW_DATA` in `src/backend/emit/runtime.rs` |
| HTTP body / budget | `MAX_BODY()` = 4 MiB; whole response budget `MAX_BODY()+8 KiB` = 4,202,496 B | `stdlib/http.mink` |
| JSON arena | documented 64 KiB; persistent write pointer at slot 8191; serializer output area at slot 7168 | `stdlib/json.mink` |
| `rt_fs_read` | `GetFileSize` (DWORD) + one `ReadFile`; missing file → empty string | `src/backend/emit/runtime.rs` |
| SQLite | one-owner blob, whole-blob copy per statement, whole-file rewrite on persist, no fsync | `stdlib/sqlite.mink` (P6/P7) |

Focused baseline suites (debug tree, before any probing):

| Command | Result |
| --- | --- |
| `cargo test --test runtime` | 25 passed |
| `cargo test --test strings` | 63 passed |
| `cargo test --test filesystem_lib` | 46 passed |
| `cargo test --test json` | 63 passed |
| `cargo test --test process_lib` | 86 passed |
| `cargo test --test http_lib` | 40 passed |
| `cargo build --release` | fresh (no source changed this session) |

## 2. Large-file matrix (1 MiB – 64 MiB)

`tmp/m2_p8/file_matrix.py` builds `tmp/m2_p8/file_scale.mink` (filesystem lib):
read the whole file, report exact length, XOR, SUM, first/last byte, write a
copy through `rt_fs_write`, check `rt_fs_file_size` of the copy. Python
compares SHA-256 of source and copy and recomputes every number.

Pattern `b[i] = (i*131 + 17) & 0xFF`.

| Size | exit | read len | write len | XOR | SUM | SHA match | time |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 MiB | 0 | 1048576 | 1048576 | 0 | 133693440 | yes | 1.11 s (cold) |
| 4 MiB | 0 | 4194304 | 4194304 | 0 | 534773760 | yes | 0.02 s |
| 8 MiB | 0 | 8388608 | 8388608 | 0 | 1069547520 | yes | 0.04 s |
| 16 MiB | 0 | 16777216 | 16777216 | 0 | 2139095040 | yes | 0.08 s |
| 32 MiB | 0 | 33554432 | 33554432 | 0 | 4278190080 | yes | 0.13 s |
| 64 MiB | 0 | 67108864 | 67108864 | 0 | 8556380160 | yes | 0.27 s |

No ceiling was reached; the sequence continued upward past the 32 MiB the
session asked for because every level was clean. The only hard bounds remain
the 1 GiB logical heap (a single string plus its source must fit) and
`GetFileSize`'s 32-bit size (single read < 4 GiB−1). Copy-through-`fs_write`
(Session 133's `fs_copy_file` path was not used here) is byte-exact.

## 3. Large strings, allocation churn, mixed shapes

`tmp/m2_p8/heap_matrix.py` + `heap_scale.mink`: five shapes, each in a fresh
process, each run **twice** — identical output on both runs for every case
(no non-determinism), BAD count 0 in all successful runs.

| Shape | Work | Result |
| --- | --- | --- |
| `lifo` | alloc 1 MiB + 64 KiB, stamp/verify 64 boundary bytes, free small then big | 100 / 5,000 / 50,000 cycles: exit 0, 0 bad, deterministic hash; 5,000,000 cycles in 9.3 s |
| `waste` | same allocations, free **big first** (documented drop of the skipped head block) | 1,000–16,300 cycles: exit 0; 16,384+ : exit 102 `E-R02` (see §4) |
| `grow` | 17 doubling rounds 8 B → 1 MiB via `rt_str_concat`, length + boundary + seam verification at every level | 500 rounds exit 0; 600 and 700 rounds: exit 102 `E-R02` at the concat (same ceiling P4 measured) |
| `churn` | N iterations of alloc(size) → stamp/verify first+last 256 B → free | 20,000×4 KiB, 5,000×64 KiB, 500×1 MiB: exit 0, 0 bad |
| `mix` | per iteration 4 KiB + 256 KiB + 16 KiB + 1 MiB allocated, stamped, verified, freed in LIFO order | 1,000 and 5,000 iterations: exit 0, 0 bad |

Boundary bytes are checked on **both** edges of every block, so a
mis-sized/corrupted block or a wrong length appears as a nonzero BAD before any
hash can hide it; BAD stayed 0 in every successful case.

Resource profile of the steady-state `lifo` shape (sampled every 250 ms,
`tmp/m2_p8/sample_probe.py`):

```
5,000,000 cycles in 9.3 s
t=0.00s private= 4.5 MiB handles=4
t=0.26s private= 4.5 MiB handles=44
...
t=9.27s private= 4.5 MiB handles=44     BAD 0
```

~5 TB of cumulative allocation in 9.3 s with perfectly flat private memory and
handles: the free list is reused, nothing leaks, nothing drifts. This is the
strongest stability result of the session.

## 4. Fragmentation: exact ceiling and the "fails with free space" case

The `waste` shape models an application that allocates a large block then a
small one and frees them large-first: the next cycle's large request walks past
the small head block (dropping it) and reuses the large block, so each cycle
bumps 64 KiB and abandons 64 KiB.

Measured ceiling (byte-identical across runs):

| Cycles | Exit | Note |
| --- | --- | --- |
| 16,000 | 0 | clean, BAD 0 |
| 16,300 | 0 | clean, BAD 0 |
| 16,384 | 102 | `E-R02` at the 1 MiB allocation |
| 16,500 / 17,000 | 102 | `E-R02` |

16,384 = 1 GiB / 64 KiB exactly, matching the analytic per-cycle model. The
failure is an explicit structured diagnostic with a source location — never
truncation or corruption.

**"Allocation fails although aggregate free space is plentiful":** the `tail`
shape runs N `waste` cycles and then requests one 4 MiB block.

| Cycles first | 4 MiB request | Exit |
| --- | --- | --- |
| 1,000 | succeeds | 0 |
| 16,000 | succeeds | 0 |
| 16,300 | **fails** | 102 `E-R02` |

At 16,300 cycles the free list holds approximately 16,300 × 64 KiB ≈ 1.0 GiB in
64 KiB blocks, yet a 4 MiB contiguous request fails: the allocator does not
coalesce adjacent free blocks and does not split a reused block. That is the
Session 93 design (identical in both emitters and the Rust reference), recorded
by P4 as deliberate-but-poorly-documented. P8 adds the exact ceiling, the
determinism, and the "free space but no contiguous block" demonstration.

**Classification: B — deliberate allocator behavior, explicit and loud.**
No product change (per the session rules and P4's earlier decision).

## 5. Large JSON

`tmp/m2_p8/json_matrix.py` + `json_scale.mink`: documents are compact integer
arrays `[0,1,2,...]` generated to an exact size and read **from a file** (no
source-literal limits). Two arena configurations: the library's documented
64 KiB arena, and an arena 8× the input (the "generous caller" case).

| Input | documented 64 KiB arena | 8× arena |
| --- | --- | --- |
| 8 KiB | `E-R05`, exit 105 | `E-R05`, exit 105 |
| 32 KiB | `E-R05`, exit 105 | parse returns 0 (graceful signal), exit 0 |
| 64 KiB | `E-R05`, exit 105 | parse returns 0 |
| 256 KiB | `E-R05`, exit 105 | parse returns 0 |
| 1 MiB | `E-R05`, exit 105 | parse returns 0 |
| 4 MiB | `E-R05`, exit 105 | parse returns 0 |

The documented-arena failure is always the structured `E-R05`
("invalid memory access") — loud, deterministic, never silent. Band sweep with
the 8× arena:

| Input | parse | serialization |
| --- | --- | --- |
| 512 B, 1024 B | correct (n, first, last) | exact length |
| 2048–3499 B | correct | `E-R05` (exit 105) |
| 3998–8191 B | `E-R05` | — |

With a fixed 1 MiB arena the serializer can be **silently wrong** (P4's
finding reproduced): 4094 B parse correct but serialization returns 2842 B;
6997 B → 1702 B; 7999 B and 8191 B happen to serialize exactly. That is
category **D** — the library produces wrong data without an error on the
serialize path in a specific geometry — and it is unchanged from P4.

The mixed workload (§7) exposed a related operational limit: the parse
write pointer only moves forward (slot 8191 is persistent), so a long run must
allocate a **fresh arena per parse**; reusing one 64 KiB arena fails loudly
with `E-R05` after ~1,162 parses of the workload's documents. Documented in the
workload, not fixed.

**No JSON limit was raised.** Per the session rules, a larger input failing is
evidence, not a defect; the silent-serialization case remains P4's D-class
finding for the JSON design session.

## 6. Large HTTP bodies, the size ceiling, and the truncation finding

Fixture: `tmp/m2_p8/http_fixture.py`, deterministic raw HTTP on `127.0.0.1`
(one request per connection, `Connection: close`). Probe
`http_scale.mink`: status, raw response bytes, extracted body length,
first/last bytes and an FNV-1a hash of the body, plus `http_last_error()`.
Python recomputes every value.

Body matrix (two requests per size, both identical):

| Served body | status | body | first/last | hash | `http_last_error()` |
| --- | --- | --- | --- | --- | --- |
| 64 KiB | 200 | exact | 17/142 | match | 0 |
| 256 KiB | 200 | exact | 17/142 | match | 0 |
| 1 MiB | 200 | exact | 17/142 | match | 0 |
| 2 MiB | 200 | exact | 17/142 | match | 0 |
| 4 MiB | 200 | exact | 17/142 | match | 0 |
| 8 MiB | 200 | 4,202,433 prefix | 17/81 | prefix-correct | **−4 `TOO_LARGE`** |

Overflow boundary (budget = 4,202,496 whole-response bytes):

| Served | status | body | error |
| --- | --- | --- | --- |
| 4 MiB + 0 | 200 | 4,194,304 | 0 |
| 4 MiB + 1 | 200 | 4,194,305 | 0 |
| 4 MiB + 4096 | 200 | 4,198,400 | 0 |
| 4 MiB + 16384 | 200 | 4,202,433 | −4 `TOO_LARGE` |
| 4 MiB + 65536 | 200 | 4,202,433 | −4 `TOO_LARGE` |

A peer that accepts and closes with no response still reports −5 (Session 135's
`h44` fix holds).

**Truncation vs `Content-Length` (P7's deferred finding) — reproduced
deterministically:**

| Declared | Actually sent | status | body | `http_last_error()` |
| --- | --- | --- | --- | --- |
| 524,288 | 200,000 | 200 | 200,000 | **0** |
| 1,048,576 | 1,000 | 200 | 1,000 | **0** |
| 65,536 | 0 | 200 | 0 | **0** |
| 200,000 | 200,000 | 200 | 200,000 | 0 (control) |

`http_last_error() == 0` is documented as "0 after a completed request", and
the module's failure model calls an aborted transfer a failed request — so a
body that the server declared longer than it delivered is returned as a
successful response. This is a genuine correctness defect (silent data
truncation; category A/B), not a documented limitation. The smallest safe fix
is identified in §11; it was **not** applied this session (see §11/§13).

## 7. Large process output and the exact 8 MiB boundary

`tmp/m2_p8/process_matrix.py` + `process_scale.mink`: a Python child emits
`b[i] = 65 + (i % 26)` on stdout (or stderr); the probe runs it with
`process_run`, then reports the captured length, first/last byte and FNV-1a
over the *borrowed* capture view (`process_stdout()`/`process_stderr()` must
not be freed — the probe reads them inline for exactly that reason).

| Emitted | Exit | Captured | Hash |
| --- | --- | --- | --- |
| 1 MiB | 0 | exact | match |
| 4 MiB | 0 | exact | match |
| 7 MiB | 0 | exact | match |
| 8,388,600 B | 0 | exact | match |
| 8,388,601 B | 102 | — | `E-R02` |
| 8,388,607 / 8,388,608 B | 102 | — | `E-R02` |
| 8,388,609 B | 102 | — | `E-R02` |
| stderr 1 MiB (stdout empty) | 0 | stderr exact, stdout 0 | match |

The per-stream payload capacity is **8,388,600 bytes = 8 MiB − 8**: the window
includes the 8-byte length prefix (`STDIO_WINDOW_DATA` in the emitter). The
P2/P3 records say "8 MiB per stream with a loud `E-R02` beyond"; P8 pins the
exact byte where it flips. `E-R02` is explicit, immediate and never truncates.
The 8 MiB contract was not changed.

## 8. Sustained mixed workload

`tmp/m2_p8/mixed.mink` + `mixed_driver.py`. One iteration combines:

1. **filesystem** — build `tmp/m2_p8/mixed/work_<i>` and write it, then read it
   back;
2. **strings** — content assembled by `rt_str_concat` (64 × `"<i>-P8payload-"`);
3. **hashing** — SHA-256 of the read-back file (first 8 hex digits folded into
   the report);
4. **JSON** — parse `[<i>]` with a fresh 64 KiB arena, check array length and
   value;
5. **network** — `http_client_get` a 1024-byte fixture body, check status 200
   and error 0;
6. **SQLite** — every 50th iteration, `INSERT INTO items VALUES (1,'v',7)`
   (literal statement, see §10) and count rows with `sql_rows`.

The driver verifies **every** iteration line against an independent Python
computation (content length, SHA prefix, JSON value, HTTP length, DB count),
independently verifies the final iteration's file on disk, and re-runs the whole
workload to require byte-identical output.

| Iterations | Exit | Iterations verified | Mismatches | Final artifact | Repeat run |
| --- | --- | --- | --- | --- | --- |
| 100 | 0 | 100 | 0 | byte-equal, sha verified | byte-identical |
| 500 | 0 | 500 | 0 | byte-equal | byte-identical |
| 5,000 | 0 | 5,000 | 0 | byte-equal | byte-identical |
| 20,000 requested | 102 `E-R02` | 10,201 | 0 | byte-equal (iteration 10,200) | — |

The 5,000-iteration run: 55 s, 100 SQLite rows, `BADTOTAL 0`.
The 20,000-iteration request fails at iteration 10,201 with the structured
`E-R02` at `mixed.mink:4665` (the per-iteration 64 KiB JSON arena allocation);
all 10,201 completed iterations verified, the artifact of the last completed
iteration is byte-equal, and the failure is explicit with a source location.
There is no corruption and no silent loss.

Resource trend (private bytes / handles, sampled every 50 ms):

```
5,000 iterations:    t=0.00s   4.7 MiB / 59 handles
                     t=27.6s 625.6 MiB / 59 handles
                     t=55.2s 888.5 MiB / 59 handles
20,000 iterations:   heap ceiling (~1,050 MiB private) at iteration 10,201
```

Handles are flat at 59 for the entire run — no descriptor growth, no stale
socket or file state. Memory grows ~100 KiB per iteration of this shape until
the 1 GiB heap ceiling; the growth is allocator fragmentation (per-iteration
64 KiB JSON arena plus string churn in an allocator that neither splits nor
coalesces, §4), not a leak: the process exits through the structured `E-R02`
with a source location, and every completed iteration was verified correct.

## 9. Resource measurements

| Workload | Duration | Private bytes | Handles | Failures |
| --- | --- | --- | --- | --- |
| `heap_scale lifo`, 5,000,000 cycles | 9.3 s | 4.5 MiB flat | 44 flat | none |
| mixed workload, 100 iterations | <1 s | 4.7 MiB | 59–60 | none |
| mixed workload, 5,000 iterations | 55 s | 888 MiB (growing) | 59 flat | none |
| mixed workload, until `E-R02` | ~2–3 min | ~1,050 MiB (heap ceiling) | 59 flat | loud `E-R02` at iteration 10,201 |
| 64 MiB file round trip | 0.27 s | — | — | none |

Interpretation: absolute memory is not treated as a defect. The stability
signals are (a) handles flat everywhere, (b) the LIFO steady state perfectly
flat over 5 M cycles, and (c) the mixed workload's growth being attributable to
the documented fragmentation rule with an explicit ceiling rather than a leak
(no `E-R06`, no `E-R04`, no crash).

## 10. Defect 1 — SQLite misparses heap-built statements (E, reproduced)

Found while building the mixed workload. Minimal reproducer
(`tmp/m2_p8/sql_probe5.mink`): identical 34-byte statement text, two storage
classes, same database:

```
CREATE=OK|HEAPLEN=34|HEAP=INSERT INTO items VALUES (2,'v',7)|
LIT=OK|CONCAT=ERROR: unsupported statement|ROWS=-1
```

- `sql_exec(db, "INSERT INTO items VALUES (2,'v',7)")` — literal → `OK`
- `sql_exec(db, heap_stmt)` where `heap_stmt = rt_str_concat(...)` has exactly
  the same bytes (printed immediately before the call) → `ERROR: unsupported
  statement`

`sql_probe4` shows the heap statement fails as the *first* statement after
`CREATE` and after a preceding literal insert; `sql_probe3`/`sql_probe2` show
`CREATE` itself works with a literal and that the same table/statement is fine
in literal form. The engine's `_arena` copies both strings byte-wise and
`_run` then sees "no known first keyword", i.e. the statement copy inside the
arena does not hold the statement bytes for a heap source.

This is MINK handling a workload **incorrectly** (not a missing capability):
the same SQL text produces different results depending on how the string was
produced, and dynamically built SQL is the normal case for real applications.
Root cause not yet established (candidates: the arena copy path for heap
strings, or a codegen/argument-handling interaction when both `sql_exec`
arguments are heap strings). Per the session rules this is stopped and
recorded, not fixed blind:

- reproducer: `tmp/m2_p8/sql_probe5.mink` (build with `stdlib/sqlite.mink`
  prepended, `tmp/m2_p8/common.py assemble(..., ["sqlite"], body)`);
- expected next step: instrument `_arena`/`_iscan_kw` to print the arena bytes
  for both storage classes, identify whether the copy or the argument
  hand-off is wrong, then fix with a permanent regression test in
  `tests/sqlite_lib.rs`.

**Classification: E — genuine implementation defect, root cause open.**

### Defect 2 — HTTP truncated body reported as success (A/B, reproduced)

§6. The smallest safe fix, determined but not applied: in `http_send`, after
`http_recv_all` and only when `http_last_error_code == 0`, compare the body
length against `Content-Length` when that header is present and no
`Transfer-Encoding` header exists, and set `HTTP_ERR_RECV` (−5, the existing
"aborted transfer" code) on a short body. That is a stdlib-only change
(`stdlib/http.mink` + its `npm/mink/stdlib/http.mink` mirror, which must stay
identical) plus a new `tests/http_lib.rs` fixture that declares more bytes than
it sends. It must not fire for `TOO_LARGE` responses (error already −4), for
responses without `Content-Length`, or for chunked responses.

### Defect 3 — sustained mixed workload reaches the heap ceiling (B, recorded)

§8. Documented allocator behavior; explicit `E-R02`.

## 11. Changes

**No product-code changes.** Two proven defects (§10) need fixes with permanent
regression coverage, and the session ran past its timebox before either could
be done to the standard the rules require (minimal reproducer → root cause →
smallest safe fix → focused tests → re-run the large-scale reproducer →
re-run the integrated workload). Both reproducers are minimized and
deterministic, so the follow-up session can start from them directly. The
alternative — a rushed change to the SQLite arena copy or the HTTP receive
path in the last minutes of an overrun session — is exactly what the rules
forbid.

- New probe files (all git-ignored, `tmp/m2_p8/`): `common.py`,
  `file_scale.mink` + `file_matrix.py`, `heap_scale.mink` + `heap_matrix.py`,
  `json_scale.mink` + `json_matrix.py`, `http_scale.mink` + `http_matrix.py` +
  `http_fixture.py`, `emit_child.py` + `process_scale.mink` +
  `process_matrix.py`, `mixed.mink` + `mixed_driver.py`, `sample_probe.py`,
  `sql_probe*.mink`, generated inputs/logs.
- No runtime, emitter, compiler, Cargo, version, tag, npm or Linux change.

## 12. M2 evidence matrix

| Capability | Tested scale | Result | Classification |
| --- | --- | --- | --- |
| Filesystem (read/write/copy) | 1, 4, 8, 16, 32, 64 MiB | exact lengths, SHA-256 match, XOR/SUM match, 0.27 s at 64 MiB | works (limit: 1 GiB heap, 4 GiB `GetFileSize`) |
| Strings / heap | 8 B → 1 MiB × 17 doubling rounds; 5 M alloc/free cycles | 500 grow rounds clean (600 → `E-R02`); 5 M cycles with flat 4.5 MiB / 44 handles; BAD 0 everywhere | works (grow ceiling documented) |
| Allocator fragmentation | 16,384-cycle `waste` shape; 4 MiB request after 16,300 cycles | ceiling exactly 1 GiB/64 KiB = 16,384 cycles; large request fails with ~1 GiB free in 64 KiB blocks | B — deliberate design, loud `E-R02` |
| JSON | 8 KiB – 4 MiB, documented + 8× + 1 MiB arenas | documented arena: loud `E-R05` from 8 KiB; 8× arena: parse 0 above 32 KiB; ≤1 KiB parse+serialize exact; 2–3.5 KiB parse ok / serialize `E-R05`; 1 MiB arena: silent serializer truncation at 4094/6997 B | C/D — documented-arena limit (loud) + P4's silent-serialization D, not fixed |
| HTTP body | 64 KiB – 8 MiB; budget boundary; truncation | exact through 4 MiB (twice each); 8 MiB → loud −4 with pattern-correct prefix; **truncated body → err 0** | works to 4 MiB (C for the ceiling); **A/B defect for truncation** |
| Process output | 1 MiB – 8,388,609 B, stdout + stderr | byte-exact to 8,388,600 B; `E-R02` at 8,388,601; streams independent | works; exact payload boundary = window − 8 B (C) |
| Sustained mixed workload | 100 / 500 / 5,000 verified iterations; 10,201 until `E-R02` | 0 mismatches, final artifact verified, repeat byte-identical, handles flat | works to the heap ceiling (B) |
| Memory stability | 5 M-cycle steady state; mixed workload | flat 4.5 MiB for LIFO steady state; mixed grows ~127 KiB/iteration (fragmentation) to the 1 GiB ceiling | stable / B |
| Handle stability | all runs | 44 (heap probe) / 59 (mixed) flat; no growth, no stale sockets | stable |
| SQLite under load | every 50th iteration of the mixed workload | literal statements correct (100 rows at 5,000 iterations); **heap-built statements misparse** | **E defect, open** |

## 13. Remaining gaps and recommendation for M2 P9

Priority order for the next session:

1. **Fix defect 1 (SQLite heap-statement misparse)** — root-cause the arena
   copy / argument hand-off, fix minimally, and add a permanent regression
   test to `tests/sqlite_lib.rs`. Re-run the mixed workload afterwards. This is
   the only defect found that makes a normal workload produce wrong results
   (rather than a loud failure), and it invalidates dynamic SQL for real
   applications.
2. **Fix defect 2 (HTTP truncation)** with the `Content-Length` check from §10
   and an `h45` regression test; re-run the 4 MiB body matrix and the
   truncation table.
3. Re-run the mixed workload for 5,000+ iterations against the fixed SQLite
   path to confirm the ceiling behavior is unchanged, then decide whether the
   per-iteration arena allocation in the workload should be replaced by a
   JSON reset API (a JSON design decision, not a P9 fix).
4. If both fixes land, M2 P9 can be the final Windows gap-closure/audit session
   (re-run this evidence matrix plus P1–P7 suites on the fixed tree).
5. Not a defect, carried for M3: JSON arena/serializer design (P2's capacity
   contract), allocator coalescing/splitting (only if an allocator session is
   scheduled), socket connect/receive timeouts (P5), P6's shared-scratch
   `rt_print_int`.

## 14. Repository state and audit

- Intended session change: this document. No product, test, runtime, emitter,
  Cargo, version or npm change.
- Version 1.0.4; tags `v1.0.0`–`v1.0.4`; HEAD `bb3ff1e`; nothing committed; no
  npm/publish/release/tag command.
- All pre-existing modified/untracked files (Sessions 128–135 work, the
  roadmap archive, `nl1.*`, the untracked `httpserver` files and
  `tests/httpserver_lib.rs`, the pasted transcript) are untouched, as is
  Linux.
- Probes live under the git-ignored `tmp/m2_p8/` (`git check-ignore` →
  `.gitignore:14:tmp/`).
- No server, fixture, probe or client process is left running (§15).

## 15. Probe inventory (git-ignored, `tmp/m2_p8/`)

| Probe | Purpose |
| --- | --- |
| `file_scale.mink`, `file_matrix.py` | 1–64 MiB read/verify/write/SHA matrix |
| `heap_scale.mink`, `heap_matrix.py` | lifo/waste/tail/grow/churn/mix shapes + ceilings |
| `json_scale.mink`, `json_matrix.py` | 512 B – 4 MiB parse/serialize with 3 arena geometries |
| `http_scale.mink`, `http_fixture.py`, `http_matrix.py` | body matrix, overflow, truncation, close-without-response |
| `process_scale.mink`, `emit_child.py`, `process_matrix.py` | capture boundaries to 8,388,609 B |
| `mixed.mink`, `mixed_driver.py` | sustained 6-subsystem workload + independent verification |
| `sample_probe.py` | external private-bytes/handle sampling |
| `sql_probe2–6.mink`, `sql_probe.mink` | SQLite statement-shape isolation and the minimized defect reproducer |
| `common.py`, `logs/`, `gen/`, `files/`, `json/`, `mixed/`, `*.log` | build/test harness, generated inputs, logs |
