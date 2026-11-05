# Session 137 — M2 P8.1: Windows defect closure for the two proven P8 defects

**Scope:** close the two proven M2 P8 correctness defects from Session 136 —
the SQLite "heap-built statement misparse" (recorded there as category E) and
the HTTP "truncated Content-Length reported as success" (category A/B) — with a
root cause for each, the smallest safe fix, permanent regression coverage, and
revalidation of the original reproducers, the affected workloads and the
release tree. Windows x86_64 only; Linux frozen; version 1.0.4; no npm,
publish, tag or release activity; no P9 or M3 work started.

**Outcome:** both defects are closed.

1. **Defect 1 is not a MINK or SQLite defect.** Its root cause is the
   reproducer itself: `tmp/m2_p8/sql_probe5.mink` called
   `sql_exec(heap_stmt, db)`, while the documented signature is
   `sql_exec(db, sql)`. The *database blob* was therefore parsed as SQL. Its
   first byte is `T` (the schema record tag) — not a statement keyword — which
   is exactly the recorded `ERROR: unsupported statement`, and the returned
   "database" was the statement text, which is why `sql_rows` then reported
   `-1` ("no such table"). Literal, variable, concatenated, function-produced,
   cross-boundary and 120 length-swept heap-built statements all execute
   identically once the arguments are in the documented order. No product
   change was needed; two permanent regression tests now lock the behavior.
2. **Defect 2 was real and is fixed in `stdlib/http.mink`** (and its
   `npm/mink/stdlib/http.mink` mirror, which stays byte-identical). A
   non-chunked response that delivers fewer body bytes than the Content-Length
   it declared is now `HTTP_ERR_RECV` (-5, the existing "the transfer did not
   complete" code) instead of a completed request. Six permanent regressions
   cover the truncation, the exact control, the declared-shorter-than-sent
   case, the no-Content-Length case, chunked framing (no decode, no false
   rejection) and the HEAD/204 exemptions.

Focused suites: `sqlite_lib` 18 passed, `http_lib` 46 passed. Broader affected
suites: 402 passed, 0 failed. Reduced P8 matrix: 16/16 cases pass. Concurrency:
28 process runs with no divergence. Release: 5/5 representative checks pass.

### Defect 1 at a glance

- **BEFORE:** `tmp/m2_p8/sql_probe5.mink` printed `CONCAT=ERROR: unsupported
  statement` and `ROWS=-1` for the same 34 bytes that executed as a literal.
- **ROOT CAUSE:** the probe called `sql_exec(heap_stmt, db)`; the signature is
  `sql_exec(db, sql)`. The database blob was parsed as SQL (first byte `T` =
  schema tag → "unsupported statement") and the statement text was returned in
  the database position (→ `sql_rows` "no such table"). Not a MINK, runtime or
  SQLite defect; no product change required.
- **CHANGE:** `tmp/m2_p8/sql_probe5_fixed.mink` (same program, call corrected);
  `tmp/m2_p8/sql_probe5.mink` kept intact as the historical artifact. Plus two
  permanent regressions in `tests/sqlite_lib.rs`.
- **AFTER:** `CONCAT=OK`, `ROWS=2`; `sql_probe7` all six storage classes `OK`
  with `ROWS=6`; `sql_probe8` 120/120 heap statements `OK` across the 8-byte
  arena packing.
- **REGRESSION:** `s41_heap_built_statement_matches_its_literal`,
  `s41_heap_statement_length_and_repetition_are_stable` — and the first test
  fails with the exact P8 failure shape when mutated back to the swapped call
  order (§6).

### Defect 2 at a glance

- **BEFORE:** declared `Content-Length: 524288`, 200000 bytes sent → status
  200, body 200000, `http_last_error() == 0` (silent truncation presented as a
  completed request).
- **ROOT CAUSE:** `http_send` had no completeness signal beyond "zero bytes
  arrived"; the declared `Content-Length` was never compared with the received
  body, and at socket level a short body is indistinguishable from a normal
  close-delimited end of response.
- **CHANGE:** `stdlib/http.mink` (+ identical `npm/mink/stdlib/http.mink`): a
  non-consuming single-pass helper `_http_incomplete` and one guarded call in
  `http_send`; `HTTP_ERR_RECV` (-5) keeps its value and gains the new meaning.
- **AFTER:** the P8 case returns status 200, body 200000 and
  `http_last_error() == -5`; exact, no-length, over-declared, chunked, HEAD,
  204 and 304 responses are unchanged; the `TOO_LARGE` (-4) outcome still wins
  when the receive budget filled.
- **REGRESSION:** `h45`–`h50` in `tests/http_lib.rs` (§10), with `h45` verified
  to fail against a reconstructed pre-fix `stdlib/http.mink`.

## 1. Objective

Close the two proven defects from
`docs/implementation/SESSION_136_M2_P8_WINDOWS_LARGE_SCALE.md` §10:

- **Defect 1 — SQLite heap-built statements.** Minimal reproducer
  `tmp/m2_p8/sql_probe5.mink`: identical 34-byte SQL text works as a literal
  and returned `ERROR: unsupported statement` when built by `rt_str_concat`.
  Root cause was recorded as open and had to be established before any edit.
- **Defect 2 — HTTP Content-Length truncation.** Declared 524288, 200000 bytes
  sent: status 200, body 200000, `http_last_error() == 0`. Required behavior:
  an incomplete non-chunked response becomes `HTTP_ERR_RECV`, without falsely
  rejecting chunked or length-less responses.

## 2. Baseline

Recorded before any edit, on a tree that already carried the Sessions 128–135
work (which this session leaves untouched):

| Item | Value |
| --- | --- |
| HEAD | `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d` |
| Version | 1.0.4 (`Cargo.toml`) |
| `git status --short` | 20 modified files + 9 untracked `docs/implementation/SESSION_*` records, `docs/roadmap/archive/`, `stdlib/httpserver.mink`, `npm/mink/stdlib/httpserver.mink`, `tests/httpserver_lib.rs`, `nl1.exe`, `nl1.mink`, one pasted transcript |
| `git diff --stat` | 20 files, 943 insertions, 390 deletions (all pre-session) |
| `cargo build` | success (59 pre-existing warnings) |
| `cargo test --test sqlite_lib --test http_lib --no-fail-fast` | **not captured before the additions — see the deviation note below** |

**Baseline test-count deviation (recorded, not hidden).** The step-0 focused
baseline was recorded as HEAD, version, `git status`, `git diff --stat` and
`cargo build`, but `cargo test --test sqlite_lib --test http_lib` was not run
until after this session's regression tests were appended. The counts that
matter are the post-fix ones (§11). The pre-session counts follow from the
additions and are stated here as arithmetic, not as a run: this file added
exactly 2 tests to `tests/sqlite_lib.rs` and 6 to `tests/http_lib.rs`, so the
pre-session suites held 16 and 40 tests respectively, and P8's record lists
`http_lib 40 passed` at its own baseline, which matches. No test was removed or
weakened in either file.

`tmp/m2_p8/` is git-ignored (`.gitignore:14:tmp/`), so all probe artifacts
written this session stay out of the diff.

## 3. Defect 1 reproduction

`tmp/m2_p8/sql_probe5.mink` built and run against the current tree, exactly as
recorded by P8:

```
CREATE=
OK
|
HEAPLEN=
34
|HEAP=
INSERT INTO items VALUES (2,'v',7)
|
LIT=
OK
|
CONCAT=
ERROR: unsupported statement
|
ROWS=
-1
```

Reproduced verbatim, including `ROWS=-1`. Two details in that output are the
whole case: the heap statement text is exactly the literal text (34 bytes,
printed immediately before the call), and the *query* on the same database
afterwards also fails (`-1` = table not found), even though `CREATE TABLE`
succeeded. A statement-parsing failure alone would not explain the database
losing its schema.

## 4. Defect 1 root cause

`stdlib/sqlite.mink` defines `sql_exec(db: Str, sql: Str)` — database first,
statement second (the same order used by every existing test through the
`ex(db, sql)` helper in `tests/sqlite_lib.rs`). `tmp/m2_p8/sql_probe5.mink`
(and `sql_probe3`, `sql_probe4`, `sql_probe6`) call it the other way round:

```
let b = sql_exec(heap_stmt, db);   // P8 probe: args swapped
```

Both parameters are `Str`, so this type-checks: the *database blob* is bound to
`sql` and the *statement* is bound to `db`. The engine then:

- copies both into the arena (`_arena`), parsing `sql` = the database text;
- `_run` reads the first keyword of the database text: byte 0 is `T`, the
  schema-record tag (`T <1> name <1> ncols <1> cols`), followed by byte 1
  (0x01, not an identifier character), so the keyword is the single letter `T`
  — no statement dispatch arm matches, hence `ERROR: unsupported statement`;
- returns the statement text in the database position, so the subsequent
  `sql_rows(db, "items")` searches the *statement text* for a schema record and
  finds none → `-1`.

This explains all four recorded observations at once, which no string-handling
hypothesis did. Nothing in string construction, length metadata, NUL
termination, the arena packing, or argument hand-off is wrong: the probe simply
called the API with its arguments reversed. P8's §10 "candidates" (the arena
copy path for heap strings, or a codegen/argument-handling interaction) were
both wrong; the swap hypothesis raised in `sql_probe6` was right.

Evidence that the storage class is irrelevant (all with the documented
argument order):

| Probe | What it does | Result |
| --- | --- | --- |
| `tmp/m2_p8/sql_probe5_fixed.mink` | `sql_probe5.mink` with only the call corrected to `sql_exec(db, heap_stmt)` | `CONCAT=OK`, `ROWS=2` |
| `tmp/m2_p8/sql_probe7.mink` | A literal / B heap-built / C variable holding a literal / D concatenated / E function-produced / F copied through a second function boundary — all with the correct order | every class `OK`; `ROWS=6`; `SELECT *` returns all six rows |
| `tmp/m2_p8/sql_probe8.mink` | 120 heap-built statements whose byte lengths sweep the 8-byte arena packing (including exact multiples of 8) | `ITER=120 FAILS=0 ROWS=120` (11,950 statement bytes) |

## 5. Defect 1 fix

**No product change.** The engine, the stdlib and the runtime are correct as
shipped; the defect was in the validation probe's call site. Fixing a
"defect" that does not exist in the product by changing the product would be
the wrong action, and the §5 requirement — that heap/runtime-built statements
behave equivalently to literals — is already satisfied and is now proven and
permanently fenced.

The corrected probe is kept alongside the original:

- `tmp/m2_p8/sql_probe5.mink` — **left byte-for-byte intact** as the historical
  reproducer (per the rules: keep the reproducer, do not weaken it).
- `tmp/m2_p8/sql_probe5_fixed.mink` — the same program with the single call
  corrected; this is the revalidation artifact.

## 6. Defect 1 regression coverage

Two permanent tests in `tests/sqlite_lib.rs` (real `mink build` + native run,
`rt_exit(0)` arena validation, exact `results()` assertions on real data, not
just a clean exit):

| Test | Coverage |
| --- | --- |
| `s41_heap_built_statement_matches_its_literal` | Six storage classes of byte-identical `INSERT` text — literal, variable holding a literal, function-produced heap string, heap variable, concatenation, and a heap string crossing a second function boundary — then `SELECT COUNT(*)` (must be 5), `SELECT * ... WHERE id = 30` (must be the heap-built row) and `SELECT * ... WHERE qty = 7` (all five rows in insertion order) |
| `s41_heap_statement_length_and_repetition_are_stable` | 25 growing heap statements (lengths ~35–60 bytes, crossing the 8-byte arena word packing) plus 10 repetitions of one statement; asserts 25 then 35 rows and all ten repeated rows readable |

**Pre-fix comparison (practical, temporary, reverted).** The permanent test was
mutated to the P8 probe's argument order
(`sql_exec(mk_stmt(20), db)`) and re-run:

```
assertion `left == right` failed: every storage class must execute:
["OK", "OK", "ERROR: unsupported statement", "ERROR: no such table", ...]
```

i.e. the new test reproduces the exact P8 failure shape and fails on it; the
mutation was reverted immediately and the test passes again. There is no
product change whose revert could serve as a before/after, so the mutation is
the honest equivalent.

## 7. Defect 2 reproduction

`tmp/m2_p8/p8_1_defect2.py` drives the unmodified P8 probe
(`tmp/m2_p8/http_scale.mink`, no source changes) against
`tmp/m2_p8/http_fixture.py` on loopback. Before the fix:

| Declared Content-Length | Bytes actually sent | Status | Body | `http_last_error()` |
| --- | --- | --- | --- | --- |
| **524288** | **200000** | **200** | **200000** | **0** |
| 1048576 | 1000 | 200 | 1000 | 0 |
| 65536 | 0 | 200 | 0 | 0 |
| 200000 | 100000 | 200 | 100000 | 0 |
| 200000 | 200000 | 200 | 200000 | 0 (control) |
| 200000 | 300000 | 200 | 300000 | 0 (control) |
| 524288 | 524288 | 200 | 524288 | 0 (control) |

The first four rows are silent data truncation presented as a completed
request, exactly as P8 recorded; the controls establish that the receive path
itself delivers every byte that actually arrives.

## 8. Defect 2 root cause

`http_send` had a single completeness signal: `http_recv_all` returns when the
peer closes, reports `HTTP_ERR_TOO_LARGE` when the receive budget filled while
the peer was still sending, and `http_send` reports `HTTP_ERR_RECV` only when
**zero** bytes arrived. A response whose headers arrived and whose body ended
early therefore looked exactly like a response whose body was complete
followed by an ordinary close: the module never compared the body it received
against the length the response itself declared. `http_recv_all` cannot detect
it — a short body is indistinguishable from a normal end-of-response at the
socket level, because `Connection: close` framing ends the body at EOF. The
information needed (the `Content-Length` header value) is in the received text
and was already parsed elsewhere by `http_content_length`, but that function
consumes the response and was not part of the `http_send` path.

## 9. Defect 2 fix

Smallest safe change, `stdlib/http.mink` only (mirrored to
`npm/mink/stdlib/http.mink`):

- A new internal helper `_http_incomplete(response, is_head) -> (Int, Str)`
  scans the raw response once and returns a verdict, handing the consumed
  response back (the module's V1 ownership style for a helper that must read a
  `Str` and leave it live for the caller — user functions with `Str`
  parameters move them, so a consuming-and-returning helper is the pattern the
  codebase already uses for `sql_rows`/`sql_tables`). It reads with
  non-consuming intrinsics only.
- It reports incomplete **only** when all of these hold: a well-formed
  `Content-Length` header is present; no `Transfer-Encoding` header is present;
  the header block ended (so a body length is comparable); and
  `received_body_bytes < declared`. The comparison is strictly `<`, so a
  response that sends *more* than it declared keeps returning everything it
  sent.
- Exemptions that prevent false rejections: any `Transfer-Encoding` response
  (framing is not Content-Length-delimited — this covers every chunked
  response), any `1xx`/`204`/`304` status, and any reply to a `HEAD` request
  (detected case-insensitively from the request line before the header scan;
  the last two describe a body that is deliberately not sent).
- `http_send` runs the check only when `http_last_error_code == 0` and the
  response is non-empty, so a `TOO_LARGE` (-4) response, a Winsock transport
  failure and the existing empty-response `HTTP_ERR_RECV` all keep their
  existing, more specific outcome.
- `HTTP_ERR_RECV` (-5) keeps its existing value and public contract; its
  documentation was extended from "no response bytes arrived" to "the transfer
  did not complete (no response bytes, or fewer body bytes than declared)".
  The module header's limits list now states the new rule.

Helper unit check against crafted responses (`tmp/m2_p8/http_facts_probe.mink`,
14 cases) before wiring it in: declared-short → 1; exact → 0; no
Content-Length → 0; chunked → 0; HEAD → 0; 204 → 0; 304 → 0; declared-smaller
→ 0; lowercase header name → 1; `Content-Length-Extra` prefix trap → 0;
malformed value → 0; unterminated header block → 0; `0/0` → 0; `declared 0`
with a body → 0. All 14 as designed.

After the fix, the same driver:

| Declared | Sent | Status | Body | `http_last_error()` |
| --- | --- | --- | --- | --- |
| **524288** | **200000** | **200** | **200000** | **−5 `HTTP_ERR_RECV`** |
| 1048576 | 1000 | 200 | 1000 | **−5** |
| 65536 | 0 | 200 | 0 | **−5** |
| 200000 | 100000 | 200 | 100000 | **−5** |
| 200000 | 200000 | 200 | 200000 | 0 (control) |
| 200000 | 300000 | 200 | 300000 | 0 (control) |
| 524288 | 524288 | 200 | 524288 | 0 (control) |

The status code and body still carry the received prefix (`status=200`,
`body=200000`), so callers can inspect the partial transfer; only the error
code changes, from "silent success" to "failed request".

## 10. Defect 2 regression coverage

Six permanent tests in `tests/http_lib.rs`, all driven against raw loopback
servers inside the test binary (deterministic, no internet, one request per
connection, response bytes written verbatim so "declared more than sent" is
expressed by the bytes themselves):

| Test | Case | Assertion |
| --- | --- | --- |
| `h45_client_get_truncated_content_length_reports_error` | P8's exact sizes: declared 524288, sent 200000 | `err == -5`, body 200000, first/last bytes match the pattern |
| `h46_client_get_exact_content_length_completes` | declared 200000, sent 200000 | `err == 0`, body 200000, bytes match |
| `h47_client_get_declared_shorter_than_sent_is_unchanged` | declared 100, sent 200 | `err == 0`, body 200 (existing semantics preserved) |
| `h48_client_get_without_content_length_is_unchanged` | no `Content-Length`; plus a `204` with `Content-Length: 9` | both `err == 0`, bodies 5 and 0 |
| `h49_client_get_chunked_response_stays_successful` | `Transfer-Encoding: chunked` framing (13 bytes, undecoded), plus the same framing with an inconsistent `Content-Length: 99` | both `err == 0`, body 13 with framing bytes intact |
| `h50_client_head_without_body_stays_successful` | `HEAD` reply with `Content-Length: 5` and no body, then a `GET` with the same header that does send the body | `err == 0` for both, GET body 5 |

**Before/after at test level:** h45 against a reconstructed pre-fix
`stdlib/http.mink` (this session's additions removed, verified by
`grep -c _http_incomplete` → 0) fails — exit 106 instead of 45. The 106 is the
program's own failure path returning before releasing the body, which the
runtime reports as `E-R06: memory leak: live allocations remain at exit`; a
direct build of the same source prints `ERR=0 BODY=200000`, i.e. the defect
itself. The pre-fix file was immediately replaced by the fixed one
(`diff stdlib/http.mink npm/mink/stdlib/http.mink` → identical).

`h43` (full 4 MiB body + overflow `TOO_LARGE`), `h44` (peer closes without a
response → −5) and all other pre-existing HTTP tests still pass.

## 11. Focused tests

| Command | Result |
| --- | --- |
| `cargo test --test sqlite_lib --test http_lib --no-fail-fast` | **sqlite_lib 18 passed, http_lib 46 passed, 0 failed** |
| `cargo test --test runtime --test strings --test filesystem_lib --test json --test process_lib --test network_lib --test http_lib --test sqlite_lib --test threads_lib --no-fail-fast` | **402 passed, 0 failed** (runtime 25, strings 63, filesystem_lib 46, json 63, process_lib 86, network_lib 37, http_lib 46, sqlite_lib 18, threads_lib 18) |

No test was skipped, deleted or weakened; the two SQLite and six HTTP tests
above are additions.

## 12. Original reproducer revalidation

| Reproducer | Before | After |
| --- | --- | --- |
| `tmp/m2_p8/sql_probe5.mink` | `CONCAT=ERROR: unsupported statement`, `ROWS=-1` | **Unchanged, deliberately**: the file is preserved as the historical artifact. Its failure is fully explained and is a probe defect (argument order), not a product defect (§4) |
| `tmp/m2_p8/sql_probe5_fixed.mink` (same program, call corrected) | (did not exist) | `CREATE=OK`, `LIT=OK`, `CONCAT=OK`, `ROWS=2` |
| `tmp/m2_p8/http_scale.mink` on `/truncate?declared=524288&sent=200000` | status 200, body 200000, `http_last_error() == 0` | status 200, body 200000, `http_last_error() == -5` (`HTTP_ERR_RECV`), and the request is not reported as completed |
| same probe on `/truncate?declared=524288&sent=524288` (control) | status 200, body 524288, err 0 | unchanged |

The SQLite exit gate item "the original reproducer must no longer produce
`ERROR: unsupported statement`" is satisfied in substance and reported
precisely here: the *program under test* — a heap-built statement executing
against its database — no longer produces that error in any form
(`sql_probe5_fixed`, `sql_probe7`, `sql_probe8`, both new permanent tests, the
integrated workload and the release tree). The preserved historical file still
prints it because it feeds the database text to the statement parser; that is
the defect's definition, not an unresolved product behavior. Nothing about the
outcome is being claimed that the evidence does not show.

## 13. Integrated workload revalidation

`tmp/m2_p8/p8_1_matrix.py` (reduced but meaningful P8 matrix; one or two
representative levels per subsystem, each independently verified in Python):

| Subsystem | Case | Result |
| --- | --- | --- |
| Filesystem | 32 MiB read / exact length / XOR / SUM / first / last / write copy / `fs_file_size`, SHA-256 of source vs copy | **PASS** — len 33,554,432, sum 4,278,190,080 (recomputed), SHA equal, 0.23 s |
| Strings/heap | 500,000 `lifo` cycles of 1 MiB + 64 KiB alloc/stamp/verify/free | **PASS** — `BAD 0`, deterministic hash, 0.91 s |
| JSON | 511 B and 1023 B documents, documented 64 KiB arena | **PASS** — parsed 512/1024, tag 4, n 255/511, first 0, last 4/0 (all recomputed) |
| HTTP | 64 KiB / 256 KiB / 1 MiB / 4 MiB exact bodies | **PASS** — status 200, exact lengths, FNV-1a of the body recomputed and equal, err 0 |
| HTTP | truncated 524288/200000 | **PASS** — err −5 with body 200000 |
| HTTP | exact 524288/524288 control | **PASS** — err 0 |
| HTTP | chunked framing (endpoint added to the P8 fixture for this matrix) | **PASS** — err 0, body 13 bytes, framing untouched |
| Process | 1 MiB / 4 MiB / 8,388,600 B stdout capture | **PASS** — exact lengths |
| Process | 8,388,601 B | **PASS** — loud `E-R02`, exit 102, unchanged |
| SQLite | literal / heap-built / concatenated / function-produced | **PASS** — all `OK` |
| SQLite | 20 repeated heap-built statements | **PASS** — 0 failures |
| SQLite | transaction path: `BEGIN`/`INSERT`/`COMMIT` then `BEGIN`/`INSERT`/`ROLLBACK` | **PASS** — 24 → 25 → 25 rows |
| Mixed | 500-iteration six-subsystem workload (`mixed_driver.py`, every iteration checked against an independent Python computation) | **PASS** — `iterations_run=500 mismatches=0` (exit 0) |

16 cases, 16 pass. The matrix runs in a few minutes rather than the P8
campaign's hours: it is regression confidence for the two fixes, not a repeat
of the scale campaign.

## 14. Release verification

`cargo build --release` — success (release tree already current; no source
outside `stdlib/` and `tests/` changed, so no compiler rebuild was required).

`tmp/m2_p8/p8_1_release.py` rebuilds the representative probes with
`target/release/mink` and requires the same corrected behavior:

| Release check | Result |
| --- | --- |
| SQLite heap-built statement (plus concat, function-produced, 20 repeats) | **PASS** — heap `OK`, concat `OK`, func `OK`, 0 repeat failures, 24 rows |
| HTTP truncated 524288/200000 | **PASS** — status 200, body 200000, err **−5** |
| HTTP exact 524288 control | **PASS** — err 0, body 524288 |
| HTTP valid 1 MiB body | **PASS** — status 200, exact length, FNV-1a recomputed and equal, err 0 |
| 32 MiB file read/write/hash | **PASS** — len 33,554,432, SHA-256 equal, 1.18 s |

## 15. Defect classification

| Defect (SESSION_136) | Class then | Class now | Basis |
| --- | --- | --- | --- |
| SQLite heap-built statement misparse | E — genuine defect, root cause open | **NOT A DEFECT — invalid reproducer (probe argument order)** | Root cause established from source and confirmed by six storage classes executing identically (§3, §4); permanent fence added (§6). SESSION_136 is not rewritten; the historical record stands and this session records the closure with its root cause |
| HTTP truncated Content-Length reported as success | A/B — silent correctness defect | **CLOSED** | Fixed in `stdlib/http.mink` + npm mirror; original reproducer now reports `HTTP_ERR_RECV`; six permanent regressions; chunked/no-length/over-declared/HEAD/204/304 all unchanged (§7–§10) |

Session 136's third finding (sustained mixed workload reaches the 1 GiB heap
ceiling with an explicit `E-R02`) stays **B — deliberate allocator behavior**;
the reduced 500-iteration mixed workload ran clean, which is consistent with
P8's per-iteration growth model and is not a change in that ceiling.

## 16. Remaining M2 gaps

Unchanged, deliberately not redesigned in this session:

- JSON arena behavior (documented 64 KiB arena), including the silent
  serialization geometry found by P4/P8 — a JSON design decision.
- Allocator no-split/no-coalesce fragmentation ceiling (`E-R02`, explicit).
- HTTP 4 MiB body budget (`MAX_BODY()` + 8 KiB receive budget, loud −4 above).
- stdio 8 MiB − 8 payload window (8,388,600 B usable, loud `E-R02` above).
- Socket connect/receive timeout API gap (P5).
- P6's shared-scratch `rt_print_int` note.
- HTTP V1 still does **not** decode chunked transfer encoding: a chunked body
  is returned framed, and (documented in the new header note) a
  Transfer-Encoding response is deliberately exempt from the completion check.
- `http_content_length`/`http_header` remain consuming helpers (unchanged
  public API shape); the new check uses its own non-consuming scan.

## 17. Audit

- `stdlib/http.mink` and `npm/mink/stdlib/http.mink` are byte-identical
  (`diff` empty) after the fix.
- Changed files this session: `stdlib/http.mink`, `npm/mink/stdlib/http.mink`,
  `tests/http_lib.rs`, `tests/sqlite_lib.rs`, this record. No runtime, emitter,
  compiler, Cargo, version, tag, npm-package, Linux or roadmap change; no
  unrelated cleanup; no test skipped or weakened; no binary or generated
  artifact added to the tree.
- All probes, fixtures and logs live under the git-ignored `tmp/m2_p8/`
  (`git check-ignore` → `.gitignore:14:tmp/`).
- Pre-existing modifications (Sessions 128–135 work, `docs/roadmap/archive/`,
  `stdlib/httpserver.mink` + `npm/mink/stdlib/httpserver.mink` +
  `tests/httpserver_lib.rs`, `nl1.exe`, `nl1.mink`, the pasted transcript) were
  not touched and remain in the working tree exactly as found.
- No server, fixture, probe or client process left running; the one matrix run
  that overran its timebox was stopped by PID and verified gone.
- Nothing committed, no tag, no publish, no version change.

## 18. Next

M2 is ready for the P9 final gap-closure/audit session. Recommended order for
P9 (unchanged from P8's recommendation, minus the two closed items):

1. Re-run the P1–P7 suites plus this session's additions on the fixed tree and
   confirm the evidence matrix.
2. Decide whether the JSON arena needs a reset/capacity API (a JSON design
   session, not a P9 fix) — the mixed workload's per-iteration arena
   allocation is what reaches the heap ceiling.
3. Carry the documented ceilings (allocator coalescing, 4 MiB HTTP budget,
   stdio window, socket timeouts, chunked decoding) into M3 as design items.

Do not start P9 automatically.

## 19. Session inventory (git-ignored probes added this session)

| Probe | Purpose |
| --- | --- |
| `sql_probe5_fixed.mink` | `sql_probe5.mink` with the corrected `sql_exec(db, sql)` order |
| `sql_probe7.mink` | six storage classes of byte-identical SQL text |
| `sql_probe8.mink` | 120 heap-built statements sweeping statement length/word packing |
| `sql_probe9.mink` | literal/heap/concat/function, 20 repeats, transaction commit+rollback |
| `http_facts_probe.mink`, `http_facts_dbg.mink` | 14 crafted-response unit checks of the completion helper |
| `http_trunc_probe.mink` | minimal truncation probe for the concurrency check |
| `p8_1_defect2.py` | the P8 truncation table (before/after), P8 probe unchanged |
| `p8_1_matrix.py` | the reduced 16-case P8 matrix |
| `p8_1_concurrency.py` | 10 sequential + 4 concurrent instances per fixed path |
| `p8_1_release.py` | release-built representative verification |
| `http_fixture.py` (extended) | `/chunked` endpoint added; existing endpoints unchanged |
| `logs/p81_*.log`, `logs/session137_broad.log` | raw evidence |

## 20. M2 P8.1 exit gate

| Gate | Status | Evidence |
| --- | --- | --- |
| SQLite original reproducer fixed | **YES (as a probe defect)** | §3/§4/§12: root cause is the probe's argument order; the corrected probe and all equivalence probes pass |
| SQLite root cause identified | **YES** | §4 — reproduced from source, explains all four recorded symptoms |
| SQLite permanent regression added | **YES** | 2 tests, `tests/sqlite_lib.rs` §6 |
| SQLite focused suite passes | **YES** | 18 passed |
| HTTP original truncation reproducer fixed | **YES** | §7/§9/§12 — `HTTP_ERR_RECV`, no longer reported as success |
| HTTP Content-Length regression added | **YES** | `h45`–`h50` §10 |
| HTTP chunked behavior remains correct | **YES** | `h49` + matrix `/chunked` case + 14-case helper check |
| HTTP focused suite passes | **YES** | 46 passed |
| Broader affected suites pass | **YES** | 402 passed, 0 failed §11 |
| Integrated workload passes | **YES** | 16/16 matrix cases §13 |
| Release build passes | **YES** | `cargo build --release` §14 |
| Release representative verification passes | **YES** | 5/5 §14 |
| SESSION_137 record complete | **YES** | this document |
| `git diff`/`git status` audited | **YES** | §17, empty `git diff --check` |
| No unrelated modifications | **YES** | only `stdlib/http.mink`, `npm/mink/stdlib/http.mink`, `tests/http_lib.rs`, `tests/sqlite_lib.rs`, this record; HEAD/version unchanged |
| No stray processes | **YES** | no probe/fixture process left; the one overrun run was stopped by PID and verified gone |

**STATUS = COMPLETE.** Both proven M2 P8 defects are closed with permanent
regression coverage and revalidated reproducers. M2 P9 is not started; nothing
is committed, tagged, published or versioned by this session.
