# Session 140 — M3 P1: `if` statement versus if-expression in expression blocks

**Scope:** resume major development safely after Sessions 128–139; establish the exact
repository and Git state; re-read the locked roadmap and the M2 closure evidence; select the
highest-value next task from evidence rather than assuming M3; then implement the single open
**A-class** defect carried out of M2 (A5) with a root-cause fix, permanent regression
coverage, and full re-verification.

Windows x86_64 only. Linux stayed untouched and frozen. Version stayed **1.0.4**. No tag, no
publish, no push, no version change, no release-state change. M4/M5 and the rest of M3 were
not started. No pre-existing uncommitted work was modified, discarded or committed.

**Outcome:** A5 is **CLOSED**. An `if` without `else` inside an *expression block* (a closure
body, a block expression or a match-arm body) is now a statement `if`, exactly as it is in a
function body, and an `else if` chain with no final `else` is a statement chain. The
`else` requirement for an if-*expression* (`E-P26`) is unchanged. Two further defects were
proven during the audit and are recorded for the next session; neither is caused by this
change.

## 1. Starting state

Verified before any edit, not assumed from the handoff note.

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `3b93bdbb834ba2faeb63faf9e22825f279d93416` — *brand: integrate the official MINK artwork and the .mink Windows file identity* |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` — *Rename: replace the retired studio name Northbyte Studios with P4inz Interactive Labs…* |
| Divergence | **ahead 2, behind 1** |
| Merge base | `dd556a692b8290416e14703471307cc311995d2f` (tag `v1.0.4`) |
| Staged | none (empty index) |
| Modified tracked files | 27 |
| Version | **1.0.4** (`Cargo.toml`; `npm/mink/package.json` untouched) |
| `git diff --check` | clean |
| `cargo build` | success, 59 pre-existing warnings (no new warnings) |
| Local-only commits | `bb3ff1e` (M1 Windows practical parity closure), `3b93bdb` (Session 139 branding) |
| Remote-only commit | `b860837` (Northbyte Studios → P4inz Interactive Labs rename) |

Modified tracked files (all pre-existing): `LICENSE`, `README.md`,
`docs/audits/OFFICIAL_PYTHON_CAPABILITY_PARITY_AUDIT.md`, `docs/core/LANGUAGE_DESIGN.md`,
`docs/core/MINK_MASTER_SPEC.md`, `docs/ecosystem/LIBRARY_PRIORITY_FRAMEWORK.md`,
`docs/governance/GOVERNANCE_VERSIONING.md`,
`docs/implementation/SESSION_139_BRAND_ASSETS_WINDOWS_FILE_IDENTITY.md`,
`docs/roadmap/IMPLEMENTATION_ROADMAP.md`, `docs/roadmap/LINUX_X86_64_IMPLEMENTATION_PLAN.md`,
`docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md`,
`docs/roadmap/WINDOWS_PYTHON_PARITY_IMPLEMENTATION_PLAN.md`,
`docs/validation/VALIDATION_MILESTONE_1_REPORT.md`, `npm/mink/stdlib/http.mink`,
`npm/mink/stdlib/json.mink`, `src/backend/emit/runtime.rs`, `src/hir/lower.rs`,
`src/mir/lower.rs`, `stdlib/http.mink`, `stdlib/json.mink`, `tests/closures.rs`,
`tests/http_lib.rs`, `tests/json.rs`, `tests/mir.rs`, `tests/network_lib.rs`,
`tests/sqlite_lib.rs`, `tests/threads_lib.rs`.

Untracked (all pre-existing): `MINK UI SETS OFFICIAL.zip`, the Session 128–138 records,
`docs/roadmap/archive/`, `stdlib/httpserver.mink`, `npm/mink/stdlib/httpserver.mink`,
`tests/httpserver_lib.rs`, `nl1.exe`, `nl1.mink`, and a pasted chat transcript
(`freebuff-chat-2026-10-01T19-08-00.600Z.md`). All were listed in `git status` before any
edit and were left exactly as found.

## 2. Git reconciliation analysis

The divergence was inspected before deciding anything.

- **`bb3ff1e`** and **`3b93bdb`** are local only and are the M1-closure validation work and the
  Session 139 branding integration. Committed, verified, and not on the remote.
- **`b860837`** is remote only and changes exactly three files, each by one line:
  `README.md`, `docs/core/MINK_MASTER_SPEC.md`, `docs/governance/GOVERNANCE_VERSIONING.md` —
  the studio-name rename.
- The **working tree already contains that rename**: `docs/governance/GOVERNANCE_VERSIONING.md`
  in the tree is byte-identical to `origin/main`'s version, and the other two files contain
  the rename plus additional local work. `grep -rn "Northbyte"` over the tree finds no
  occurrence except the historical discussion inside the Session 139 handoff note.
- The upstream branch also moved *backwards* relative to what the Session 139 handoff recorded:
  it noted `b860837..8b00083 main -> origin/main` from a `fetch --dry-run`, while
  `git reflog show origin/main` now reads `dd556a6 → b860837` (a fetch fast-forward). The
  remote's `8b00083` was therefore removed upstream; the object still exists locally, but
  `origin/main` is `b860837`.

**Conclusion: a merge is a content no-op on all three conflicting files and is the least-risk
reconciliation** (no history rewrite, no commit or working-tree change discarded).
**No merge was performed in this session**, because the merge would have to rewrite those
three files and the working tree carries uncommitted edits in them. Any reset, rebase or
`checkout` was rejected: all three discard local commits or working-tree work. Reconciliation
is a deliberate maintainer action, documented in §11.

## 3. Roadmap decision

Read as authoritative: `docs/roadmap/IMPLEMENTATION_ROADMAP.md` (the locked M0–M9 roadmap),
`docs/implementation/SESSION_128_STRATEGIC_ROADMAP_LOCK.md`,
`docs/implementation/SESSION_138_M2_P9_FINAL_WINDOWS_GAP_AUDIT.md` (M2 closure),
`docs/implementation/SESSION_139_BRAND_ASSETS_WINDOWS_FILE_IDENTITY.md` (branding + handoff).

Established facts:

- M0 and M1 are complete; **M2 is CLOSED** by Session 138 (practical-use gate PASS).
- Session 139 (branding, `.mink` file identity) is complete and committed.
- The roadmap file still says “M2 — NEXT” in §3 and §8. That text is **stale** — it is part of
  the uncommitted Session 128 work and predates the Session 138 closure. Correcting it is a
  documentation follow-up that must be part of the commit that carries that uncommitted work
  (§11), not of this isolated commit.
- M2 left exactly one **A-class** defect open: **A5**, the closure-block `if` parse error.
  Everything else was closed or classified B (diagnostic gap) / C (documented ceiling) /
  D (capability gap) / unvalidated.

Candidates considered:

| Candidate | Value | Readiness | Why not selected |
| --- | --- | --- | --- |
| **M3 P1 — close A5** (selected) | Only open A-class defect from M2; it rejects valid, idiomatic code, so it directly contradicts “simpler to build” | Parser-only; the rest of the pipeline already supports the shape (proven by the function-body control) | — |
| M3 — `net_dial` error preservation | Real B-class diagnostic gap; root cause now proven (§9) | Needs a new runtime intrinsic in `src/backend/emit/runtime.rs`, which carries uncommitted Session 136 work | Cannot be committed in isolation this session; scheduled first in §11 |
| M3 — socket timeouts, UDP parity, chunked decoding, JSON capacity, allocator split/coalesce, SQLite durability | Genuinely useful | `D`/`C` capability and ceiling items per M2; several need memory-model or API redesign | The locked M2 rule is that only genuine A-class defects automatically become implementation candidates; these need their own justification |
| M4 — developer-experience benchmark | Directly answers the strategic question | Ready in principle | Locked sequencing is M3 (scope from M2 evidence) before M4; skipping it would leave a known A-class defect unfixed underneath the benchmark |
| M2 re-run / more benchmarking | — | — | Explicitly excluded: M2 is closed and repeated validation without a new question is waste |

**Decision: M3, first tranche = A5.** It is the only item M2 evidence classifies as an
automatic implementation candidate, it is fully isolatable (all touched paths are clean), and
it is small enough to be finished and verified properly rather than started and abandoned.

## 4. Reproduced defect (before)

```
fn main() {
    let f = |x: Int| {
        if x > 0 {
            let y = 1;
        }
        return 0;
    };
    return f(1);
}
```

| Form | Before | After |
| --- | --- | --- |
| Closure block, `if` without `else` as a non-final statement | `E-P26` at the following `return`, then a cascading `E-P01` | parses, compiles, runs |
| Same body in a plain function | compiles (control) | compiles |
| Closure block, `else if` chain with no final `else` | `E-P26` | parses, compiles, runs |
| `let x = if c { 1 };` | `E-P26` | `E-P26` (unchanged) |
| Trailing `if c { 1 } else { 2 }` in a closure | value-producing if-expression | unchanged |
| Trailing `if c { 1 }` at the end of a closure body | `E-P26` | `E-P06` — identical to the function-body diagnostic for the same text |
| Match-arm body, `if` without `else` before the trailing expression | `E-P26` | parses, compiles, runs |

## 5. Root cause

`Parser::parse_block_expr` — the parser for *expression* blocks, used for closure bodies,
block expressions, if-expression branches and match-arm bodies — routed everything that was
not a statement keyword through `parse_expression()`. `TokenKind::If` is not in that keyword
set, so `if` reached `parse_primary`, whose `If` arm is `parse_if_expr`, and
`parse_if_expr` **requires** an `else` (otherwise `ParseErrorKind::ExpectedElse`, `E-P26`).

A function body does not have this problem: `parse_block_body` delegates every item to
`parse_statement()`, where `TokenKind::If` dispatches to `parse_if` → `parse_if_stmt`, which
accepts a missing `else`. The two block parsers therefore disagreed about whether an `if`
without `else` is legal — the exact inconsistency recorded as A5.

The failure also cascaded: `parse_if_expr` returns `Err(())`, the enclosing `parse_expression`
propagates it, and the expression block then resynchronised badly enough to report a spurious
second error at the next top-level token.

## 6. Implementation

**`src/lexer/scanner.rs`** — `Lexer` derives `Clone`, with a doc comment stating why: a
snapshot of the scan position (including recorded lex errors) lets the parser attempt a
grammar alternative and roll back.

**`src/parser/mod.rs`**

1. `Parser` derives `Clone`, documented as a full parse-state snapshot (input position, lexer
   and parser errors, delimiter stack, token count).
2. In `parse_block_expr`, an `if` at statement position is now disambiguated by shape:

```rust
let expr = if self.current_kind() == TokenKind::If {
    let snapshot = self.clone();
    match self.parse_expression() {
        Ok(expr) => expr,
        Err(()) => {
            // Not an if-expression: reparse the same tokens as a statement.
            *self = snapshot;
            match self.parse_statement() {
                Ok(stmt) => stmts.push(stmt),
                Err(()) => self.recover_statement(),
            }
            continue;
        }
    }
} else {
    self.parse_expression()?
};
```

**Why this shape and not a keyword table change.** The expression form is attempted first and
is used **unchanged** when it succeeds, so every input that parsed before parses to exactly
the same AST — including an if-expression that is an operand of a following binary operator
(`if c { a } else { b } + x`), which a naive “add `If` to the statement keywords” change would
have broken. Only inputs that previously *failed* change behaviour. No new AST node, no
lowering change, no type-checker change was needed, because `StmtKind::If` already exists and
the rest of the pipeline already handles it — which the function-body control demonstrated.

**Safety properties checked, not assumed**

- `parse_statement()` dispatches `TokenKind::If => parse_if()` (`src/parser/mod.rs:1648`), so
  the fallback parses the statement form rather than re-entering the expression path.
- `recover_statement()` consumes to `;`/`}`/EOF and skips balanced brace groups, so it always
  makes progress: no infinite loop is possible.
- Rolling back with `*self = snapshot` restores the lexer (with its error list), the parser
  error list, the delimiter stack, `token_count` and the unclosed-delimiter flag, so the
  discarded speculative attempt cannot leak errors or double-count tokens.
- The speculative attempt costs one `Parser` clone per `if` in statement position inside an
  expression block. The clone copies the accumulated error list, so the cost grows in an
  already-failing file with many `if`s; it is compile-time only and was not measurable in the
  suite run.

**`docs/language/CORE_GRAMMAR.md`** §21 — added the precise rule: an `if` inside an
expression block is disambiguated by its own shape (a chain ending in `else` is an
if-expression, a chain that does not is a statement `if`), and only an if-*expression* needs
`else`. The existing §21 sentence “in statement position it is an if-statement; in expression
position it is an if-expression” was already the documented intent; the implementation now
matches it.

## 7. Tests added

**`tests/parser.rs`** — six AST-level tests (pure additions, no existing test changed):

| Test | Locks |
| --- | --- |
| `closure_block_statement_if_without_else_parses_as_a_statement` | A5's exact shape lowers to `StmtKind::If` with no else branch |
| `closure_block_else_if_chain_without_final_else_parses_as_a_statement` | the `else if` chain with no final `else` is a statement chain |
| `closure_block_statement_if_before_a_trailing_expression_parses` | the `if` is a statement and the trailing expression is still the value |
| `closure_block_trailing_if_expression_parses_as_an_expression` | a trailing `if`/`else` is still an if-expression |
| `block_expression_statement_if_without_else_parses` | the fix is not closure-specific (plain block expression) |
| `if_expression_without_else_in_value_position_still_requires_else` | `E-P26` is still reported in value position |

**`tests/if_expressions.rs`** — new end-to-end suite (build + execute), nine tests:

| Test | Locks |
| --- | --- |
| `statement_if_without_else_in_a_closure_block_builds_and_runs` | the A5 program builds and exits 0 |
| `statement_if_without_else_in_a_closure_block_executes_its_branch` | the then-block actually runs (and is skipped when false) |
| `check_and_build_agree_on_a_statement_if_in_a_closure_block` | `check` and `build` agree |
| `else_if_chain_without_final_else_in_a_closure_block_builds_and_runs` | each arm of a chain with no final `else` is reachable |
| `statement_if_without_else_in_a_block_expression_builds_and_runs` | block-expression context |
| `trailing_if_expression_in_a_closure_block_returns_its_value` | a value-producing trailing if-expression is unchanged |
| `nested_else_if_expression_in_a_closure_block_returns_the_taken_arm` | Session 138's nested-`else if` fix stays closed |
| `if_expression_without_else_in_a_value_position_is_rejected` | `E-P26` preserved for value positions |
| `trailing_if_without_else_in_a_closure_block_is_a_diagnosed_error` | a valueless trailing `if` is a structured diagnostic, never a panic and never silent `Unit` |

The end-to-end tests live in a new file rather than `tests/closures.rs` because that file is
part of the uncommitted Session 138 work; a new file keeps this session's commit isolated and
is consistent with the existing `loop_expressions.rs` / `match_expressions.rs` naming.

No test was deleted, weakened, skipped or re-pointed.

## 8. Verification

All commands run on Windows x86_64 (MSVC toolchain), Git Bash. `python3` in this environment is
the Microsoft Store alias stub, so the documented `python3.exe` shim (a copy of the working
3.11 interpreter) was prepended to `PATH` for the TLS target, exactly as Sessions 138 recorded.

| Check | Result | Classification |
| --- | --- | --- |
| `cargo build` | success, **59** pre-existing warnings — unchanged count | PASS |
| `cargo test --test parser` | **104 passed, 0 failed** (98 before + 6 new) | PASS |
| `cargo test --test if_expressions` | **9 passed, 0 failed** (new) | PASS |
| Focused batch — `adversarial` 94, `closures` 51, `hir` 25, `lexer` 50, `loop_expressions` 39, `match_expressions` 41, `mir` 35, `optimization` 38, `semantics` 13, `smoke` 163 | all `ok`, 0 failed | PASS |
| Full suite `cargo test --no-fail-fast` (76 test binaries + doc-tests) | **3,088 passed, 0 failed, 1 ignored**, exit **0** | PASS |
| `tests/tls_lib.rs` within that run | **19 passed, 0 failed, 1 ignored** (shim present) | PASS |
| `cargo build --release` | success, 59 warnings, 29.49 s | PASS |
| Release compiler (`mink 1.0.4`) builds the A5 programs and they run: statement `if` → `f(1)=0`; `else if` chain → `10 / 20 / 0`; trailing if-expression → `7 / 8` | exact | PASS |
| `git diff --check` | clean (exit 0) | PASS |
| New-defect reproducers (`net_dial`, MIR panic) | reproduced on this tree | PASS (evidence) |

The full-suite total is internally consistent with the Session 138 baseline: that session
recorded **3,073** passing tests with the shim, and this session adds exactly 15 tests
(6 + 9), giving **3,088**. Nothing else changed count.

Environment conditions encountered: the intermittent `LNK1104` link lock (also recorded by
Session 138) hit two batched runs; all test binaries were pre-built with `cargo test --no-run`
(retried), after which the run completed with exit 0. The Store `python3` alias remains a
host condition, not a product failure.

## 9. Defects found during this session

### 9.1 `net_dial` loses the Winsock error — root cause proven (B-class, not fixed)

Minimal reproducer (stdlib prepended):

```
dial=-1  err=0
```

while the same refused connection through `net_connect` reports `10061`, and a bad host name
reports `11001` — until the socket is closed:

```
connect=-1     err_after_connect=10061
close=0        err_after_close=0
badhost_connect=-1  badhost_err=11001
net_close(s)        badhost_err_after_close=0
```

**Root cause:** `net_dial` closes the failed socket (`net_close` → `rt_net_close` →
`closesocket`) before returning, and `closesocket` resets `WSAGetLastError()` to 0. The error
is destroyed by the cleanup, not by the connect.

**Fix (planned, not applied here):** capture `net_last_error()` before the close and republish
it afterwards. MINK has `rt_net_wsa_last_error` but no setter, so this needs a
`rt_net_wsa_set_last_error` intrinsic (+ `RuntimeService` variant, backend lowering, stdlib
wrapper). That work touches `src/backend/emit/runtime.rs`, which carries uncommitted
Session 136 changes, so it cannot be committed in isolation in this session and was
deliberately deferred rather than mixed into this commit.

### 9.2 Compiler panic: binary expression with an if-expression operand as a closure's trailing expression (A-class, pre-existing, not fixed)

```
fn main() -> Int { let c = true; let f = |y: Int| { if c { y } else { 0 } + 5 }; return f(37); }
```

`mink check` → `panicked at src/mir/lower.rs:571: literal_operand is only called for literals`,
exit 101. Characterised:

| Shape | Result |
| --- | --- |
| `return if c { 1 } else { 2 } + 1;` in a function | compiles |
| `(if c { … } else { … }) + 5` as a closure's trailing expression | **panics** |
| `5 + if c { … } else { … }` as a closure's trailing expression | **panics** |
| `if c { … } else { … } * 2` as a closure's trailing expression | **panics** |
| `return if c { … } else { … } + 5;` inside a closure | compiles |
| bare `if c { … } else { … }` as a closure's trailing expression | compiles |

**Not caused by this session.** Proven with the pre-change `target/release/mink.exe`
(built 2026-10-20, before this session's edit): the same input panics identically. It is the
same `literal_operand` family as Session 138's A4, in the shape the closure desugaring
(`return <body>`) still exposes. Session 138's `needs_cfg` routing covers the `return` path but
not a closure body whose trailing expression is a binary with an if-expression operand.

**Planned fix:** extend the CFG-aware operand routing on the closure-body path in
`src/mir/lower.rs` (dirty with Session 138 work — same commit-isolation constraint as 9.1),
with regressions in `tests/mir.rs` and `tests/closures.rs`.

### 9.3 `parse_block_expr` consumes a non-`}` token after a trailing expression (pre-existing, not fixed)

After `result = Some(expr); break;`, the function unconditionally bumps the current token as
the closing brace. If a trailing expression is followed by a statement (for example
`|x| { if c { 1 } else { 2 } return 0; }`), that statement's first token is consumed as `}` and
the rest of the block is misparsed. Verified byte-identical output from the pre-change release
binary on exactly that input (`E-P06` at `return`, then `E-P01` at the next line), so this is
pre-existing and unchanged by this session. Left alone: fixing it changes error recovery for a
broad class of malformed inputs and needs its own regression design. Recorded for M3.

## 10. Audit

| Area | Result |
| --- | --- |
| Correctness | The fallback parses the statement form via the same code path a function body uses; the expression form is untouched on success. Verified by AST tests + build/run tests + the pre-change binary as oracle for unchanged cases. |
| Architecture consistency | Parser-only change; no new AST node, no lowering/typecheck/runtime change — consistent with the grammar doc's documented intent. |
| API consistency | No public API, intrinsic, stdlib signature or CLI surface changed. |
| Compiler/runtime interaction | No runtime or emitter touched; no generated-code change. |
| Error handling | `E-P26` preserved for if-expressions; the trailing valueless `if` now yields the function-body diagnostic (`E-P06`) instead of a misleading `E-P26`; no panic, no silent acceptance (regression test asserts both). |
| Infinite-loop / progress safety | `parse_statement` dispatches `If`; `recover_statement` always consumes. |
| Regression coverage | 15 permanent tests; the full suite grew by exactly those 15 (3,073 → 3,088) with 0 failures. |
| Documentation | `CORE_GRAMMAR.md` §21 corrected/expanded. Historical records (Session 129, 138) were **not** rewritten: they correctly describe A5 as open at their time. |
| Release/version consistency | `Cargo.toml` 1.0.4, `mink version` 1.0.4, `npm/mink` untouched, no tag. |
| Windows x86_64 compatibility | All verification performed on Windows x86_64; release build and release-compiled binaries executed. |
| Repository cleanliness | `git diff --check` clean; no `tmp/` artifact is tracked (`tmp/` is ignored); all probes live under `tmp/m3_p1/`. |
| Scope expansion | One checkbox-style change: only the A5 defect and its coverage. `net_dial`, the MIR panic and the trailing-token bug were **reproduced and recorded, not fixed**, because each requires files that carry uncommitted work. |
| Unintended changes | Staged file list inspected before commit; the 27 pre-existing modified files and all pre-existing untracked files are excluded. |

Issue found and fixed during the audit itself: the first version of this change called
`parse_if_expr()` directly instead of `parse_expression()`, which would have dropped
surrounding operators (`if c { a } else { b } + x`) and regressed working programs. Corrected
to attempt the full `parse_expression()` and reuse its result.

Second issue found and fixed during writing the tests: two end-to-end assertions assumed
`rt_print_str`/`rt_print_int` do not terminate the line (they emit CRLF). Rewritten to encode
their outcomes in a single integer rather than loosening the assertion.

## 11. Remaining work and exact next actions

1. **Reconcile Git (maintainer decision, no push).** With a clean tree, `git merge origin/main`
   is the least-risk option: the only remote change (`b860837`) is already present in the
   working tree, so it should merge as a no-op on those three files. Do **not** reset, rebase or
   force-push.
2. **Commit the completed M2 work still sitting uncommitted** (Sessions 128–138: the roadmap
   lock, `stdlib/{http,json}.mink` + npm mirrors, `src/{hir,mir}/lower.rs`,
   `src/backend/emit/runtime.rs`, the `tests/*` additions, the `httpserver` module and suite,
   and the eleven session records) as coherent commit(s), keeping the branding README hunks and
   the untracked artefacts (`MINK UI SETS OFFICIAL.zip`, `nl1.*`, the transcript) out. This is
   the prerequisite for the next two fixes.
3. **Correct the roadmap status** in that same documentation commit: `M2 — NEXT` →
   `M2 — COMPLETE (Session 138)`, `M3 — NEXT`, and update §8's recommended next action. The file
   is currently stale because it is part of the uncommitted Session 128 work.
4. **M3 P2 — fix 9.1 and 9.2** (both now have proven root causes and minimal reproducers) with
   permanent regressions, then re-run the full suite.
5. **M3 P3 — remaining carried items** in M2 priority order: `rt_print_int`/float shared
   buffers, thread-local HTTP last-error, socket connect/read timeouts, UDP buffer parity,
   chunked decoding, then the `parse_block_expr` trailing-token bug (9.3).
6. Only after M3 is exhausted, move to **M4** (developer-experience benchmark) — MINK is
   explicitly allowed to lose there.

## 12. Final state

- Version **1.0.4**; no version change, no tag, no publish, no push, no release action.
- Compiler, runtime, standard library and npm package: **no functional change** except the
  parser behaviour described in §6.
- Linux: not touched; the ELF backend and the Linux plan remain frozen.
- Working tree: this session's files plus the pre-existing Sessions 128–139 changes listed in
  §1, which were **not** modified, staged or discarded.
- Git actions performed: one local commit of this session's isolated work. No merge, no rebase,
  no reset, no clean, no amend, no push, no tag.
