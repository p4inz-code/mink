# Session 138 — M2 P9: final Windows gap-closure and audit

**Scope:** final M2 Windows x86_64 practical-use audit: revalidate the complete
M2 evidence set, classify every remaining gap, close only proven
correctness/reliability defects with a minimal safe change and permanent
regression coverage, and decide whether M2 can be closed. Linux was not
touched; the version stays 1.0.4; nothing was committed, tagged, published or
version-bumped; M3 was not started.

**Outcome:** four correctness defects are closed. Two were known
(Session 129's if-expression-in-operand compiler panic; the Session 136 HTTP
truncation was already closed in Session 137 and stays closed). Two were
proven in this audit: a nested `else if` **expression** silently produced the
zero initializer (pre-existing HIR lowering bug), and the JSON serializer's
fixed output base silently corrupted serialization in an enlarged arena, with
a related silent parse corruption from the library's fixed write-pointer slot.
All four now have root causes, minimal fixes and permanent regressions. The
complete M2 evidence set is green, the release build and representative
release workloads pass, and **M2 is CLOSED**.

## 1. Objective

Perform the final M2 gap audit on Windows x86_64:

1. establish a clean baseline (build + complete Rust regression);
2. build the remaining-gap inventory from Sessions 125–137 and classify each
   item A–F;
3. apply the criticality gate (silent corruption, false success, crash,
   deadlock, practical-workload failure, explicitness, API gap);
4. revalidate JSON, allocator, process I/O, HTTP, networking,
   server/concurrency, SQLite and a representative application matrix;
5. close only proven correctness/reliability defects, with a minimal fix and a
   permanent regression;
6. re-run the complete suite, build release, run representative release
   workloads, and decide M2 closure from evidence.

Not in scope: M3, Linux, package/ecosystem work, socket-timeout API, JSON or
allocator redesign, version/publish/tag changes, speculative language work.

## 2. Baseline

Recorded before any edit, on the tree carrying the uncommitted Sessions
128–137 work:

| Item | Value |
| --- | --- |
| HEAD | `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d` |
| Branch | `main` |
| Version | **1.0.4** (`Cargo.toml`; npm package untouched) |
| `git status --short` | 20 modified files + the untracked Session 128–137 records, `docs/roadmap/archive/`, `stdlib/httpserver.mink`, `npm/mink/stdlib/httpserver.mink`, `tests/httpserver_lib.rs`, `nl1.exe`, `nl1.mink`, and a pasted transcript |
| `git diff --check` | clean |
| `cargo build` | success (59 pre-existing warnings; no errors) |
| Rust | stable MSVC toolchain already used by Sessions 129–137 |
| `cargo test --no-fail-fast` | **76 targets: 3043 passed, 18 failed, 1 ignored**, exit 101 |
| Failed target | `tls_lib` only: `1 passed; 18 failed; 1 ignored` |

The `tls_lib` failures are the known environment issue: the harness spawns
`python3`, which on this machine is the Microsoft Store alias stub. With a
`python3.exe` shim (a copy of the working `python` 3.11 interpreter) prepended
to `PATH`, the same target reports **19 passed, 0 failed, 1 ignored**. No
product failure exists in the baseline.

Pre-existing untracked artifacts (`nl1.exe`, `nl1.mink`, the pasted
transcript, the untracked `httpserver` module and its suite) were not created,
modified or deleted by this session.

## 3. M2 remaining-gap inventory

Built from Sessions 125–137 and re-checked against the current source/tests.

| # | Item | Class before | Evidence this session | Final class / status |
| --- | --- | --- | --- | --- |
| 1 | JSON fixed 64 KiB arena and serializer geometry | D (P4), D (P8) | Silent wrong serialization reproduced (4091 B → 2842 B; 3499 B → 3149 B, no error); fixed write-pointer slot silently corrupts parses in an enlarged arena | **CLOSED as an A-class defect** (serializer fix + parse guard + regressions); fixed-arena capacity remains a documented limit |
| 2 | Allocator no-split/no-coalesce fragmentation ceiling | B deliberate | 16,300 cycles clean; 16,384 → `E-R02` twice; 4 MiB request after 16,300 cycles → `E-R02` | **DOCUMENTED LIMITATION** (loud, deterministic, no corruption) |
| 3 | HTTP 4 MiB body budget | C documented | 4 MiB exact; 4 MiB+1..+4096 accepted; +16384 → `TOO_LARGE` with pattern-correct prefix | **DOCUMENTED LIMITATION** |
| 4 | stdio 8 MiB − 8 payload window | C documented | 8,388,600 B exact; 8,388,601 B → `E-R02`, exit 102 | **DOCUMENTED LIMITATION** |
| 5 | Socket connect/read timeout API | D capability gap | No timeout parameter; unreachable connect is OS-bounded (~21 s), readiness timeout is exact | **DOCUMENTED LIMITATION** (capability gap; carried) |
| 6 | UDP concurrency / shared receive buffer | unvalidated | not exercised concurrently in this session | **UNVALIDATED** (explicitly limited; no broad claim made) |
| 7 | `net_dial` loses the WSA error on failure | B diagnostic | `net_dial` returns −1 but `net_last_error()` is 0; direct `net_connect` reports 11001/10061 | **DOCUMENTED LIMITATION** (diagnostic gap; operation reliably fails) |
| 8 | Shared runtime formatting buffers (`rt_print_int`/float paths) | B (P6 B1) | not exercised by this session's workloads; stdout formatting only | **DOCUMENTED LIMITATION** (carried to M3) |
| 9 | SQLite single-owner/whole-blob persistence, no fsync | C documented | dynamic strings + transactions revalidated; no product change | **DOCUMENTED LIMITATION** |
| 10 | HTTP chunked decoding | C/D documented | chunked body returned framed; completeness check deliberately exempts `Transfer-Encoding` | **DOCUMENTED LIMITATION** |
| 11 | Session 129 A4: if-expression in operand position crashes the compiler | A, deferred | `check` panics, exit 101, at `literal_operand` | **CLOSED** (fully fixed, 8 regressions) |
| 12 | Session 129 A5: `if` without `else` as a non-final statement in a closure block is a parse error | A, deferred | still rejected with structured `E-P26` (a diagnostic, not a crash) | **OPEN (documented)** — parser inconsistency carried to M3 |
| 13 | `MAX_LIVE_ALLOCS = 1024` simultaneous allocations | C architectural | not re-measured; unchanged ABI limit | **DOCUMENTED LIMITATION** |
| 14 | Nested `else if` expression silently returns 0 | not previously identified | reproduced on the stale pre-P9 compiler (exit 0 instead of 9) | **CLOSED** (HIR fix + regressions) |
| 15 | JSON parse write pointer overwrites a value in an enlarged arena | previously recorded as wrong middle values (P4/P8) | 4091/6996 B documents return corrupt parsed values with a 1 MiB arena | **CLOSED** (parse now returns 0 instead of corrupt data) |

## 4. Criticality assessment

| Defect | Silent corruption? | False success? | Crash? | Deadlock? | Typical workload blocked? | Explicit? | Verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| If-expression in operand position (A4) | no | no | **compiler panic** | no | common expression form | no (panic) | **CRITICAL/HIGH → fixed** |
| Nested `else if` expression | **yes** (wrong value) | **yes** | no | no | normal branching | no | **CRITICAL → fixed** |
| JSON serializer overlap | **yes** (wrong/short text) | **yes** | no | no | JSON documents in enlarged arenas | no | **CRITICAL → fixed** |
| JSON write-pointer collision | **yes** (wrong middle values) | **yes** | no | no | enlarged-arena documents >~3.5 KiB | no | **CRITICAL → fixed (refuse parse)** |
| Allocator ceiling | no | no | no | no | no (loud `E-R02`) | yes | B, documented |
| HTTP 4 MiB budget | no | no (loud −4) | no | no | no | yes | C, documented |
| stdio window | no | no (loud `E-R02`) | no | no | no | yes | C, documented |
| Socket timeouts | no | no | no | OS-bounded wait | a silent peer can occupy a worker | partly | D, carried |
| `net_dial` error | no | no (returns −1) | no | no | no | no (error text lost) | B diagnostic, carried |
| Shared print buffers | possible log-text mix-up | no | no | no | no (stdout only) | no | B, carried |
| SQLite persistence | no (documented) | no | no | no | no | yes | C, documented |
| Chunked decoding | body returned framed, not decoded | no | no | no | no | yes | C/D, documented |
| A5 closure `if` parse error | no | no | no | no | workaround exists (`else` branch) | yes (E-P26) | A, carried |

Only the four silent-corruption/crash items received code changes.

## 5. JSON

### 5.1 Reproduced before the fix

`tmp/m2_p8/json_scale.mink` (built with the pre-session library) on compact
integer arrays with a 1 MiB caller arena:

| Document | Parse | Reported serialization | Actual result |
| --- | --- | --- | --- |
| 2047 B | correct | 2047 B | exact |
| 3499 B | correct | **3149 B** | **silently wrong length** |
| 4091 B | “correct” (n/first/last) | **2842 B** | silently wrong, middle values corrupted |
| 6996 B | “correct” | **1702 B** | silently wrong |

A 4091-byte document in a 1 MiB arena also had a wrong middle byte even after
the serializer fix: its parsed value tree reaches slot **8191**, where
`json_parse` stores its persistent write pointer, so the pointer write
overwrote a value and the parse still reported success.

### 5.2 Root causes

1. **Serializer:** `json_to_str` wrote its output at a fixed slot (`7168`) with
   a 1024-slot area, regardless of the caller's arena size. For a parsed value
   tree that extended past slot 7168, serialization overwrote values it had
   not yet read (including array counts and digits), producing a shorter or
   wrong string with no error.
2. **Parser write pointer:** the persistent pointer is stored at fixed slot
   8191. A value tree that reaches that slot is overwritten by the pointer
   write. With the documented 64 KiB arena the overflow is usually caught by
   the arena bounds (`E-R05`); with an enlarged arena the writes stay in
   bounds and the corruption was silent.

### 5.3 Fix (minimal, no memory-model redesign)

`stdlib/json.mink` and the byte-identical `npm/mink/stdlib/json.mink`:

- `json_to_str` writes **after** the parsed value tree:
  `out_base = offset + _val_size(arena, offset)`. Reads and writes can no
  longer overlap. The fixed `_ser_out_base` helper was removed.
- `json_parse` refuses a parse whose value tree would reach slot 8191: it
  frees the input and returns 0 before writing the pointer, so a caller with a
  larger arena gets a parse failure instead of corrupted data. With the
  documented 64 KiB arena the existing `E-R05` bound is unchanged.
- The header limit text was corrected: the geometry is fixed at 64 KiB and a
  larger arena does **not** raise the supported input size (it can hold the
  serializer output of an accepted document).

### 5.4 After

| Document / arena | Parse | Serialization |
| --- | --- | --- |
| 2047 B / 64 KiB | correct | 2047 B exact |
| 3499 B / 1 MiB | correct | **3499 B exact** (hash recomputed) |
| 4091 B / 1 MiB | **refused (0)** | not exposed |
| 6996 B / 1 MiB | **refused (0)** | not exposed |

Permanent regressions: `tests/json.rs`
`serialize_2047_byte_document_in_documented_arena`,
`serialize_3499_byte_document_in_enlarged_arena`,
`parse_rejects_document_that_would_reach_the_write_pointer`.

## 6. Allocator

Re-ran the P8 shapes against the current tree:

| Case | Result |
| --- | --- |
| `waste` 16,300 cycles | exit 0, `BAD 0` |
| `waste` 16,384 cycles (run twice) | exit 102, `E-R02` both times at the 1 MiB allocation |
| `tail` 16,300 cycles then one 4 MiB request | exit 102, `E-R02` |
| `lifo` 5,000,000 cycles | exit 0, `BAD 0` |

The ceiling is deterministic, the failure is explicit (`E-R02` with a source
location), no false success and no corruption were observed. It is recorded as
**B — deliberate allocator behavior, documented and loud**. No allocator change
was made.

## 7. Process I/O

| Case | Result |
| --- | --- |
| 8,388,600-byte child stdout | exit 0, captured exactly 8,388,600 |
| 8,388,601-byte child stdout | exit 102, `E-R02` (never truncation-with-success) |
| 1 MiB child stderr, empty stdout | exit 0, stderr 1,048,576, stdout 0 |

The window is exactly `8 MiB − 8` per stream (the 8-byte length prefix), as
documented. stderr remains independent. **C — deliberate/documented window**;
the limit was not raised.

## 8. HTTP

`tmp/m2_p8/http_matrix.py` against a deterministic loopback fixture, plus
Python recomputation of every body:

| Case | Result |
| --- | --- |
| 64 KiB / 256 KiB / 1 MiB / 2 MiB bodies, two requests each | status 200, exact lengths, first/last correct, FNV-1a recomputed and equal, `http_last_error() == 0` |
| 4 MiB body | status 200, 4,194,304 bytes, recomputed hash equal, err 0 |
| 8 MiB served | status 200, 4,202,433-byte pattern-correct prefix, **err −4 `TOO_LARGE`** |
| 4 MiB + 0 / +1 / +4096 | err 0 (inside the 8 KiB headroom) |
| 4 MiB + 16384 / +65536 | err −4, pattern-correct prefix, no crash |
| Truncated `Content-Length` 524288/200000 | status 200, body 200,000, **err −5 `HTTP_ERR_RECV`** |
| Truncated 1048576/1000 and 65536/0 | err −5 |
| Exact 200000/200000 | err 0 |
| Peer closes with no response | status 0, body 0, err −5 |

No incomplete `Content-Length` response is reported as success; the Session 137
fix remains closed. Chunked responses stay framed and are deliberately exempt
from the completion check (`h49`). The 4 MiB budget remains **C — documented**.

## 9. Networking

Regenerated probes against deterministic loopback fixtures:

| Case | Result |
| --- | --- |
| `net_dial("127.0.0.1", http)` / `net_dial("localhost", http)` | both succeed |
| `net_resolve("localhost", 80)` | `127.0.0.1` |
| Echo through hostname dial | 9 bytes sent, 9 received, equal |
| Invalid hostname | `−1`, `net_last_error() == 0`, no hang (fast) |
| Refused loopback port | `−1`, `net_last_error() == 0`, ~2.0 s (OS behavior) |
| HTTP by hostname | status 200, 17-byte exact body |
| HTTP by IPv4 | status 200, 17-byte exact body |
| HTTP 4096-byte pattern | status 200, exact, first 17 / last 142 |
| HTTPS (full `tls_lib` with the `python3` shim) | **19 passed, 0 failed, 1 ignored** (pinned CA, SAN, wildcard, expired, wrong name, untrusted root, wrong usage, large transfer, refused, repeated connections) |
| Concurrent TCP echo (8 threads × 50) | 0 failures, 5/5 processes |
| Concurrent HTTP (8 threads × 10) | 0 failures, 5/5 processes |

`net_dial` still clears `net_last_error()` on failure (the operation itself
reliably returns `−1`); recorded as a **B diagnostic gap**, not a correctness
blocker. There is no socket connect/read timeout option (**D capability gap**);
the OS bounds an unreachable connect (~21 s) and `net_wait_readable` is the
documented readiness primitive. Concurrent UDP was not exercised and is
recorded as **UNVALIDATED**, with no broad UDP claim.

## 10. Server and concurrency

`tmp/m2_p6/server_echo_full.exe` with 8 workers and the independent Python
client:

- 800 concurrent requests (8 client threads × 100), **0 failures**, 0.38 s;
- server counters `SERVED 801 ERRORS 1` (the one error is the readiness
  probe's abrupt close);
- handles 90–94 (flat), working set 4.7 → 5.5 MB;
- clean `QUIT`, exit 0.

The Session 134 `rt_str_from_int` concurrency regression and the Session 136
network-init/`net_recv` regressions remain green in the complete suite. The
`rt_print_int` shared-scratch item (P6 B1) is not exercised by these workloads
and remains a **B documented limitation** (stdout text only).

## 11. SQLite

`tmp/m2_p8/gen/p81_sql_probe9.exe` (current engine):

```
CONCAT=OK | FUNC=OK | REPEAT_FAILS=0 | ROWS=24
TX_ROWS=25 | TX_AFTER_ROLLBACK=25
```

Literal, heap-built, concatenated, function-produced and repeated statements
all execute; the transaction path commits and rolls back correctly. The
Session 137 regressions (`s41_*`) are green in `sqlite_lib` (18 tests). The
remaining limitations are unchanged and documented: single owner, whole-blob
copy per statement, whole-file rewrite per persist, no fsync. **C — documented
limitation**; no redesign.

## 12. Representative application matrix

Existing M2 probes/workloads, re-run on the current tree:

| Workload | Result |
| --- | --- |
| A. Windows filesystem pipeline (`workload_a`) | PASS — report byte-equal to the independent Python computation (25 files, 3,362 bytes) |
| B. Process/automation utility (`workload_b`) | PASS — per-file counts, regex tags, SHA-256 and child `cmd /c ver` capture exact |
| C. SQLite local application (`workload_c`) | PASS — `COUNT 480`, HEAD digest and row sum match; separate `verify` process reproduces them |
| D. HTTP client (`workload_d`) | PASS — 200/84-byte body, close → −5, refused → explicit failure, recovery request exact |
| E. Concurrent worker application (`workload_e`) | PASS — 128 whole lines, XOR matches, `FAILS 0` |
| Filesystem scale | PASS — 1/4/8/16/32/64 MiB read/write, SHA-256 equal, 0.27 s at 64 MiB |
| Mixed workload (filesystem + strings + hash + JSON + HTTP + SQLite) | PASS — 100/100 iterations verified, 0 mismatches, repeat output identical, handles flat |
| Large process capture | PASS — exact at 8,388,600 B, loud `E-R02` above |
| HTTP/HTTPS client | PASS — 4 MiB exact, truncation −5, HTTPS suite 19/19 |

Release builds and release execution of representative workloads are recorded
in §18.

## 13. Resource stability

| Workload | Observation |
| --- | --- |
| 5,000,000-cycle `lifo` alloc/free | exit 0, `BAD 0`, no growth observed in the run |
| Mixed 100 iterations | 4.7 → 7.1 MiB private, handles 65–66, no monotonic handle growth |
| Echo server, 800 concurrent requests | handles 90–94, working set 4.7 → 5.5 MB |
| File 64 MiB round trip | 0.27 s, SHA equal |

No handle leak, no stray process and no unexpected runtime degradation was
observed. No allocator or runtime code changed in this session; performance
regression risk is limited to the compiler/AST lowering changes, which are
compile-time only and covered by the full suite.

## 14. Defects found

### Defect 1 — if-expression in operand position panics the compiler (A4, known)

**BEFORE:** `mink check` on `rt_print_int(if true { 1 } else { 2 });` panics
(`exit 101`) at `literal_operand is only called for literals`. Also reproduces
for `return if ...`, `1 + if ...`, `f(if ...)`, assignment RHS, array/tuple/
struct elements and a block trailing expression.

**ROOT CAUSE:** `FnBuilder::eval_operand` only intercepted while/loop/match/
try/block/and/or; every other composite expression was delegated to
`StmtEval::eval_operand`, whose `IfExpr` arm called `literal_operand`, which is
documented for literals only. `return if ...` initially worked after the first
partial fix because `Return` goes through `FnBuilder`, but call arguments,
binary operands and assignments still reached `StmtEval`.

**CHANGE:** `src/mir/lower.rs` — `FnBuilder::eval_operand` now lowers all
composite operands (unary, binary, range, call, deref, member, index, struct
literal, array literal, enum variant, tuple, tuple field access, assignment)
and recursively routes their sub-expressions through the CFG-aware path. The
if-expression arm materializes the value into a temporary with the same
`lower_if_expr_binding` used for a binding initializer. `lower_block_trailing_expr`
also materializes a trailing if-expression. `eval_assign` was split into
value evaluation + `eval_assign_with_value` so the RHS can use the CFG-aware
path while sharing the target/compound-assignment logic.

A recursive `needs_cfg` predicate gates the new path: an operand that contains
no control-flow expression (and is not a top-level `&&`/`||`, which has its own
short-circuit arms) still goes through the single-rvalue `StmtEval` shape. That
keeps the pre-existing MIR shape — and the constant folding the optimizer tests
pin — while routing exactly the operands that need branch blocks through the
CFG path.

**AFTER:** `return if`, call arguments, binary operands, assignment RHS,
array/tuple/struct elements and block trailing if-expressions compile and
produce the taken branch's value. `tests/optimization.rs` (38 tests) stays
green: pure operands add no temporaries and `&&`/`||` still desugar to
short-circuit blocks. `?` (`HirExprKind::Try`) is always routed to the CFG path
because it returns early; the existing `adv_option_try_*` tests in
`tests/adversarial.rs` (94 green) caught the first version of the gate, which
tried to lower `?` through `StmtEval` and hit its
"Try should be handled by FnBuilder" assertion.

**REGRESSION:** `tests/closures.rs`
`if_expression_in_return_position`, `if_expression_as_call_argument`,
`if_expression_as_binary_operand`,
`nested_if_expression_in_operand_position`,
`if_expression_as_block_trailing_operand`,
`if_expression_as_assignment_value`, `if_expression_in_array_literal`; plus
`tests/mir.rs::if_expression_in_operand_position_lowers` (the MIR layer must
lower without panicking).

### Defect 2 — nested `else if` expression silently returns 0 (new)

**BEFORE:** `let x = if false { 1 } else if true { 2 } else { 9 }; return x;`
builds and exits **0** instead of 2. The same shape in `return` position also
returned 0 after the operand-position fix. Reproduced with the pre-P9 release
compiler, so it predates this session.

**ROOT CAUSE:** `src/hir/lower.rs` lowered the nested `else if` of an
if-*expression* as `HirElseBranch::If` (a statement `if`). MIR's
`lower_if_expr_binding` handles `If` with `lower_if`, which never assigns the
expression's result local, so the local kept its zero initializer.

**CHANGE:** lower the nested branch as `HirElseBranch::IfExpr`, matching the
other two if-expression lowering sites.

**AFTER:** the binding and return forms return the taken arm (2 and 9).

**REGRESSION:**
`tests/closures.rs::nested_else_if_expression_in_binding_returns_the_taken_arm`
(and the existing nested operand test).

### Defect 3 — JSON serializer overwrites parsed values (A-class silent corruption)

**BEFORE:** a 3499-byte document parsed into a 1 MiB arena serialized to
**3149 bytes**; 4091 B → 2842 B; 6996 B → 1702 B, all with exit 0 and no
error.

**ROOT CAUSE:** fixed output base at slot 7168 with a 1024-slot area,
overlapping parsed values that extend past 7168.

**CHANGE:** write output after the value tree
(`out_base = offset + _val_size(arena, offset)`); remove the fixed helper.

**AFTER:** 2047 B (64 KiB arena) and 3499 B (1 MiB arena) serialize exactly
with recomputed hashes. Documents whose value tree reaches slot 8191 are
refused by the parser instead of exposing corrupt data.

**REGRESSION:** the three `tests/json.rs` Session 138 tests.

### Defect 4 — JSON parse pointer overwrites a value at slot 8191 (silent parse corruption)

**BEFORE:** 4091 B / 6996 B documents in an enlarged arena reported a
successful parse, but a middle value was overwritten by the persistent
write-pointer write at slot 8191 (e.g. `841` became `\x0841`).

**ROOT CAUSE:** fixed arena geometry; the pointer lives in the last slot and
the parser did not check whether the value tree reached it.

**CHANGE:** refuse the parse (`return 0`) when
`result.0 + _val_size(arena, result.0) > 8191`, before writing the pointer.

**AFTER:** those documents return 0; no corrupt value is exposed. The
documented 64 KiB limit is unchanged (oversized input there still fails with
`E-R05`).

**REGRESSION:**
`tests/json.rs::parse_rejects_document_that_would_reach_the_write_pointer`.

No other new critical/high correctness defect was found.

## 15. Fixes

| File | Change |
| --- | --- |
| `src/mir/lower.rs` | CFG-aware composite operand lowering; if-expression materialization; block-trailing if; assignment value split |
| `src/hir/lower.rs` | nested `else if` expression lowered as `HirElseBranch::IfExpr` |
| `stdlib/json.mink` | serializer output after the value tree; parse refuses a value tree reaching slot 8191; corrected header limits |
| `npm/mink/stdlib/json.mink` | byte-identical mirror |
| `tests/closures.rs` | 8 if-expression regressions |
| `tests/mir.rs` | 1 MIR lowering regression |
| `tests/json.rs` | 3 serializer/parse regressions |

No runtime/emitter, allocator, HTTP, SQLite, version, npm-package, Linux or
roadmap change was made. No test was deleted, skipped or weakened.

## 16. Permanent regression coverage

| Test | Locks |
| --- | --- |
| `if_expression_in_return_position` | `return if ...` lowers and returns the arm value; `check` agrees with `build` |
| `if_expression_as_call_argument` | `f(if ...)` evaluates before the call |
| `if_expression_as_binary_operand` | `1 + if ...` merges before the surrounding operation |
| `nested_if_expression_in_operand_position` | nested `else if` in operand position |
| `nested_else_if_expression_in_binding_returns_the_taken_arm` | the HIR statement-vs-expression fix |
| `if_expression_as_block_trailing_operand` | block trailing if-expression |
| `if_expression_as_assignment_value` | assignment RHS if-expression |
| `if_expression_in_array_literal` | array element if-expression |
| `if_expression_in_operand_position_lowers` (MIR) | lowering must not panic on the A4 shapes |
| `serialize_2047_byte_document_in_documented_arena` | documented-arena serialization beyond the old 1024-slot area |
| `serialize_3499_byte_document_in_enlarged_arena` | serializer no longer overlaps parsed values |
| `parse_rejects_document_that_would_reach_the_write_pointer` | corrupt parse is refused, not returned |

## 17. Full regression

Final tree, `CARGO_BUILD_JOBS=1` (to avoid the environment's intermittent
`LNK1104` link lock):

| Command | Result |
| --- | --- |
| `cargo test --no-fail-fast` | 87 binaries: **3,055 passed, 18 failed, 1 ignored** |
| `cargo test --test tls_lib` (with the `python3` shim on `PATH`) | **19 passed, 0 failed, 1 ignored** |
| Combined | **3,073 passed, 0 failed, 1 ignored** |

All 18 failures in the plain run are `tests/tls_lib.rs` server-spawn failures
(`bad port line from TLS test server: ""`): the suite launches its TLS fixture
with `python3`, which is absent on this host. Re-running that one target with a
`python3.exe` shim on `PATH` is green. The failures never reach MINK code; they
are the documented Session 125–137 environment condition, and this is the same
baseline state recorded in §2.

Per-target results on the final tree: `adversarial` 94, `closures` 51, `mir` 66,
`json` 35, `optimization` 38 — 0 failed. No test was removed, skipped or
weakened, and no test target changed its pass count except by the 12 new tests.

## 18. Release verification

Performed after all source changes.

- `cargo build --release` — success (`mink v1.0.4`, release profile, 1m11s).
- `target/release/mink.exe` built five representative workloads to standalone
  PEs: `workload_{a,b,c,d,e}_full.exe` (42, 84, 114, 94, 46 functions).
- Executed release binaries (not debug builds):

| Workload | Command | Result |
| --- | --- | --- |
| A — filesystem hashing pipeline | `workload_a_full.exe <dir>` | exit 0; SHA-256 `5891b5b5…f6be03` for `one.txt` (6 B) and `41b4209a…a768d` for `two.txt` (12 B), both matching the independently computed digests; `COUNT 2 BYTES 18` |
| B — filesystem + process automation | `workload_b_full.exe <dir>` | exit 0; same two digests, `FILES 2 LINES 2`, `CHILD exit=0 has=1` |
| C — SQLite application | `workload_c_full.exe app.db write`, then `… verify` in a second process | exit 0 both; `COUNT 480`, `HEAD e73e70c1…d889f`, `ROWS 480 SUM 380055` identical across processes |
| E — concurrent worker application | `workload_e_full.exe` | exit 0; `WORKERS 8 LINES 128 XOR 1055109961331643210 FAILS 0` |

An invocation with a missing required argument produced a loud
`E-R10` (index out of range on `rt_argv(0)`) and exit 110 — the runtime does not
silently substitute an empty path.

Workload D (HTTP client) drives a live server through the Python fixture and is
covered by the `http_lib` suite (46 tests, green) and the P8 HTTP matrix rather
than by a standalone release run in this session.

## 19. Final M2 evidence matrix

| Category | Evidence | Result | Test/source | Classification | Status |
| --- | --- | --- | --- | --- | --- |
| Filesystem | 1–64 MiB read/write/copy, SHA-256 | exact at every size | `file_matrix.py`, `filesystem_lib` | works | **CLOSED** |
| Strings/heap | 5,000,000-cycle LIFO, boundary checks | `BAD 0`, exit 0 | `heap_scale lifo` | works | **CLOSED** |
| Allocator | waste 16,300/16,384; tail; 4 MiB request | deterministic `E-R02` at the ceiling; no corruption | `heap_scale` | B deliberate | **DOCUMENTED LIMITATION** |
| JSON | 2047/3499/4091/6996 B, 64 KiB and enlarged arenas | fixed: exact serialization ≤ accepted size, oversized parse refused | `tests/json.rs`, `json_matrix.py` | fixed + fixed 64 KiB limit | **CLOSED** |
| Process I/O | 1 MiB–8,388,601 B stdout, 1 MiB stderr | exact to 8,388,600; loud `E-R02` above; streams independent | `process_scale`, `process_lib` | C documented | **CLOSED** |
| Process stdin/lifecycle | 4096-byte stdin boundary; 1,000-child soak; handle invariance | documented 4096-byte cap; no leak | Session 131, `process_lib` | C documented | **CLOSED** |
| Networking | hostname/IP TCP, DNS, invalid/refused, repeated, concurrent | core paths work; `net_dial` error text lost | `tcp_hostname`, `network_lib` | B diagnostic gap | **CLOSED (core)**; diagnostic **DOCUMENTED LIMITATION** |
| UDP | not exercised concurrently | no broad claim | — | unvalidated | **UNVALIDATED** |
| HTTP | 64 KiB–4 MiB exact, overflow, truncation, close, chunked | exact; truncation −5; overflow −4; chunked framed | `http_lib` 46, `http_matrix.py` | works; 4 MiB C | **CLOSED** |
| HTTPS | `tls_lib` with shim | 19 passed, 1 ignored | `tls_lib` | works | **CLOSED** |
| Server workloads | 8-worker echo, 800 concurrent requests | 0 failures, clean lifecycle | `server_echo`, Python client | works | **CLOSED** |
| Concurrency | TCP/HTTP 8 threads, `rt_str_from_int` regression | 0 failures, 0 mismatches | `concurrency_*`, `threads_lib` | works | **CLOSED** |
| SQLite | literal/heap/concat/function statements, repeats, transaction | all OK, rows/rollback correct | `sqlite_lib` 18, `p81_sql_probe9` | works; C persistence | **CLOSED (engine)**; persistence **DOCUMENTED LIMITATION** |
| Large-scale workloads | 64 MiB files, 8 MiB process capture, 4 MiB HTTP | exact; loud ceilings | P8 matrices | works | **CLOSED** |
| Mixed workload | 100 iterations, six subsystems | 0 mismatches, identical repeat | `mixed_driver.py` | works | **CLOSED** |
| Resource stability | 5 M cycles; mixed 100; 800-request server | no leak, handles flat | probes | works | **CLOSED** |
| Release build | `cargo build --release` | success | — | works | **CLOSED** |
| Release execution | representative release workloads | see §18 | — | works | **CLOSED** |
| Regression coverage | full suite + 12 new tests | 0 failures | `cargo test` | works | **CLOSED** |
| Documentation | corrected JSON limits; this record | accurate | this document | works | **CLOSED** |

There is no “mostly works” status: each category is CLOSED, DOCUMENTED
LIMITATION, UNVALIDATED or OPEN DEFECT.

## 20. Practical usefulness gate

1. **Can MINK build useful native Windows CLI utilities?** Yes — five
   integrated workloads, standalone PEs, release builds (§12, §18).
2. **Can it reliably manipulate files at tens-of-MiB scale?** Yes — 1–64 MiB
   round trips with SHA-256 equality (§12).
3. **Can it process normal JSON workloads within documented limits?** Yes —
   up to ~3.5 KiB parse / ~2 KiB serialize in the 64 KiB arena, exact; larger
   input is rejected loudly or refused, never silently corrupted. Enlarged
   arenas support exact serialization of accepted documents (§5).
4. **Can it interact with SQLite reliably?** Yes — literal/dynamic SQL,
   transactions, persistence and restart, with documented single-owner limits
   (§11).
5. **Can it communicate over TCP/HTTP/HTTPS?** Yes — hostname/IP TCP, DNS,
   plain HTTP, HTTPS with pinned CA, full TLS suite green (§9).
6. **Can it run concurrent worker/server workloads?** Yes — 8-thread TCP and
   HTTP clients, 8-worker echo server, 800 concurrent requests, 0 failures
   (§10).
7. **Can it spawn processes and capture normal output reliably?** Yes — exact
   stdout/stderr capture to 8,388,600 B, loud `E-R02` above, child stdin
   documented cap, no handle leak (§7).
8. **Are major boundaries explicit rather than silent?** Yes — allocator,
   stdio, HTTP budget, JSON geometry and SQLite persistence all fail loudly or
   are documented; the JSON silent paths were closed this session.
9. **Are remaining limitations documented?** Yes — §21.
10. **Are remaining limitations mostly capability/design gaps rather than
    correctness failures?** Yes — remaining items are diagnostics (net_dial
    error text, shared print buffer), capability (socket timeouts, chunked
    decoding, UDP concurrency), documented ceilings (allocator, JSON, HTTP,
    stdio, SQLite), and one parser inconsistency (A5).

**M2 PRACTICAL USEFULNESS: PASS.**

## 21. Remaining limitations

- Allocator: 1 GiB heap, no split/coalesce; fragmentation can fail a large
  request while small free blocks exist; loud `E-R02` (B, documented).
- JSON: fixed 64 KiB arena geometry; ~3.5 KiB input parse limit, ~2 KiB
  serialization within the documented arena; larger arenas do not raise the
  input limit (C, documented).
- HTTP: 4 MiB body budget + 8 KiB whole-response headroom; `TOO_LARGE` above;
  chunked transfer encoding is not decoded (C/D, documented).
- Process: 8 MiB − 8 per-stream capture; 4096-byte child stdin cap (silent
  truncation, documented) (C).
- Sockets: no connect/receive timeout options; `net_wait_readable` is the only
  bounded wait (D capability gap).
- `net_dial` loses the Winsock error; use `net_connect` when the code matters
  (B diagnostic).
- `rt_print_int`/float formatting still share the `.bss` print buffer; worker
  stdout text can interleave (B, carried).
- HTTP last-error state is process-global, not per-thread (B, carried).
- Concurrent UDP (`net_recvfrom` shared buffer) is unvalidated (explicitly
  limited; no broad claim).
- SQLite: single owner, whole-blob copy per statement, whole-file rewrite per
  persist, no fsync; a crash between COMMIT and the write loses the last
  mutation (C).
- A5: `if` without `else` as a non-final statement in a closure block is a
  parse error (`E-P26`) while the same body in a function compiles; structured
  diagnostic, workaround available, carried to M3 (A).
- `MAX_LIVE_ALLOCS = 1024` simultaneous allocations (C architectural).
- `rt_fs_read` returns an empty string for a missing file (documented
  capability shape).

## 22. M2 closure decision

M2 closure gate:

- [x] Windows x86_64 remains green
- [x] full Rust regression passes
- [x] release build passes
- [x] representative release workloads pass
- [x] no unresolved critical correctness defect remains in the tested
      workload class
- [x] no silent truncation/false-success behavior remains in tested supported
      workloads (JSON silent paths closed; HTTP truncation −5; stdio/allocator
      loud)
- [x] process I/O boundaries are explicit
- [x] networking core paths are reliable
- [x] HTTP truncation regression remains closed
- [x] SQLite dynamic-string behavior is proven correct
- [x] concurrency regressions remain closed
- [x] large-file behavior is proven
- [x] resource stability is acceptable
- [x] remaining ceilings are documented
- [x] evidence matrix is complete
- [x] session record is complete
- [x] final git audit is clean
- [x] Linux remains untouched
- [x] version remains 1.0.4
- [x] no publish/tag/version change occurred

**M2 STATUS = CLOSED.**

M2 CLOSED — Windows x86_64 practical-use validation is complete.

## 23. Audit

- Changed files this session: `src/mir/lower.rs`, `src/hir/lower.rs`,
  `stdlib/json.mink`, `npm/mink/stdlib/json.mink`, `tests/closures.rs`,
  `tests/mir.rs`, `tests/json.rs`, and this record.
- `stdlib/json.mink` and `npm/mink/stdlib/json.mink` are byte-identical.
- No runtime/emitter, allocator, HTTP, SQLite, Cargo, npm-package, version,
  tag, Linux or roadmap change. No commit, tag, publish or release command.
- All probes, shims and logs live under the git-ignored `tmp/m2_p9/` (and the
  earlier ignored `tmp/m2_*` directories).
- Pre-existing modified/untracked files (Sessions 128–137 work, roadmap
  archive, `nl1.*`, the pasted transcript, the untracked `httpserver` module
  and suite) remain untouched.
- No stray server, fixture, probe or client process is left running.
- Version remains 1.0.4; no tags were created; Linux was not touched.

## 24. Next

M2 is closed. Do not start M3 in this session. The recommended next step is a
scoped M3 planning session driven by the carried items: socket timeouts and
`net_dial` error preservation, UDP buffer parity, thread-local HTTP/formatting
state, chunked decoding, JSON capacity API, allocator splitting/coalescing (only
if demand justifies it), SQLite durability, and the A5 parser inconsistency.
