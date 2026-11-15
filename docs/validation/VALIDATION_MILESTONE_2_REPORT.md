# Validation Milestone 2 — Windows Practical-Parity Workloads

Session 280. Scope: Windows x86_64 only (Linux remains frozen), compiler
`mink 1.0.5` built from this tree in release mode.

## 1. What this milestone validates

Milestone 1 exercised the language and the standard library feature by feature
(228 cases, see `VALIDATION_MILESTONE_1_*`). Milestone 2 does the opposite: it
runs whole applications that look like real Windows work — large data, process
automation, networking, an HTTP server, threads, a bounded soak and one
integrated program — and compares each one against an independent Python
reference on identical inputs.

Every application lives in its own `validation/m2_*` directory with a
`record.md`, the MINK source, a Python reference where byte-parity applies and a
`verify.py` harness. Nothing in this milestone reuses the earlier Session
129–138 applications: the tree changed substantially since (M7 phases), and the
older records do not pin the limits this brief asks about. The older evidence is
kept as history, not counted here.

## 2. How a result is counted

An application is *validated* only when its binary has been executed and its
observable behaviour checked. Compiling is not evidence. Each harness compares
stdout, stderr and the exit code byte for byte against the reference, or — where
parity is not meaningful (a server driven by concurrent clients) — asserts the
protocol-level and invariant-level expectations directly in Python.

Classifications used throughout, exactly as the brief defines them:

* **Compiler/runtime defect** — the program is legal and the runtime misbehaves.
* **Missing capability** — the language or library has no way to express it.
* **Environmental failure** — the machine or an external service failed.
* **Intentional limitation** — documented bound, reported structuraly.
* **Test/application defect** — the harness or the app was wrong.

## 3. Applications

| # | App | Class | Harness result |
|---|-----|-------|----------------|
| A | `m2_bigio` | large data / files / stdin / JSON | 25/25 cases match the Python reference |
| B | `m2_procauto` | process automation and capture | 8/8 cases match the Python reference |
| C | `m2_net` | TCP / UDP / HTTP clients under load | 11/11 cases match the Python reference |
| D | `m2_server` | MINK HTTP server, concurrent clients | 120 requests over 16 workers + 40 over 4: all statuses/bodies correct |
| E | `m2_threads` | contention, spawn/join churn, failures | 60,000 locked increments exact, 400 spawn/join work units, return codes propagate |
| F | `m2_soak` | bounded long run (10 s) | 238,152 loops, 23,816 threads, exit 0, no leak |
| G | `m2_integrated` | SQL + files + JSON + threads | PASS (self-checking, re-verified in Python) |

### Capability matrix

| Workload class | Capabilities exercised | Prior evidence (M1) | New validation in M2 | Result |
|----------------|------------------------|---------------------|----------------------|--------|
| Large data / files | `rt_str_alloc` + byte writes, `fs_write`/`fs_read`, SHA-256 over 5 MiB, line/digit scanning, `fs_remove_file` | `a1`/`a2`/`a3` apps, `fs_read` suite | `m2_bigio` gen/scan/copy/clean, 5 MiB round trip with Python parity | PASS |
| stdin / stdout volume | `rt_stdin_read` (8 MiB window), CRLF print, 1 MiB piped input | M1 stdin cases | `m2_bigio stdin` 128 KiB and 1 MiB, byte-identical stdout | PASS |
| JSON at scale | `json_parse_in` arena (slots), `json_arr_len/get`, `json_obj_get_key/val` | `c_json` app | `m2_bigio json` on a 12,000-record document, sum matched in Python | PASS |
| Mixed allocation churn | allocator free-list reuse across 64 KiB + 1 KiB | capacity suite (single size) | `m2_bigio cycles`, `churn_repro.mink` | cycles PASS, churn **defect D2 open** |
| Process automation | `process_run`, `process_run_input`, `process_stdout(_len)`, `process_stderr(_len)`, `process_run_ok` | `f_process` app | `m2_procauto` capture/feed/many/codes/missing vs Python reference | PASS |
| Child stdin bound | `STDIN_PIPE_LIMIT` (4,096) | documented only | `m2_procauto feed` measured 4,096 of 5,000 | limitation **L1** |
| TCP client load | `net_dial`, `net_send`, `net_recv` loop, 1 MiB payload | `e_network` basic TCP | `m2_net bulk` (1 MiB echo), `repeat` (200 round trips) | PASS |
| UDP | `net_socket`, `net_sendto`, `net_recvfrom` | `e_network` basic UDP | `m2_net udp` 8 KiB datagram round trip | PASS |
| HTTP client | `http_client_get`, `http_status_code`, `http_body` | `e_network` HTTP/HTTPS | `m2_net http` against the fixture server, body hash | PASS |
| Network failure modes | refused connect, `net_connect_timeout` bound | partial in M1 | `m2_net refused`, `timeout` (400 ms budget enforced) | PASS |
| HTTP server | `http_server_listen`, `_serve_default`, `_path`, `respond_ok`, `respond_not_found`, `_close` | `httpserver` unit corpus | `m2_server` with 16 concurrent Python clients, 160 requests | PASS |
| Threads | `thread_spawn/join`, `lock_new/acquire/release/free`, `rt_mem_*` contexts | `g_threads` pool/pipeline | `m2_threads` 12×5,000 increments, 100 spawn/join cycles, return codes | PASS |
| Long-running process | continuous alloc/file/thread work, leak check at exit | none | `m2_soak` 10 s, 238,152 loops, 23,816 threads | PASS |
| Integrated program | SQL engine, filesystem, JSON, threads in one binary | `d_sqlite`, `c_json`, `g_threads` separately | `m2_integrated` 100 rows + JSON file round trip + threaded sum | PASS |
| SQL aggregate | `sql_exec` subset | `d_sqlite` | `m2_integrated` COUNT only; no `SUM` in the engine | limitation **L4** |

### A — large data, files, stdin, JSON (`validation/m2_bigio`)

Commands: `gen` (write a 5 MiB file byte by byte), `scan` (length, line count,
digit checksum and SHA-256 of a file), `copy`, `stdin` (128 KiB and 1 MiB),
`grow` (doubling string concatenation), `cycles` (64 KiB build/free loop),
`json` (12,000-record document, one field summed), `clean`.

Result: 25/25, including every error path (missing file, bad usage, option-looking
argument, invalid JSON, non-array root, unknown command) with identical exit
codes. 5 MiB `gen` and the 1 MiB stdin read are within the documented 8 MiB
stdio window.

### B — process automation (`validation/m2_procauto`)

`capture` runs a helper that writes 64 KiB to stdout and 3 bytes to stderr and
exits 7; all four numbers (lengths, SHA-256s, code) match the reference.
`feed` offers 5,000 bytes on the child's stdin and gets 4,096 back plus the
child's own CRLF — the documented `STDIN_PIPE_LIMIT`, see limitation L1.
`many` runs 50 children in one process and proves capture is replaced, not
appended. `codes` checks non-zero propagation and `process_run_ok`. `missing`
launches a non-existent command and reports code 1 with empty stdout.

### C — networking under load (`validation/m2_net`)

Against a Python fixture server: a 1 MiB TCP upload echoed back and verified by
SHA-256, 200 sequential round trips on one connection, one 8 KiB UDP datagram
round trip, an HTTP GET through the stdlib client (status, body length and body
hash), a refused connection, and `net_connect_timeout` to a black-holed address
that returns -1 within its 400 ms budget. 11/11 including usage errors.

### D — HTTP server (`validation/m2_server`)

The MINK server answers `/health` with 200 `healthy` and anything else with 404
`nope`, then exits after a fixed number of requests. 16 concurrent Python
clients generated 120 requests (40 to `/health`), 4 workers generated 40 more;
every status and body was correct and the server printed `served=` exactly.
Connections are closed per request, so this covers 160 accepts and a backlog of
simultaneous connections.

### E — threads (`validation/m2_threads`)

12 threads × 5,000 locked increments = 60,000 exactly, with join results summing
to the same number; 100 spawn/join cycles × 4 workers lose no work; 8 workers
returning 7 propagate 7 to every join. Exit codes are asserted, not inferred.

### F — bounded soak (`validation/m2_soak`)

Ten seconds of continuous work in one process: 64 KiB string build/free, a file
write/read every 50 iterations, two threads every 20. 238,152 iterations,
23,816 thread lifecycles, exact thread counter, clean exit — no leak report
(E-R06) and no stalled thread.

### G — integrated application (`validation/m2_integrated`)

100 rows inserted into the MINK SQL engine, read back with `SELECT COUNT(*)`;
the same 100 records written as JSON, re-read from disk, re-parsed and summed;
1..1000 squared split across four threads. The SQL count, the JSON record count,
the value total (501,981) and the threaded total (333,833,500) all agree, and
`records.json` is re-checked with Python's `json` module.

## 4. Defects found and fixed

### D1 (fixed) — every string access scanned the whole liveness table

**Symptom.** Building a 64 KiB string byte by byte took ~16 ms; 8,000 such
builds took 132 s. A 20,000-iteration soak of `str_repeat` timed out at 300 s.
Bulk string work was ~2.7 MB/s — two orders of magnitude slower than the same
loop in Python.

**Cause.** `rt_str_validate` proved that a `Str` pointer is a live heap block
start by scanning all 1,024 liveness entries on **every** access
(`rt_str_byte`, `rt_str_set_byte`, `rt_str_len`, …). Both the source literal and
the destination buffer were re-proved per byte, so a byte loop cost
`bytes × 1,024` comparisons.

**Fix** (`src/backend/emit/runtime.rs`, `linux_runtime.rs`, `src/runtime/abi.rs`):

* image literals are recognised by a two-load range check against
  `str_data_start`/`str_data_end` (they live in a known immutable region), and
* heap strings are cached: `BSS.str_validate_slot` / `str_validate_start` record
  the liveness entry that last validated a string. The hint is only trusted
  after re-reading the entry's live flag **and** its start address, so a freed
  or reused block can never be validated from a stale hint; a miss falls back to
  the full scan and re-arms the hint.

**Measured.** 8,000 × 64 KiB builds: **132 s → 3.1 s (43×)**; the same program's
overall throughput is now ~165 MB/s.

**Regression.** `tests/capacity.rs::cap09_validation_fast_paths_alternate_strings`
alternates heap and literal reads across 10,000 accesses and checks the exact
byte sum.

### D2 (found, not fixed) — mixed-size churn exhausts the bounded arena

**Symptom.** `validation/m2_bigio/churn_repro.mink`: a loop that allocates
64 KiB, frees it, allocates 1 KiB, frees it, dies with `E-R02` (heap exhausted)
after ~16,000 iterations. Every block is freed before the next allocation, so
two blocks should be enough. `8000` iterations succeeds; `20000` fails.

**Cause.** The free list is first-fit without splitting: the 1 KiB request is
served from the 64 KiB block, the block is re-recorded at 1 KiB, and the blocks
ahead of it are dropped from the list. The arena therefore grows by 64 KiB per
iteration until the 1 GiB ceiling.

**Status.** Reproduced and documented (`churn_repro.mink`, `record.md`), not
fixed in this milestone. Two candidate fixes were prototyped and rejected:
recording the block's full capacity in the liveness table and a best-fit walk
both broke `tests/zlib_lib.rs` (E-R05 from an interior arena read), which shows
the liveness-table size is load-bearing beyond allocation bookkeeping. A correct
fix needs either coalescing or explicit size classes and belongs in a dedicated
allocator change with its own test suite. This is the one open runtime defect
this milestone leaves behind; it is bounded (1 GiB), diagnosable (`E-R02` with a
source location) and never corrupts data.

**Regression pinning current behaviour.**
`tests/capacity.rs::cap08_single_size_churn_reuses_the_free_list` proves that a
single size class reuses its block indefinitely (200,000 cycles).

## 5. Intentional limitations reconfirmed

| Id | Limitation | Evidence | Practical impact |
|----|-----------|----------|------------------|
| L1 | `process_run_input` delivers at most 4,096 bytes to a child's stdin | app B `feed`: 5,000 offered, 4,096 echoed | loop or use a file for larger child input; documented in `stdlib/process.mink` |
| L2 | heap ceiling 1 GiB; `MAX_LIVE_ALLOCS` 1,024 | tests/capacity.rs cap01–cap06 | D2 above is the visible consequence |
| L3 | stdio capture window 8 MiB per stream; program stdin 8 MiB−8 | app A (1 MiB stdin, 5 MiB file), app B (64 KiB capture) | large but not unbounded; overflow is a structured `E-R02`, never truncation |
| L4 | SQL engine copies the database per statement (O(N²) bytes) and has no `SUM`/`GROUP BY` | app G inserts 100 rows; app C/D unaffected | keep row counts moderate or batch; aggregate in MINK |
| L5 | HTTP server handles one connection at a time | app D (160 requests, backlog 16) | throughput, not correctness; poll helpers exist for a multiplexed server |

## 6. Verification actually run

* `python validation/m2_bigio/verify.py` → 25/25
* `python validation/m2_procauto/verify.py` → 8/8
* `python validation/m2_net/verify.py` → 11/11
* `python validation/m2_server/verify.py` → PASS (120 + 40 requests, usage errors)
* `validation/m2_threads` → invariants held for 60,000 increments, 100 cycles, 8 failures
* `python validation/m2_integrated/verify.py` → PASS
* `validation/m2_soak/main.exe 10` → exit 0, 238,152 loops
* `cargo test --all-targets` (debug and release) — see §7
* `python tools/release_acceptance.py` — see §7

## 7. Build and suite status

| Run | Result |
|-----|--------|
| `cargo test --all-targets` (debug) | 109 blocks, **3,610 passed, 0 failed, 9 ignored** |
| `cargo test --release --all-targets` | 109 blocks, **3,610 passed, 0 failed, 9 ignored** |
| `python tools/release_acceptance.py --session 280` | **PASS** on the committed tree (`report.md`, `report.json` under `target/acceptance/280/`) |

The suite gained two tests over the Session 279 baseline (3,608 → 3,610), both
from the D1 investigation (single-size free-list churn, alternating string
validation). The acceptance verdict describes the artifact build and the suite
on a clean checkout; it is not the milestone verdict — the milestone is PARTIAL
because of D2, which no suite covers yet.

## 8. Verdict inputs

* All seven workloads execute and their behaviour matches the reference.
* One genuine runtime defect was found and fixed with a permanent regression
  (D1, 43× on large string work).
* One genuine runtime defect remains open and documented with a minimal
  reproduction (D2).
* No missing capability was required by these workloads; the SQL and HTTP
  servers are deliberate subsets recorded as limitations.

**Milestone verdict: PARTIAL** — every workload in the brief was executed and
validated, and the largest performance defect was fixed, but D2 (mixed-size
churn exhausting the bounded arena) is a real runtime defect that this
milestone exposes and does not yet fix. Closing it is the first task of the
next milestone.

## 9. Update, 2026-11-15 (M7 Phase D) — D2 is closed

D2 is corrected at its root cause by the Phase D allocator change: the heap now
keeps an address-ordered free list, splits a reused block at exactly the
requested size and coalesces a freed block with its address neighbours, so a
released block (or a split remainder) is always reusable. The reproduction
`validation/m2_bigio/churn_repro.mink`, run unchanged, completes 1,000,000
cycles in 0.04 s; before the change it died with `E-R02` after ~16,000 cycles
(measured: 8,000 exit 0, 20,000 exit 102). The liveness table is unchanged in
size and meaning, and `tests/zlib_lib.rs` passes unchanged (15/15).

All seven workloads were rerun against the modified runtime with the release
compiler: big data/files/stdin/JSON 25/25, process automation 8/8, networking
11/11, HTTP server PASS, thread contention/churn/failure invariants hold, the
10 s soak exits 0 (1,512,201 loops, 151,222 thread lifecycles — 6.9× the
pre-fix count in the same wall clock on this machine), and the integrated
SQL+JSON+threads program PASSes. A permanent 20-case corpus
(`tests/allocator_conformance.rs`) now covers mixed-size reuse, coalescing,
splitting, invalid frees and the stdlib compatibility surface; 9 of its cases
fail on the pre-fix tree.

With D2 closed, the stated reason for this milestone's PARTIAL verdict no
longer applies. The per-session verdict above is left as recorded; the
milestone can be re-scored on the Phase D evidence in
[`SESSION_281_M7_PHASE_D_ALLOCATOR.md`](../implementation/SESSION_281_M7_PHASE_D_ALLOCATOR.md).
The remaining ceilings (4,096-byte child stdin, 8 MiB stdio windows, 1 GiB
heap / 1,024 live allocations, O(N²) SQL copies, one HTTP connection at a time)
are unchanged, documented limits, not defects.
