# Session 135 — M2 P7: Windows real-world application workloads

**Scope:** answer "can MINK reliably build and run useful Windows-native software
when multiple subsystems are combined?" with five integrated workloads (file
pipeline, developer automation, SQLite CLI, network client, concurrent
processing), each run repeatedly and, for a representative pair, distributed as
a standalone executable. Validation-first; Linux was not touched; the version
stays 1.0.4.

**Outcome:** all five workloads pass, repeatably, in debug and release builds.
The session also found two defects at subsystem boundaries. One was in the P5
regression test itself: its fixture answered before the request arrived because
accepted Windows sockets inherit the listener's non-blocking mode, which made
`network_lib` fail roughly one run in two. The other was a real HTTP client
contract gap: a peer that closes or aborts without a status line produced an
empty response with `http_last_error() == 0`, although the module documents 0
as "completed request". Both are fixed, with the fixture corrected and a new
`HTTP_ERR_RECV` (-5) plus a permanent regression test. No runtime/emitter change
was needed.

## 1. Baseline

Read before probing: Sessions 133/134 records, `stdlib/network.mink`,
`stdlib/http.mink`, `stdlib/sqlite.mink`, `stdlib/threads.mink`,
`stdlib/filesystem.mink`, `stdlib/hashing.mink`, `stdlib/re.mink`,
`stdlib/process.mink`, and the existing validation programs (`examples/`,
`tests/httpserver_lib.rs`).

Focused suites at the start of this session:

| Command | Result |
|---|---|
| `cargo test --test runtime` | 25 passed |
| `cargo test --test strings` | 63 passed |
| `cargo test --test threads_lib` | 18 passed |
| `cargo test --test network_lib` | **36 passed, 1 failed** — `n100_concurrent_implicit_net_init_is_serialized`, run 5, exit 2 |
| `cargo test --test http_lib` | 39 passed |
| `cargo test --test sqlite_lib` | 16 passed |
| `cargo build --release` | OK |

The single failure is investigated in section 7; it was not a runtime
regression, and it was reproducible (1 of 2 subsequent runs failed; replayed
standalone, the same program failed 2 of 25 and 7 of 50 runs).

## 2. Workload A — file/asset pipeline

**Purpose:** recursive traversal, metadata, hashing, text reporting across
nested directories, with a canonical order that does not depend on the
filesystem's enumeration order.

**Input:** `tmp/m2_p7/treeA` — 25 files, 3,362 bytes, five levels deep:
`docs/`, `docs/notes/deep/`, `src/lib/`, `assets/sub/deep/`, `data/`. It
includes an empty file, six binary files with NUL bytes, and a file name with
spaces.

**Program:** `tmp/m2_p7/workload_a.mink` (filesystem + hashing). A recursive
walk collects relative paths, sorts them by byte order (insertion sort over
offsets in one string), reads each file, and emits
`<sha256> <bytes> <relative path>` followed by `COUNT 25 BYTES 3362`.

**Result:** byte-for-byte equal to the report computed independently in Python
with `hashlib`, and exit 0 with the runtime's arena scan clean. The report
order is the same on every run even though the directory API documents
"entries are not ordered".

**Scale/limits exercised:** recursion, empty input allocation, names with
spaces, binary content in SHA-256, and 25-file sorting.

## 3. Workload B — developer automation tool

**Purpose:** argv parsing, traversal, line counting, regex matching, hashing,
subprocess execution and structured output in one utility.

**Input:** `tmp/m2_p7/treeB` — 13 files, 89 lines, 13 `TODO`, 3 `FIXME`,
16 combined tags.

**Program:** `tmp/m2_p7/workload_b.mink` (filesystem + hashing + re + process).
For each file it prints
`<sha256> <lines> <todos> <fixmes> <tags> <path>`, then
`FILES 13 LINES 89 TODO 13 FIXME 3 TAGS 16`, then runs `cmd /c ver` and reports
`CHILD exit=0 has=1`. TODO/FIXME use the regex engine (including the alternation
`TODO|FIXME` for the combined column); the content crosses strings → regex →
hashing → process capture in one request path.

**Result:** per-file and summary lines match the independent Python
computation exactly; the child's exit status and captured stdout are correct;
exit 0, no leaks.

## 4. Workload C — SQLite local tool

**Purpose:** a build-and-run utility that combines filesystem, SQLite,
transactions, updates/deletes, persistence and a restart.

**Input/formula:** 500 rows `(id, 'n<id>', id*3)` inserted inside one
`BEGIN`/`COMMIT`; `UPDATE ... SET qty = 777 WHERE id < 50`; a
`BEGIN`/`INSERT`/`ROLLBACK` probe (row 999 must not survive); `DELETE ...
WHERE id >= 480`; the blob is written to `tmp/m2_p7/c.db` (10,040 bytes).

**Program:** `tmp/m2_p7/workload_c.mink` (filesystem + hashing + sqlite),
`write` and `verify` modes.

**Result:** `write` (1.92 s) prints

```
COUNT 480
HEAD e73e70c10a51a723f0e64c70da1c3c3d7046edfd60090cd8330ea2904dfd889f
ROWS 480 SUM 380055
```

and the separate `verify` process on the persisted file prints the identical
three lines. The HEAD digest and the sum were recomputed independently
(`sha256("0\tn0\t777\n...\n4\tn4\t777\n")`, `50*777 + 3*sum(50..479)`), and the
rollback probe left no row. Transaction, update, delete, persistence, restart
and the SQLite → string boundary all hold.

**Limitation:** this is the existing value-transform engine: one owner, whole
blob copied per statement, whole file rewritten on persist (section 9).

## 5. Workload D — network client

**Purpose:** hostname HTTP, request construction, response parsing, hashing,
error handling, and recovery after failure.

**Input:** a deterministic raw fixture on `127.0.0.1` serving an 84-byte body
(`"P7DATA-" * 12`) at `/data` and closing without a response at `/close`; the
"refused" port is `fixture_port + 1`.

**Program:** `tmp/m2_p7/workload_d.mink` (http + hashing). Sequence:

1. `GET /data` → status 200, 84 bytes, SHA-256 of the body.
2. `GET /close` → empty body, explicit error.
3. connect to the refused port → empty body, explicit error.
4. `GET /data` again → identical status/length/digest.

**Result:** exactly

```
GET1 status=200 len=84 sha=d991c137a67956c0472069b7ccc90aaed4efd5c7420b4329ede1214d0ba48f7d
CLOSE len=0 err=-5 kind=recv
REFUSED len=0 nonzero=1
GET2 status=200 len=84 sha=d991c137a67956c0472069b7ccc90aaed4efd5c7420b4329ede1214d0ba48f7d
```

The digest equals Python's `hashlib.sha256(b"P7DATA-" * 12)`. Step 2 is the
new receive-error path from section 8; step 3 shows the connect path still
reports a failure; step 4 proves recovery.

## 6. Workload E — concurrent processing

**Purpose:** threads + filesystem + hashing + locking + formatted output, with
unique per-thread data and an order-independent aggregate, specifically as
follow-up evidence after the P6 `emit_str_from_int` concurrency defect.

**Input:** `tmp/m2_p7/treeE` — 8 directories × 16 files = 128 files with
distinct content.

**Program:** `tmp/m2_p7/workload_e.mink` (filesystem + hashing + threads).
Each of 8 threads reads and hashes its 16 files, formats `"<t> <i> <sha256>"`
with `rt_str_from_int`, folds the first 15 hex digits into a shared XOR and a
line counter under a `lock_new`/`lock_acquire`/`lock_release` mutex, and prints
the line while holding the lock. Main prints
`WORKERS 8 LINES 128 XOR 1055109961331643210 FAILS 0`.

**Result:** all 128 lines are whole (no interleaving), equal as a set to the
independent per-file computation, and the XOR matches Python; `FAILS 0`; exit
0; no deadlock, crash or lost counter update across repeated runs (5 debug +
5 release). Thread scheduling changes line order, so the check is
multiset-based plus the commutative aggregate — the report content is
deterministic even though the order is not.

## 7. Defect 1 — the P5 regression test fixture (test defect, E)

**Symptom:** `network_lib::n100_concurrent_implicit_net_init_is_serialized`
failed intermittently with exit code 2 from the MINK program (two body checks
failed on one empty response). Observed 2 of 3 runs, and 2 of 25 runs / 7 of 50
runs when the generated program was replayed against a standalone copy of the
same Rust fixture.

**Diagnosis, step by step:**

1. Instrumented HTTP program: every failure is an empty body, and the global
   `http_last_error()` was 0 (which is itself defect 2).
2. Low-level TCP program (dial/send/recv per request, recording
   `net_last_error()` immediately after the empty receive): `err=10053`
   (`WSAECONNABORTED`) with the full 59-byte request sent.
3. A logging copy of the fixture showed the trigger:
   `read=err:10035 (WouldBlock) bytes=0` on some accepted connections.
   **Windows makes an accepted socket inherit the listener's non-blocking
   mode.** The fixture's listener is non-blocking (for its accept poll loop),
   so `stream.read` could return `WouldBlock` before the request arrived; the
   fixture ignored the error, wrote a response and closed with the request
   still in flight. The client then legitimately saw an aborted connection.
4. Control experiment: the same fixture with
   `stream.set_nonblocking(false)` after `accept` ran 50/50 clean (4,000
   requests). The non-blocking variant failed 7/50.

**Fix (test-only):** `tests/network_lib.rs` sets the accepted stream back to
blocking with a comment explaining the inheritance. No assertion was weakened;
the test still runs 25 processes × 8 threads × 10 requests and requires exit 0.

**Verification:** the single test passed 5/5 runs (10,000 requests, versus a
~50% failure rate before), and the full `network_lib` suite passed 37/37.

## 8. Defect 2 — empty HTTP response reported success (B, fixed)

**Symptom:** a peer that accepts and then closes or aborts before sending a
status line yields an empty `http_client_get` result, but
`http_last_error()` stayed 0. The module header states the opposite: "0 after a
completed request" and "an empty string ... never means 'successful empty
response' — it means the request failed".

**Root cause:** `http_recv_all` treats an empty `net_recv` as EOF regardless of
whether it was a graceful close or a receive error, and `http_send` only set
`http_last_error_code` for socket/connect/send/too-large failures. A reset
therefore left the previous value, normally 0.

**Fix:** add `HTTP_ERR_RECV() -> Int { return -5; }` and, in `http_send`, set it
when the received response is empty. Applied identically to `stdlib/http.mink`
and its `npm/mink/stdlib/http.mink` mirror, and the header's failure list now
includes "an empty/aborted transfer".

**Regression test:** `tests/http_lib.rs::h44_client_get_closed_without_response_reports_error`
— a fixture accepts and drops the connection without responding; the program
requires an empty body **and** `http_last_error() == -5`. It passes, and
Workload D proves the same path end-to-end against a real fixture.

**Scope note:** a response that is cut off after a status line (a truncated
non-empty body) is still not validated against `Content-Length`; only the
zero-byte case is closed here (section 10).

## 9. Repeatability

`tmp/m2_p7/p7_suite.py` runs all five workloads and compares each against its
independent Python computation.

| Build | Checks | Result |
|---|---|---|
| debug compiler | 5 workloads × 5 runs = 25 | 25 pass, 0 fail |
| release compiler | 5 workloads × 5 runs = 25 | 25 pass, 0 fail |
| standalone distribution (A/B from `tmp/m2_p7/dist`) | 5 runs each | 5/5 pass, byte-identical outputs (one MD5 per workload across runs) |

Workload C is repeated as five fresh `write` + separate `verify` cycles (the
write is not idempotent on an existing database, so each cycle starts from a
removed file). Workload E's line order varies with scheduling; its line
multiset and XOR aggregate are identical on every run.

## 10. Distribution check

`workload_a_full.exe` and `workload_b_full.exe` were copied to
`tmp/m2_p7/dist/`, then run five times each from that directory with only the
input tree path (`../treeA`, `../treeB`) — no compiler and no environment
setup. All ten runs exited 0 with byte-identical reports (five runs share one
MD5 per workload). The programs are standalone PEs; the only external
dependency is the child `cmd /c ver` that Workload B deliberately runs.

Release compilation was healthy this session: `cargo build --release` succeeded,
the release compiler built all five workloads, and the release suite passed
25/25.

## 11. Classification

| # | Item | Class | Status |
|---|---|---|---|
| 1 | P5 `network_lib` fixture answered a not-yet-arrived request (inherited non-blocking) | E (test/application problem) | **Fixed** (test-only) |
| 2 | Empty/aborted HTTP transfer left `http_last_error() == 0` | B (reliability/diagnostics) | **Fixed**, `HTTP_ERR_RECV` + h44 |
| 3 | Truncated non-empty HTTP body is not checked against `Content-Length` | B | Recorded for M3 |
| 4 | SQLite is single-owner, copies the blob per statement, persists the whole file | C (documented limitation) | Measured in P6; used as designed |
| 5 | No socket connect/receive timeout options | D (capability gap) | Carried from P5 |
| 6 | `rt_print_int`/float formatting still share `.bss` `print_buf` (P6 B1) | B | Not exercised here; carried for M3 |
| 7 | Workloads A–E | E (works / evidence strengthened) | 25/25 debug, 25/25 release |

No missing feature was labelled a defect; the two fixes each close a violation
of an existing documented contract (the test's own contract, and HTTP's failure
model).

## 12. Changes

- `tests/network_lib.rs`: fixture sets the accepted stream to blocking
  (test-only; no assertion change).
- `stdlib/http.mink` and `npm/mink/stdlib/http.mink`: `HTTP_ERR_RECV` (-5) and
  the empty-response check; header comment updated. Both copies are identical.
- `tests/http_lib.rs`: `h44_client_get_closed_without_response_reports_error`.
- This document.
- No runtime/emitter, Cargo, version, tag, npm-package or Linux change.

Probes: `tmp/m2_p7/` (ignored) — `gen_inputs.py`, `workload_[a-e].mink`,
`*_full.mink`, `p7_suite.py`, `http_data_fixture.py`, the diagnostic
`http_race.mink`, `raw_race.mink`, `n100_diag.mink`, `rust_fixture*.rs`,
generated trees, `dist/`, and logs.

## 13. Tests run after the fixes

| Command | Result |
|---|---|
| `cargo test --test network_lib` | 37 passed, 0 failed |
| `cargo test --test http_lib` | 40 passed, 0 failed (39 + h44) |
| `cargo test --test network_lib n100_concurrent` (5 runs) | 5/5 pass |
| `python tmp/m2_p7/p7_suite.py 5` (debug) | 25/25 pass |
| same with release-built workloads | 25/25 pass |
| distribution runs (A/B from `dist/`) | 5/5 each, identical output |
| `cargo build --release` | OK |

The other baseline suites (runtime 25, strings 63, threads_lib 18,
sqlite_lib 16) were not touched by the changes and were not re-run.

## 14. Remaining limitations

- SQLite: one owner thread, whole-blob copies per statement, whole-file
  rewrite per mutation, no fsync; a crash between `COMMIT` and the file write
  loses the last mutation.
- HTTP: only the zero-byte failure is detected; a body truncated after the
  headers is returned as-is, and `Content-Length` is not enforced.
- Sockets still have no connect/receive timeout options; a silent peer needs an
  app-level `net_wait_readable` loop.
- P6's `rt_print_int` shared-scratch issue (B1) is still open; workloads here
  print from one thread under a lock.
- The workloads are single-machine, local-loopback, files of a few kilobytes;
  large-file/large-body scale remains an M2 item.
- `tmp/m2_p7` diagnostics include deliberate fixture reproductions; they are
  probes, not committed tests.

## 15. Repository state and audit

- Version 1.0.4; tags `v1.0.0`–`v1.0.4`; HEAD still `bb3ff1e`; no
  npm/release/publish/tag command; nothing committed.
- Linux files untouched; no `Cargo.toml`/package version change.
- Probes are under the git-ignored `tmp/m2_p7/`.
- Pre-existing modified and untracked files (Sessions 128–134 work, the
  roadmap archive, `nl1.*`, the untracked `httpserver` files, the pasted
  transcript) remain untouched.
- No workload server, fixture or client process is left running.

## 16. Recommendation for M2 P8

The integrated-application and subsystem-boundary work is in good shape. The
remaining M2 coverage gap is **scale**: files above 4 MiB, large JSON, large
HTTP bodies and long-running stability, where the P6 SQLite memory high-water
and the HTTP receive budget interact. Recommend M2 P8 as a scale/limits
session (large files, large structured data, sustained daemon run), with these
M3 candidates carried forward in priority order:

1. B1: stack-scratch `rt_print_int`/float formatting, so worker logging cannot
   corrupt digits (P6 evidence).
2. HTTP: validate a received body against `Content-Length` so truncation is
   detected, not just a zero-byte response (this session's defect 2 remainder).
3. Socket connect/receive timeout options or a documented readiness recipe.
4. A way to share owned values across threads (SQLite service workers).

Do not start M2 P8 until this result is reviewed.
