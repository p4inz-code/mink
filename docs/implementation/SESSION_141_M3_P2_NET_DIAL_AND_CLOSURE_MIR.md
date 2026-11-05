# Session 141 — M3 P2: `net_dial` error preservation and the closure trailing-expression MIR panic

**Scope:** resume after Session 140 and close the two pre-existing A/B-class defects it recorded
for M3 P2, with root-cause fixes, permanent regressions and full re-verification. Nothing else.

Windows x86_64 only. Linux stayed frozen (the new intrinsic resolves to the established no-op
stub there). Version stayed **1.0.4**. No tag, no publish, no push, no version change. M4/M5 and
the rest of M3 were not started. Every pre-existing dirty file was preserved; no reset, rebase,
clean, checkout or amend was performed.

**Outcome:** both defects are **CLOSED in the working tree and verified**.
`net_dial` now reports the real Winsock code after it closes the failed socket, and a closure
whose trailing expression is a composite operand containing an if-expression now lowers
correctly instead of panicking in MIR. 13 permanent regressions were added; the full suite is
**3,101 passed, 0 failed, 1 ignored**.

**One step is deliberately blocked:** the single focused commit. The two fixes extend code that
only exists in the pre-existing uncommitted Session 129–138 work (`needs_cfg` and the CFG-aware
operand path in `src/mir/lower.rs`), so a commit containing only this session's hunks would not
fix the defect and its new tests would fail at that commit; a commit containing enough to pass
would absorb several hundred lines of earlier sessions' work. Per the session's own safety rule
(§10/§14), the commit was **not** created. §13 gives the exact unblocking sequence.

## 1. Starting state

Verified before any edit.

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `1d6fa24896c106c2ef7213d841e66eb6e3938b76` — *docs: record the Session 140 commit hash in its session record* |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Divergence | **ahead 4, behind 1** (Session 140 ended `ahead 3, behind 1`; its docs commit adds the fourth) |
| Staged | none |
| Modified tracked files | 27 (the same 27 Session 140 listed) |
| Untracked entries | 19 (`MINK UI SETS OFFICIAL.zip`, Session 128–138 records, `docs/roadmap/archive/`, the pasted transcript, `nl1.exe`, `nl1.mink`, the three `httpserver` files) |
| Version | **1.0.4** (`Cargo.toml`, `mink version`) |
| `git diff --check` | clean |
| `cargo build` | success, **59** warnings — unchanged |
| Focused baseline | `mir` 35, `closures` 51, `network_lib` 37 — all passed |

This matches Session 140's final state exactly, including the branch divergence and the dirty
entries. The baseline differed only in that Session 140's records are now untracked files, not
new ones.

## 2. Defect A — `net_dial` loses the Winsock error

### 2.1 Reproducer (before)

```
net_init();
let s = net_dial("127.0.0.1", 19991);
let err = net_last_error();        // 0
```

Program output: `-1` then `0`. Controls on the same tree, same run:

| Path | Result |
| --- | --- |
| `net_connect` to the same refused port | `-1`, `net_last_error() == 10061` |
| `net_connect("mink-no-such-host.invalid", 80)` | `-1`, `net_last_error() == 11001` |
| `net_close` after either failure | `net_last_error()` becomes `0` |

### 2.2 Root cause

`stdlib/network.mink`'s `net_dial` closes the failed socket before returning
(`net_close` → `rt_net_close` → `closesocket`), and `closesocket` clears the calling thread's
Winsock last-error. The connect result and its reason are correct; the cleanup erases the
reason. The library comment already promised `net_last_error()` holds the reason, so this was a
contract violation, not a missing feature.

### 2.3 Fix

A setter was the missing mechanism (`WSASetLastError` was already resolved in the runtime's
extended ws2_32 table, but not exposed to MINK):

| File | Change |
| --- | --- |
| `src/runtime/intrinsics.rs` | `rt_net_wsa_set_last_error(code: Int) -> Int` |
| `src/backend/ir.rs` | `RuntimeService::NetWsaSetLastError`, arity 1, callable |
| `src/backend/lower.rs` | intrinsic-name mapping |
| `src/backend/emit/runtime.rs` | emitter calling `WSASetLastError` (`NET_FUNC_TABLE2 + 0`), returns 0; guarded by `NET_INIT_FLAG` so it is a no-op, never a null call, before `net_init` |
| `src/backend/emit/linux_runtime.rs` | no-op stub returning 0 (Linux stays frozen) |
| `stdlib/network.mink` | `net_set_last_error(code)` wrapper; `net_dial` captures the code, closes, republishes |
| `npm/mink/stdlib/network.mink` | byte-identical mirror |
| `docs/architecture/CROSS_PLATFORM_RUNTIME_ARCHITECTURE.md` | intrinsic table entry |

`net_dial`'s failure path is now:

```
let conn_result = net_connect(sock, addr, port);
if conn_result != 0 {
    let err = net_last_error();
    net_close(sock);
    net_set_last_error(err);
    return -1;
}
```

No socket timeout, UDP, chunked-decoding, HTTP or other network API was touched.
`net_listen_on` was left alone (same family, outside this session's strict scope).

### 2.4 Regression tests

`tests/network_lib.rs` §12 — four tests, appending only:

| Test | Locks |
| --- | --- |
| `n103_net_dial_reports_refused_error_after_cleanup` | `-1` **and** `10061` survive the close |
| `n104_net_dial_reports_resolver_error_after_cleanup` | `-1` **and** `11001` for `.invalid` (offline, deterministic) |
| `n105_net_dial_repeated_failures_report_their_own_error` | refused → resolver → refused each report their own code; no stale zero |
| `n106_net_dial_success_still_returns_a_usable_socket` | the success path still returns a closeable socket |

Against the pre-fix `stdlib/network.mink`: **n103, n104, n105 failed; n106 passed** (the success
control). With the fix: all pass. No existing test was changed or weakened.

## 3. Defect B — closure trailing-expression MIR panic

### 3.1 Reproducer (before)

```
fn main() -> Int { let c = true; let f = |y: Int| { if c { y } else { 0 } + 5 }; return f(37); }
```

`mink check` → `panicked at src/mir/lower.rs:571: internal error: entered unreachable code:
literal_operand is only called for literals`, exit **101**.

### 3.2 AST/HIR/MIR path

1. The monomorphizer desugars the closure to a named function whose body is
   `return <closure body>` (`desugar_closure_to_fn`), so the body is a **block expression in
   operand position**.
2. HIR keeps that shape: `HirExprKind::Block` whose `result` is
   `HirExprKind::Binary { lhs: IfExpr, rhs: Int(5) }`.
3. `FnBuilder::eval_operand` sees `Block`, `needs_cfg(Block) == true`, enters its Block arm, and
   `lower_block_trailing_expr` lowers the statements. That helper only materializes a
   while/loop/match/**if** trailing expression; a `Binary` trailing expression returns `None`.
4. The arm then fell back to `self.eval.eval_operand(expr)` (the non-CFG `StmtEval`). `StmtEval`
   re-enters the block, reaches the `Binary` arm, evaluates the `IfExpr` operand through
   `StmtEval`'s own `IfExpr` arm, and calls `literal_operand` — documented to accept only
   literals — and hits `unreachable!`.
5. A plain function's `return if c { … } else { … } + 5;` never panicked because the return value
   is the `Binary` directly, not wrapped in a `Block`, so the `needs_cfg` routing applied.

**Root cause:** the Block operand arm of the CFG-aware path dropped back to the non-CFG evaluator
for its trailing expression whenever that expression was not one of the four control-flow
shapes, even when it *contained* one. The panic site is MIR, but the defect is the missing
routing in the same arm — not the literal helper, and not the parser/HIR.

### 3.3 Fix

`src/mir/lower.rs`, `FnBuilder::eval_operand`, `HirExprKind::Block` arm — the trailing expression
now re-enters the CFG-aware path:

```
match &block.result {
    Some(result) => self.eval_operand(result),
    None => self.eval.eval_operand(expr),   // unit block: keep the constant value
}
```

A pure trailing expression still delegates to `StmtEval` unchanged (`needs_cfg` is false), so the
optimizer shape for ordinary closures is untouched. No panic guard, no defensive placeholder, no
parser or HIR change. The equivalent non-closure shapes are fixed by the same routing.

### 3.4 Regression tests

`tests/mir.rs` — `block_operand_trailing_binary_with_if_expression_lowers_to_branches`: lowers
`return { if c { 37 } else { 0 } + 5 };` and asserts the MIR contains a real `Branch` terminator,
at least three blocks (then/else/merge) and ordered blocks. It **fails with the pre-fix arm**
(`literal_operand` panic) and passes with the fix.

`tests/closures.rs` — eight tests:

| Test | Shape |
| --- | --- |
| `closure_trailing_binary_with_if_expression_operand` | the exact reproducer; `check` and build agree |
| `closure_trailing_binary_with_if_expression_on_the_right` | `5 + if …`, other branch |
| `closure_trailing_parenthesized_if_expression_operand` | parenthesized operand, `*` |
| `closure_trailing_binary_with_nested_if_expression_operand` | nested if inside the operand |
| `closure_trailing_if_expression_operand_reads_locals` | branches read locals, not literals |
| `closure_trailing_binary_with_block_wrapped_if_expression` | block nested in the trailing expression |
| `block_trailing_binary_with_if_expression_operand_outside_a_closure` | `let` initializer and `return` block |
| `ordinary_and_statement_prefixed_closure_trailing_expressions` | control: ordinary / statement + trailing |

Against the pre-fix arm: **7 of the 8 failed** with the same `literal_operand` panic; the control
test passed as intended. With the fix: all pass.

Manual adversarial matrix (release and debug compilers agree):

| Shape | Exit |
| --- | --- |
| `if c { y } else { 0 } + 5`, `c=true, y=37` | 42 |
| `5 + if c { 1 } else { 2 }`, `c=false` | 7 |
| `(if c { y } else { 21 }) * 2` | 42 |
| nested `if` operand | 42 |
| ordinary trailing expression | 42 |
| statement then trailing expression | 42 |
| literal/variable branches | 42 |
| block-wrapped if expression | 42 |
| non-closure block initializer | 7 |
| `-` operand | 42 |
| `if … && true` operand | 1 |
| `Str` if-expression inside a call argument | 42 |

## 4. Verification

| Check | Result | Classification |
| --- | --- | --- |
| `cargo build` | success, **59** warnings — unchanged | PASS |
| `cargo test --test network_lib` | **41 passed, 0 failed** (37 + 4) | PASS |
| `cargo test --test mir` | **36 passed, 0 failed** (35 + 1) | PASS |
| `cargo test --test closures` | **59 passed, 0 failed** (51 + 8) | PASS |
| Broader batch — `adversarial` 94, `hir` 25, `if_expressions` 9, `lexer` 50, `loop_expressions` 39, `match_expressions` 41, `optimization` 38, `parser` 104, `semantics` 75, `smoke` 13 | all `ok`, 0 failed (488 total) | PASS |
| `cargo test --test http_lib` / `--test httpserver_lib` | **46 / 10 passed**, 0 failed | PASS |
| Full suite `cargo test --no-fail-fast` (76 test binaries + doc-tests), `python3` shim on `PATH` — re-run after the final emitter guard | **3,101 passed, 0 failed, 1 ignored**, exit **0** | PASS |
| `tests/tls_lib.rs` in that run | **19 passed, 0 failed, 1 ignored** (public-internet test) | PASS |
| Existing-test proof | n103/n104/n105 and 7 closure tests + the MIR test fail with the fixes reverted | PASS |
| `cargo build --release` | success, 59 warnings (18.12 s; final rebuild after the emitter guard 32.00 s) | PASS |
| Release compiler (`mink 1.0.4`) | `b1` → 42; `a_dial` → `10061 / 10061 / 11001`; the whole matrix matches | PASS |
| `git diff --check` | clean (exit 0) | PASS |

The full-suite total is consistent: Session 140 recorded **3,088**; this session adds exactly
**13** tests (4 + 1 + 8) → **3,101**. Nothing else changed count.

**Environment condition:** the first `cargo build --release` failed with rustc exiting
`0xc0000005` (`STATUS_ACCESS_VIOLATION`) while compiling the library. The rerun of the identical
command succeeded in 18.12 s with no source change, so this is the host/toolchain instability
family already recorded (Session 138's `LNK1104`), **not** a MINK regression. No `LNK1104` lock
occurred during this session.

## 5. Audit

| Area | Result |
| --- | --- |
| Compiler — invariants | The fix restores the existing `needs_cfg` invariant at the one arm that violated it; no new AST/HIR/MIR node, no change to pure operands' lowering shape. |
| Compiler — layers | Root cause fixed in MIR lowering, where the routing lived. Parser, AST, HIR and type analysis were inspected and needed no change. |
| Compiler — closure lowering | The desugared `return <block>` path is the one tested; the non-closure block forms are covered by the same routing. |
| Networking — error lifetime | Capture happens immediately after `net_connect` returns and before `net_close`; republish happens after the close; success path does not read or write the error. |
| Networking — repeated failures | n105 proves each failure republishes its own code; resolver and socket errors are distinguished. |
| Networking — platform | Windows calls `WSASetLastError`; Linux stub is a no-op, preserving frozen behaviour. |
| Tests | 13 permanent regressions; each defect's tests proven red against the pre-fix code; no test deleted, skipped, re-pointed or weakened. |
| Documentation | Intrinsic table updated; this record added. Historical Session 129/133/138/140 records were **not** rewritten — they correctly describe the defects as open at their time. |
| Scope | Only the two M3 P2 defects and their coverage. No timeout, UDP, HTTP, JSON, allocator, FFI, Linux, branding, version or release change. |
| Unrelated bugs found | One new non-blocking defect, recorded below and left untouched. Session 140 §9.3 (`parse_block_expr` trailing-token recovery) remains open and untouched. |

Issues found and corrected during verification:

1. The first MIR regression used `let x = { … }`. `lower_binding` has its own Block handling, so
   that shape passed even before the fix. Rewritten as `return { … }` — the exact closure-desugar
   shape — and re-proven red against the old arm.
2. The first `net_dial` probe embedded a copy of `stdlib/network.mink` taken before the edit and
   reported the old `0`; the probe was regenerated from the fixed library. No product issue.

**New finding (pre-existing, not fixed, non-blocking):** `net_last_error()` called before
`net_init()` segfaults (exit 139). `emit_net_wsa_last_error` in `src/backend/emit/runtime.rs`
calls `WSAGetLastError` through `NET_FUNC_TABLE + 16` with no `NET_INIT_FLAG` guard, so the
pointer is still null. The new `net_set_last_error` intrinsic *is* guarded and returns 0 in the
same situation (verified). This does not affect either fix because `net_dial` calls `net_init`
first and checks its result; the getter's missing guard is a separate M3 item and was left
alone rather than folded into this session.

## 6. Files changed (this session only)

Clean at session start, changed only here:

- `src/runtime/intrinsics.rs`
- `src/backend/ir.rs`
- `src/backend/lower.rs`
- `src/backend/emit/linux_runtime.rs`
- `stdlib/network.mink`
- `npm/mink/stdlib/network.mink`
- `docs/architecture/CROSS_PLATFORM_RUNTIME_ARCHITECTURE.md`
- `docs/implementation/SESSION_141_M3_P2_NET_DIAL_AND_CLOSURE_MIR.md` (this record)

Already dirty, received only the minimal M3 P2 hunk:

- `src/backend/emit/runtime.rs` — emit registration + `emit_net_wsa_set_last_error`
- `src/mir/lower.rs` — Block-arm trailing-expression routing
- `tests/network_lib.rs` — four appended tests
- `tests/mir.rs` — one appended test
- `tests/closures.rs` — eight appended tests

All other pre-existing dirty and untracked entries are byte-identical to their starting state.

## 7. Remaining M3 work

From Session 140 §11 item 5 and Session 138's carry list, still open:

1. `rt_print_int` / float shared-buffer issue.
2. Thread-local HTTP last-error.
3. Socket connect/read timeouts (explicitly out of scope here).
4. UDP buffer parity.
5. HTTP chunked decoding.
6. Session 140 §9.3: `parse_block_expr` consumes a non-`}` token after a trailing expression.
7. Session 141 finding: `net_last_error()` before `net_init()` calls a null function pointer
   (guard `emit_net_wsa_last_error` with `NET_INIT_FLAG`, as `net_socket` already does).

M4 (developer-experience benchmark) stays after M3, per the locked roadmap.

## 8. Git actions and final state

- HEAD: `1d6fa24896c106c2ef7213d841e66eb6e3938b76` (unchanged).
- `origin/main`: `b860837a8a6039f304a503e3d85728a643ab470b`; divergence unchanged: ahead 4, behind 1.
- No commit, no merge, no rebase, no reset, no clean, no amend, no push, no tag, no publish.
- Working tree: 0 staged; 34 modified tracked files (27 pre-existing + the 7 clean files this
  session changed); 19 pre-existing untracked entries plus this record.
- `git diff --check`: clean.
- Version: **1.0.4** unchanged.

### 8.1 Why the focused commit was not created

The MIR fix extends Session 138's uncommitted CFG-aware operand path (`needs_cfg`,
`FnBuilder::eval_operand`'s if/while/loop/match arms). That code is not in HEAD. Consequences:

- A commit of only this session's hunks would compile at HEAD but **would not fix the defect**,
  and its new closure/block tests would panic — a red commit.
- A commit green at HEAD would have to include the Session 129–138 hunks in
  `src/mir/lower.rs`, `tests/mir.rs`, `tests/closures.rs`, and (for a coherent tree) the rest of
  that uncommitted M2 work — several hundred lines that are not this session's work.

The session's safety rules forbid absorbing unrelated dirty work, so the commit was stopped
rather than forced, and the verified work is preserved in the working tree.

### 8.2 Exact unblocking sequence

1. Commit the completed M2 work that is still uncommitted (Session 140 §11 items 2–3): the
   roadmap lock, `stdlib/{http,json}.mink` + npm mirrors, `src/{hir,mir}/lower.rs`,
   `src/backend/emit/runtime.rs`, the `tests/*` additions, the `httpserver` module and suite, and
   Session 128–138 records — keeping the branding hunks and untracked artifacts out.
2. Then commit the M3 P2 changes: the seven clean files above, this record, and only the M3 P2
   hunks in `src/backend/emit/runtime.rs`, `src/mir/lower.rs`, `tests/mir.rs`,
   `tests/closures.rs`, `tests/network_lib.rs`.
3. Re-run `cargo test --test network_lib --test mir --test closures` and `git diff --check`
   before that second commit. No push.
