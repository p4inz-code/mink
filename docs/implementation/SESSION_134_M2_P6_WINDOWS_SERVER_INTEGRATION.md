# Session 134 — M2 P6: Windows server workloads and integrated application

**Scope:** prove that the current Windows x86_64 runtime can carry realistic
long-running/server-style workloads with the APIs that already exist: TCP server
lifecycle, multiple and concurrent clients, request/response correctness,
SQLite-backed service behaviour, sustained execution, failure recovery and
resource stability. Validation-first; no HTTP-server subsystem was added. Linux
was not touched; the version stays 1.0.4.

**Outcome:** the milestone passes with one runtime defect found and fixed. A
multi-client TCP service served 11,500+ echo requests (including 4,800
concurrent requests) with exact responses, accepted 120 intentionally bad
connections and kept serving, and another service persisted a SQLite-backed
key/value store through clean restarts. The defect: `rt_str_from_int` built its
digits in a shared `.bss` scratch buffer, so concurrent worker threads received
each other's numbers — the exact failure shape of a server formatting a length
or id into a response. It is fixed in the Windows emitter with a permanent
regression test. One related latent issue (`rt_print_int` still shares that
buffer) is recorded for M3.

## 1. Baseline

Read before probing:

- `stdlib/network.mink`: `net_listen_on(addr, port, backlog)` is init + socket +
  bind + listen; `net_accept` blocks; `net_recv(sock, maxlen)` returns an owned
  `Str` (P5 made it a fresh per-call allocation); `net_wait_readable(sock,
  timeout_ms)` is the only timeout surface; there is no connect or receive
  timeout option; `net_set_nonblocking` exists and `net_poll` wraps `WSAPoll`.
- `stdlib/sqlite.mink`: the engine is a value transform —
  `sql_exec(db, sql) -> (result, newdb)` — over a database blob owned by the
  caller. It has no handle, no file I/O and no locking; persistence is the
  application's job. Its header documents that each statement copies the whole
  database, so appending N rows costs O(N²) bytes. Supported SQL:
  `CREATE TABLE`, `INSERT`, `SELECT *` / `SELECT COUNT(*)`, `UPDATE`, `DELETE`,
  `WHERE`, `BEGIN`/`COMMIT`/`ROLLBACK`.
- `stdlib/threads.mink`: a thread entry is `fn(Ptr<Int>) -> Int`, with an
  explicit shared context block and `rt_mutex_*`-backed `lock_new/acquire/
  release/free`.
- `stdlib/filesystem.mink`: `fs_read`/`fs_write` are stdlib functions with
  **consuming** `Str` parameters; they forward to the binary-safe
  `rt_fs_read`/`rt_fs_write` intrinsics (length-prefixed, so NUL and 0x01/0x0A
  bytes in the SQLite blob round-trip).
- Session 133 (`SESSION_133_M2_P5_WINDOWS_NETWORKING.md`): hostname TCP and
  plain HTTP work; concurrent `net_recv` and first-time `net_init` were fixed
  there; `net_dial` error preservation and timeout options are open M3 items.
- Session 132 (`SESSION_132_M2_P4_WINDOWS_RESOURCE_SCALING.md`): resource
  scaling baseline.
- `tests/httpserver_lib.rs` + `stdlib/httpserver.mink` (pre-existing untracked
  files, untouched): the existing HTTP server module, exercised here only to
  confirm it still builds and passes.

Focused baseline on this tree:

| Command | Result |
|---|---|
| `cargo test --test network_lib` | 37 passed, 0 failed (35.50 s) |
| `cargo test --test sqlite_lib` | 16 passed, 0 failed (13.27 s) |
| `cargo test --test httpserver_lib` | 10 passed, 0 failed (7.79 s) |
| `cargo build` | OK |
| `cargo build --release` | OK, 32.16 s (P5's rustc crash did not reproduce) |

The compiler used for all probes was `target/debug/mink.exe` unless stated;
section 12 records the release-build re-verification.

## 2. TCP server workload

Two temporary probes under the ignored `tmp/m2_p6/`:

- `server_echo.mink` (network + threads): `ECHO <payload>` replies
  `OK <payload> <length>`, `QUIT` replies `BYE` and stops the service. One
  request per connection, LF terminated. `argv[0]` is the port and `argv[1]`
  the worker count. A 512-byte read cap, a 2 s readiness timeout per read, a
  100 ms poll on the listener and non-blocking accept let N workers share one
  listener without one silent peer pinning the whole service.
- `server_kv.mink` (network + filesystem + sqlite): the persistent
  key/value service described in section 6.

Lifecycle validated end to end: `net_listen_on` (init + socket + bind +
listen), `net_accept`, receive, deterministic response, `net_shutdown` +
`net_close` per client, and a clean `QUIT` that joins every worker, closes the
listener, frees the lock and context, and exits 0.

Repeated sequential connections (one worker, MINK client): 20 requests, 0
failures, exit 0. Server counters: 22 connections served (one readiness probe
that connected and closed, 20 requests, 1 QUIT) and one error, which is the
readiness probe's abrupt close.

## 3. Multi-client concurrency and the runtime defect

`client_echo.mink` sends a unique payload per request (`id<thread>-<index>`) and
requires the exact deterministic reply, so a cross-client mix-up cannot pass.
Each worker returns separate counts for connect failures, send failures and
reply mismatches.

| Workload | Result |
|---|---|
| 8 workers, 8 client threads x 50 (400 requests) | 1 of 1 runs clean, handles 84-85 |
| 8 workers, 8 client threads x 100 (800), 4 runs, **before the fix** | 3 of 4 runs failed: 7 mismatches in 3,200 requests |
| same, **after the fix**, 4 runs (3,200 requests) | 0 dial, 0 send, 0 mismatch |
| same, release compiler, 2 runs (1,600 requests) | 0 failures |
| independent Python client, 8 threads x 200 (1,600) | 0 failures, 0.79 s |

Every pre-fix mismatch had the same shape: the payload echoed correctly but the
trailing length digit belonged to another thread, e.g.
`got=OK id7-90 6 want=OK id7-90 1`. The server marked all 401 connections as
successful, so this was not a socket or protocol failure — the integer-to-text
conversion itself was corrupted. Root cause and fix are in section 9.

## 4. Failure recovery

MINK client, 8 threads x 5 rounds, each round: connect and close with no data;
send `ECHO half` with no terminator and close; send an unknown command; send a
short command. Then 200 valid requests on the same server instance.

- 120 explicit `ERR` replies (unknown and short commands) plus 40 abrupt closes
  were handled; no crash, no deadlock, no stale socket state.
- The 200 valid requests after the failures all matched exactly; server exited
  0 with `SERVED 321 ERRORS 121 MUTATIONS 0`.
- A request without a terminator is treated as complete when the peer closes
  (EOF ends the line). That is the probe's protocol choice, not a runtime
  behaviour, and is why the 40 unterminated requests counted as successful.

SQLite service, hostile phase on a fresh database:

- A 5,000-byte `SET` is truncated by the probe's 512-byte read cap and stored
  as 500 bytes; the reply is `OK 1`, a subsequent `GET` returns those 500
  bytes, and the next `SET`/`GET` is served normally.
- A client that connects and sends nothing blocks its read for the probe's 5 s
  readiness timeout, then receives `ERR short`; the service continues (the
  next phase's requests all pass).
- Malformed `SET`/`GET`/`DEL`, an unknown command, and a missing key return
  `ERR usage` / `ERR notfound` and never leave a partial row behind.

## 5. Sustained load

| Workload | Requests | Failures | Wall time |
|---|---|---|---|
| Echo, 1 worker, sequential | 1,200 | 0 | 3.96 s |
| Echo, 8 workers, 8 client threads | 1,600 | 0 | 0.79 s |
| KV, 400-key bulk (400 SET + 800 GET + COUNT) | 1,202 | 0 | 5.61 s (first run) / 7.11 s (400-row DB at start) |
| KV, constant-size churn (300 SET + 300 GET, one row) | 600 | 0 | 3.31 s |
| KV, same-key race (8 threads x 50 SET) | 402 | 0 | 0.64 s |

Across all phases this session served about 11,500 echo requests and about
3,900 key/value requests, every one checked against an exact expected reply.
No server exited non-zero, and no run produced a wrong or partial response
after the fix.

Churn timing was linear (decile marks 0.47, 0.82, 1.14, 1.54, 1.87, 2.15,
2.51, 2.96, 3.07, 3.17 s for 30 request pairs each), so there is no progressive
slowdown on a constant-size database.

## 6. SQLite-backed service

`server_kv.mink` is a local persistent service on the existing APIs only:

```
SET <key> <value>  ->  BEGIN; DELETE old row; INSERT new row; COMMIT; persist
GET <key>          ->  SELECT * FROM kv WHERE k='<key>'
DEL <key>          ->  DELETE FROM kv WHERE k='<key>'; persist
COUNT              ->  SELECT COUNT(*) FROM kv
QUIT               ->  BYE and a clean shutdown
```

Statements are built with `sql_quote`, values are read with `sql_field`, and the
database blob is written to `tmp/m2_p6/kv.db` with `fs_write` after every
mutation. `BEGIN`/`COMMIT` makes the replace atomic: a failed statement rolls
back instead of leaving the key missing. An empty or missing file is a fresh
database (`CREATE TABLE kv (k STR, v STR)`).

Validated:

- **Initialization**: first start creates the schema and writes an empty blob;
  a restart loads the persisted blob and reports the row count through SQL.
- **Repeated requests**: 100 keys (`4 threads x 25`), then 400 keys
  (`4 x 100`), each written, counted and read back exactly; then 300
  overwrites of the same key with per-iteration read-back.
- **Transaction correctness**: every `SET` is a transaction; the 400-key bulk
  and the 400-write same-key race left `COUNT` at 400 and 1 respectively, with
  no duplicate or missing rows.
- **Concurrent access**: 8 client threads opened 400 connections against the
  same key; every request was accepted, the final value was one of the written
  values, `COUNT` stayed 1, and the persisted value matched the final in-memory
  value after restart.
- **Failed request isolation**: malformed requests before and after valid ones
  (section 4) never affected later reads or writes.

## 7. Restart and persistence

| Step | Result |
|---|---|
| Fresh DB, load 10 rows, quit | exit 0, `ROWS 10` |
| Restart same file, verify all 10 | exact values, `ROWS 10`, 0 failures |
| Update all 10, delete the 5 even-index keys, quit | `ROWS 4`, 0 failures |
| Restart same file, verify | `COUNT 4`; the 4 odd-index keys hold the updated values; 0 failures |
| Fresh DB, 8x50 same-key race, quit | `ROWS 1`, final value `t5-49` |
| Restart same file | persisted value `t5-49`, `COUNT 1` |

This is the core M2 P6 result: a MINK program can act as a basic persistent
Windows local service, not only as an isolated one-shot CLI program.

## 8. Resource observations

Sampled every 250 ms with `GetProcessHandleCount` / `GetProcessMemoryInfo`
(`tmp/m2_p6/run_phase.py`):

| Process | Handles | Working set |
|---|---|---|
| Echo, 1 worker, 1,200 requests | 63-64, flat | 4.2 → 5.4 MB, then flat |
| Echo, 8 workers, 1,600 concurrent | 84-91 transient, ends 84 | 4.3 → 6.0 MB |
| KV, 10 rows / 35 requests | 60-61 | 4.3 MB |
| KV, 400-key bulk, 1,202 requests | 60-61, flat | 4.3 → 60.5 MB |
| KV, same 400-key bulk on a pre-existing 400-row DB | 60-61, flat | 4.3 → 75.0 MB |
| KV, constant 1-row churn, 600 requests | 60-61, flat | 4.3 → 5.3 MB |

Interpretation:

- Handles are flat and socket cleanup is complete: no per-connection handle
  growth across thousands of connections, including 121 intentionally failed
  ones. The small transient rise in the 8-worker run is the per-worker
  listener/thread handles.
- There is **no per-request memory leak**: 300 rewrites of a one-row database
  plateau at 5.3 MB, and `rt_exit`'s arena scan reports no `E-R06`.
- Working set grows with the **total database-copy volume**, not with request
  count. `sqlite.mink` copies the whole blob per statement (documented O(N²)
  growth), so a 400-row bulk run commits tens of megabytes of short-lived
  allocations; the non-coalescing first-fit heap keeps that as its high-water
  mark. Running the same bulk against an already-400-row database is slower
  (7.11 s vs 5.61 s) and peaks higher (75.0 MB vs 60.5 MB) because every
  statement copies the full blob from the start. This is a characteristic of
  the existing engine, not a leak (section 10, C1).
- No progressive slowdown and no stale global networking state after failures.

## 9. Defect, root cause, fix, regression test

### A1 (fixed) — concurrent `rt_str_from_int` returned another thread's digits

**Symptom:** 7 wrong replies in 3,200 concurrent requests; only the trailing
length digit differed, and the client's expected value was wrong in the same
way the server's would be if its own text were corrupted.

**Root cause:** `emit_str_from_int` in `src/backend/emit/runtime.rs` built the
decimal digits backwards in `BSS.print_buf`, a process-global 32-byte scratch
area, then allocated a `Str` and copied. Two threads calling it concurrently
overwrote each other's digits between the write and the copy, so one caller
returned the other's number. This is the P5 shared-scratch defect class on a
different buffer, and it is directly relevant to server workloads: the echo
service formats the payload length into every response.

**Fix (minimal):** build the digits in a 32-byte scratch area on the calling
thread's stack instead (`sub_rsp(48)`, cursor at `rbp-17`) and copy from there.
No lock, no ABI change, no allocation change; the two spill slots at
`rbp-8`/`rbp-16` do not overlap the scratch. Negative values and the zero fast
path are unchanged.

**Regression test (permanent):** `tests/threads_lib.rs`,
`s134_concurrent_str_from_int_returns_its_own_value`. Eight threads each
round-trip 4,000 distinct values through `rt_str_from_int`, parse the text back
and count mismatches, then join and assert `0`. On the unmodified emitter the
test fails with 9,579 mismatches out of 32,000; after the fix it passes in both
debug and release builds.

**Workload re-run:** the 800-request concurrent workload that failed 3 of 4
pre-fix runs passed 4 of 4 post-fix runs (3,200 requests), then another 1,600
under the release compiler.

### B1 (recorded, not fixed) — `rt_print_int` still uses the shared scratch

`emit_print_int` (and the float/format paths that share `print_buf`) still
build their text in the same `.bss` buffer. A worker thread that logs an integer
can therefore print another thread's digits. The P6 workload does not print
from workers (servers print one summary line at exit), so this was not
reproduced by the workload and is left for M3. The minimal repair is the same
stack-scratch approach; a print lock is the alternative if call ordering also
matters.

## 10. Classification

Using this session's classes (A correctness, B reliability/resource,
C documented/intentional, D capability gap, E not reproduced):

| # | Finding | Class | Status |
|---|---|---|---|
| A1 | Concurrent `rt_str_from_int` returned another thread's digits | A | **Fixed**, regression test added |
| B1 | `rt_print_int` / float / format paths still share `.bss` `print_buf` | B | Recorded for M3 (not exercised by workers here) |
| C1 | SQLite engine copies the whole blob per statement; service RSS high-water scales with traffic, not live data | C | Documented in `sqlite.mink`; measured here |
| C2 | A service cannot share a `Str` database blob across workers: `rt_ptr_to_int` rejects `Str` (`E-T01`), so one owner thread is required | C (D for M3) | Recorded; single-owner design used |
| C3 | stdlib `fs_write`/`fs_read` consume their `Str` parameters, so a persistent service must hand them copies to keep the blob alive | C | Documented ownership rule; cost one probe iteration |
| D1 | No connect/receive timeout options; a silent peer occupies a worker until the app times out the read with `net_wait_readable` | D | Recorded (P5 candidate) |
| D2 | The probe's 512-byte request cap silently truncates oversized requests | D | Probe design; a real protocol needs a length or size check |
| E1 | Server lifecycle, multi-client concurrency, failure recovery, restarts | E | Works; no crash, deadlock or stale state |
| E2 | Existing HTTP server module (untracked) under its own suite | E | 10/10 passed, untouched |

Mapping to the roadmap's own words: roadmap **A** (MINK defect) = A1;
roadmap **B** (missing capability) = D1; roadmap **C** (architectural
limitation) = C1/C2; roadmap **D** (deliberate difference) = C3; roadmap
**E** (test/application problem) = D2.

## 11. Tests run

| Command | Result |
|---|---|
| `cargo test --test runtime` | 25 passed |
| `cargo test --test strings` | 63 passed (0, ±, i64::MAX paths) |
| `cargo test --test threads_lib` | 18 passed (17 before this session + the new one) |
| `cargo test --test network_lib` | 37 passed |
| `cargo test --test http_lib` | 39 passed |
| `cargo test --test sqlite_lib` | 16 passed |
| `cargo test --test httpserver_lib` | 10 passed |
| `cargo test --release --test threads_lib s134_concurrent_str_from_int` | 1 passed |
| `cargo build --release` | OK |

Probe artifacts live in `tmp/m2_p6/`: `server_echo.mink`,
`client_echo.mink`, `server_kv.mink`, `py_echo_client.py`,
`py_kv_client.py`, `kv_phase.py`, `run_phase.py`, plus generated `*_full.mink`
concatenations, logs and JSON samples. `tmp/` is git-ignored.

Reproduction:

```
rm -f tmp/m2_p6/kv.db
python tmp/m2_p6/run_phase.py --server tmp/m2_p6/server_echo_full.exe \
  --port 12741 --workers 1 --name sustain1200 \
  --client python tmp/m2_p6/py_echo_client.py 12741 seq 1200
python tmp/m2_p6/run_phase.py --server tmp/m2_p6/server_kv_full.exe \
  --port 12750 --workers 1 --name kv1 \
  --client python tmp/m2_p6/kv_phase.py 12750 first
python tmp/m2_p6/run_phase.py --server tmp/m2_p6/server_kv_full.exe \
  --port 12751 --workers 1 --name kv2 \
  --client python tmp/m2_p6/kv_phase.py 12751 restart
```

## 12. Release build status

P5 recorded that `cargo build --release` crashed inside `rustc` on this machine,
including on unmodified HEAD, and used debug builds for verification. That did
**not** reproduce this session: `cargo build --release` finished in 32.16 s,
`cargo test --release --test threads_lib s134_concurrent_str_from_int` passed,
and the release-built `mink.exe` compiled and ran both probes (1,600 concurrent
echo requests and the KV load/restart phases, all clean). The P5 crash is now
recorded as a transient toolchain/environment failure rather than an ongoing
blocker. One transient `LNK1104` (cannot open the `network_lib` test exe) also
appeared during the suite run and passed on an immediate retry; no stray MINK
test process was present.

## 13. Remaining limitations and gaps

- SQLite is a single-owner value engine: no shared handle, no cross-thread
  blob, no file locking, no `fsync`, and the persist step rewrites the entire
  file after each mutation. The zero-row-loss restart result relies on the
  process reaching `fs_write`; a crash between COMMIT and the write loses the
  last mutation. This is acceptable for a capability probe, not a database.
- Memory high-water follows statement traffic (C1), and the non-coalescing heap
  never returns committed pages. A long-running service with a large blob and
  frequent rewrites will hold a working set proportional to its largest burst.
- No shared mutable object model: a multi-worker SQLite service is not
  expressible without a new capability (C2), so the probe serializes requests
  in one owner thread. Concurrency across the network layer was validated with
  the stateless echo service instead.
- A silent client occupies a worker until the app-level read timeout fires
  (D1); sockets still have no connect/read timeout options.
- Probes cap requests at 512 bytes and values cannot contain spaces (D2); the
  untracked HTTP server module is the realistic request parser and was only
  regression-tested, not load-tested.
- `net_dial` error preservation, UDP `net_recvfrom` buffer parity, HTTP global
  last-error state and B1 remain open M3 candidates.

## 14. Repository state and audit

- Intended changes this session: `src/backend/emit/runtime.rs` (one function),
  `tests/threads_lib.rs` (one test), and this document. No stdlib, npm mirror,
  Linux file, Cargo version or tag change.
- Version remains **1.0.4**; tags `v1.0.0`–`v1.0.4` unchanged; no
  npm/release/publish/tag command was run; nothing was committed.
- Pre-existing modified and untracked files (Sessions 128–133 work, the
  roadmap archive, `nl1.*`, the untracked `httpserver` files, the pasted
  transcript) are untouched.
- Probes and generated binaries live under the git-ignored `tmp/m2_p6/`.
- No MINK server, client or fixture process remains running.

## 15. Recommendation for M2 P7

P6 closes the roadmap's "server workloads" item for raw TCP and the
"integrated application" item for SQLite + filesystem + networking. The largest
untested M2 area is now the **existing HTTP server module under concurrent
load**, which is a load test of an already-written module rather than new
subsystem work — followed by the multi-process/pipeline integration item. Carry
these M3 candidates forward in priority order:

1. B1: remove `print_buf` from the integer/float formatting paths (stack
   scratch), so worker-thread logging cannot corrupt digits.
2. D1: connect/receive timeout options, or a documented readiness-loop recipe.
3. C2/D: a way to share an owned value (or a database handle) across threads,
   so SQLite-backed services can use more than one owner worker.
4. P5 carry-overs: `net_dial` error preservation, UDP buffer parity, HTTP
   last-error state.

Do not start M2 P7 until this result is reviewed.
