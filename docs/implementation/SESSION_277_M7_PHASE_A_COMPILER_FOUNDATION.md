# Session 277 — M7 Phase A: discovery, compiler foundation, integrity audit

**Date:** 2026-11-13
**Programme:** M7 — Compiler Maturity & Language Completeness, Phase A
**Starting HEAD:** `5f7aef359e9530b2837e8308eddcb7d45c21484c`
**Branch:** `main`, equal to `origin/main` at start
**Status:** Phase A tranche 1 complete; **Phase A remains PARTIAL**

This record covers the first engineering tranche of M7 Phase A: repository and
historical verification, the maturity matrix, the compiler-correctness probe of
the whole pipeline, the ownership/runtime audit, the language-contract work,
the standard-library audit, and two confirmed defects fixed with permanent
regressions. It does not claim Phase A completion; §9 records exactly what the
remaining gates still need.

---

## 1. Repository verification

| Item | Result |
| --- | --- |
| HEAD | `5f7aef3` — matches the reported HEAD exactly |
| Branch | `main` |
| `origin/main` | `5f7aef3` — identical after `git fetch` |
| Worktree | clean apart from four untracked user files, preserved untouched: `MINK UI SETS OFFICIAL.zip`, `freebuff-chat-2026-10-01T19-08-00.600Z.md`, `nl1.exe`, `nl1.mink` |
| Compiler package version | 1.0.5 (`Cargo.toml`), `mink --version` → `mink 1.0.5` |
| Compiler source | 68 Rust files, 69,677 lines |
| Test targets | 88 integration targets + library unit tests = 102 result blocks |
| Standard library | 27 modules, 15,756 lines |
| Baseline regression | **102 targets, 3,436 passed, 0 failed, 9 ignored** — reproduces the reported Session 274/275 figures exactly |

Nothing was reset, cleaned, or overwritten. No release action was taken: no tag,
no GitHub release, no npm publication, no version change.

## 2. Historical review

Read for this tranche: the locked roadmap and its dated status entries
(Sessions 274–276), `docs/compiler/COMPILER_ARCHITECTURE.md`,
`docs/language/CORE_GRAMMAR.md`, `CORE_LANGUAGE.md`, `TYPE_SYSTEM.md`,
`docs/runtime/MEMORY_MODEL.md`, `docs/implementation/DEFER_IMPLEMENTATION.md`,
`OWNERSHIP_IMPLEMENTATION.md`, `PARSER_IMPLEMENTATION.md`,
`HIR_IMPLEMENTATION.md`, `docs/audits/SYSTEMS_READINESS_AUDIT.md`, and the
Session 275/276 release-acceptance records.

The completed work was taken as the baseline rather than re-implemented:
element-level aggregate ownership (Session 273), loop-body ownership
(Session 272), `defer move` (Sessions 259–266), scope-exit cleanup (Sessions
249–253), struct patterns (Session 239), release acceptance automation
(Session 276). Every claim verified below was verified against the current
tree, not against a historical record.

## 3. Maturity matrix (before this tranche)

Evidence counts are `#[test]` functions in the named files. "History" is the
session record that introduced the area.

| Subsystem | Capability | Existing tests | History | Known limitations | Identified gaps | Risk | Work proposed | Acceptance criteria |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Lexer | Full token set, spans, escapes, comments, Unicode byte spans | `lexer` 50 | S03, S46 | Integer literals > i64 rejected (`E-T43`) | No token-count bound needed (linear) | Low | Probe boundaries only | Token corpus green |
| Parser | Recursive descent, precedence climbing, panic-mode recovery, delimiter stack | `parser` 116, `parser_hardening` 64 | S03–S04 | **Recursion was call-stack bounded** | **Deep input crashed the compiler** (defect D1) | **Critical** | Bound depth + chain, large stack | No crash on any input; `E-P33` diagnostic |
| Semantic analysis | Scope, shadowing, mutability, break/continue/return placement, duplicate names | `semantics` 75 | S05 | — | Not re-audited exhaustively this tranche | Low | Spot probes | 75 tests green |
| Type checker | Inference, canonical types, operators, aggregates, patterns, generics | `typecheck` 163 | S06–S07 | No implicit Int/Float conversion (verified) | Unconstrained-operand pinning already fixed in 1.0.4 | Low | Probe conversions | `E-T01`/`E-T02` verified |
| HIR | Typed, symbol-resolved tree; `Group` eliminated | `hir` 25 | S08 | **`Group` kept the parentheses' span** | **Parenthesized literal failed `build`** (defect D2) | **High** | Keep inner span | Grouped literals compile and run |
| MIR + optimizer | Basic blocks, validation, boolean folding, copy prop., DCE | `mir` 36, `optimization` 38 | S09–S10 | Boolean-only constant folding (documented) | — | Low | — | Suites green |
| Backend / codegen | x86-64 PE, aggregates, bounds checks, runtime calls, verify | `backend` 46, `adversarial` 94 | S11–S15 | Unsupported constructs rejected `E-B01+` | Literal decode relied on spans (root of D2) | Medium | Fix at HIR root | Native output verified |
| Ownership analysis | Moves, borrows, `E-S10`/`E-S11`/`E-S12`, element-level aggregate state | `ownership` 49, `references` 58, `aggregate` 59, `aggregate_returns` 52 | S15, S16, S273 | Documented aggregate limits | **No defects found this tranche** | Low | Verify behaviours | Use-after-move and double-free rejected |
| `defer` / `defer move` | LIFO scope exit on fallthrough/return/break/continue, move capture | `defer` 33, `defer_move` 74, `defer_aggregate` 40 | S249–S273 | Statement-only; capture restrictions (`E-S17`–`E-S19`) | **No defects found this tranche** | Low | Verify semantics | Ordering and exactly-once cleanup hold |
| Runtime heap | Bump + free list, 1 GiB arena, 1,024 live slots, generation retirement, leak check | `runtime` 28, `capacity` 7, `resource_lifecycle` 7, `soak` 9 | S12, S194 | Limits are deliberate and fail loudly | Limits were documented outside `docs/runtime/` | Low | Re-measure and document | Measured values in memory model |
| Diagnostics | Stable codes, spans, `check`/`build` parity | `diagnostics` 24 | S05+ | — | `check` and `build` disagreed on grouped literals (D2) | High | D2 fix | Diagnose at the correct stage |
| Language specification | Grammar + semantics records through `defer` | Doc-example harness (this session) | S03–S273 | Numeric and shift behaviour undocumented | Overflow, shift mask, division traps undocumented; nesting limits undocumented | Medium | Document contracts, mark unresolved | Spec matches implementation |
| Standard library | 27 modules; docs-coverage and signature-drift guards | 24 `*_lib` suites, ≈900 tests | S53+ | Bounded arena, ANSI paths | Full module-by-module contract audit not finished | Medium | Boundary probes | Documented drift guard green |

After this tranche, the Parser, HIR and Diagnostics rows move from the gap
state to COMPLETE for the specific defects found; the remaining gaps in the
Language specification and Standard-library rows are unchanged and are what
Phase A still owes (§9).

## 4. Confirmed defects

Both were reproduced first, isolated to a stage, fixed at the root cause, and
covered by permanent tests.

### D1 — Deep input exhausted the compiler stack (critical)

**Symptom.** The **release** compiler aborted with
`thread 'main' has overflowed its stack` and exit status 127 — a crash, not a
diagnostic — at roughly 200 nested parentheses, roughly 200 nested blocks, or
roughly 500 terms in one arithmetic chain (`1 + 1 + …`). The debug build
crashed at about 25 nested parentheses. `mink check` and `mink build` both
crashed; no message, no error code.

**Root cause.** Every stage is recursive over nested syntax: the parser is
recursive descent (`parse_expression` → precedence levels → `parse_primary` →
`parse_expression`), and semantic analysis, type checking, HIR and MIR
lowering, ownership analysis and the backend all walk the tree recursively. The
process ran on the default Windows main-thread stack (2 MiB measured here), so
depth was bounded only by the stack. `PARSER_IMPLEMENTATION.md` §8 already
recorded this as a known limitation and "a future robustness item"; it was
still live.

**Fix.**
- `src/parser/mod.rs`: `MAX_NESTING = 512` (guarded at every recursive
  production: expressions, blocks, types, patterns) and
  `MAX_EXPR_CHAIN = 2_048` (bounds the operator nodes of one outermost
  expression, which bounds the depth of the tree the later passes walk).
  Exceeding either records one `E-P33` per file and returns the existing
  `Err(())` recovery path, so statement-level panic-mode recovery takes over.
- `src/parser/error.rs`: new `ParseErrorKind::NestingTooDeep` → `E-P33`.
- `src/main.rs`: the command runs on a 64 MiB worker stack so legal input has
  room for every stage; a compiler panic now reports exit 101 instead of a
  silent success.
- Limits documented in `CORE_GRAMMAR.md` §34.
- The numbers are measured, not arbitrary: parser cost is ~6–8 KiB of stack per
  level in release (a 1 MiB stack parsed 128 levels and failed at 192). The
  limit is set at 512 because the existing parser suite deliberately treats
  300 nested groups as supported; 512 covers that with margin inside the CLI's
  64 MiB stack.

**Verified.** 200 nested parentheses and a 1,000-term chain now compile, run
and print the right values; 1,000 nested parentheses, 1,000 nested blocks,
1,000 nested types and a 5,000-term chain report a single `E-P33` with exit 1;
a failed `build` leaves no executable.

### D2 — A parenthesized literal passed `check` and failed `build` (high)

**Symptom.** `mink check` accepted `let x = (1);`, `rt_print_int((7));` and
`rt_print_str(("hi"));`. `mink build` rejected each with
`E-B10: could not decode a literal` — reported against the `(` column. The
programmer saw a backend error on a program the checker had just approved.

**Root cause.** HIR literal nodes carry no decoded value; the backend decodes
an `Int`/`Str` literal by re-reading the source text at the node's span.
`ExprKind::Group` is eliminated during HIR lowering, and the elimination kept
the **group's** span on the lowered node, so the backend decoded `"(1)"`
instead of `"1"`. A group containing a non-literal (`(1 + 2)`, `(true)`) was
unaffected because those nodes do not decode text from the node span, which is
why the defect survived a 3,436-test suite.

**Fix.** `src/hir/lower.rs`: the eliminated `Group` now keeps the **inner**
node's span. The AST still has `Group`, so tooling keeps the parentheses.

**Verified.** `(1)`, `((9))`, `rt_print_int((7))`, `rt_print_str(("hi"))` and
grouped literals inside a tuple all build and produce the right values.

## 5. Verified behaviour (no defect)

Probed deliberately; documented here so a later session does not re-open them
without new evidence.

| Area | Probe | Result |
| --- | --- | --- |
| Ownership | Use after move (`let t = s; use s`) | `E-S10` |
| Ownership | Double free through a moved binding | `E-S10` |
| Ownership | Partial struct move then use of another field | allowed; runs, exit 0 |
| Ownership | Partial tuple move then free the whole tuple | `E-S10` names the element |
| `defer` | `defer move (a, b)` frees both once on normal exit | exactly once, exit 0 |
| `defer` | Early `return` before the end of the body | cleanup runs before return, value snapshotted |
| `defer` | `continue` and `break` inside a loop | cleanup runs per iteration and on `break` |
| `defer` | Registration inside a nested block | inner cleanup at inner scope exit |
| Runtime | 1,024 live allocations / 1,025th | served / `E-R03` exit 103 |
| Runtime | Single request of 1 GiB or more | `E-R02` exit 102 |
| Runtime | 3,000 allocate/free cycles | exit 0, slots reused |
| Runtime | Unfreed allocations at exit | `E-R06` exit 106 |
| Arithmetic | `a / 0`, `a % 0` | `E-R14` exit 114 |
| Arithmetic | `i64::MIN / -1`, `i64::MIN % -1` | `E-R15` exit 115 |
| Arithmetic | `9223372036854775807 + 1` | `-9223372036854775808` (wrapping) |
| Arithmetic | `1 << 65`, `-1 >> 1` | `2` (count masked to 6 bits), `-1` (arithmetic) |
| Types | `let x: Int = 1.0;`, `let x: Float = 1;` | `E-T01` |
| Types | `1.5 + 1` | `E-T02` |
| Lexer | Unterminated string / block comment | `E-L02` / `E-L04` plus the enclosing `E-P14` |
| Compiler | Empty file, comments-only file | `check` passes, 0 tokens |
| Standard library | `str_sub("hello", 3, 2)` | empty, no panic |
| Standard library | `str_sub("hello", 0, 100)` | clamped to `hello` |
| Standard library | `str_index_of("hello", "")` | `0` |
| Standard library | `str_trim("   ")` | length 0 |

## 6. Language specification work

- **Doc examples verified.** Every ```` ```mink ```` block in
  `docs/language/`, `docs/runtime/`, `docs/core/` and `README.md` that contains
  a `fn main` was compiled: **25 of 26 pass**. The single failure is a
  deliberately multi-file example (`mod helper;`) whose sibling module is not
  in the extraction directory — a harness artefact, not a documentation defect.
- **`CORE_GRAMMAR.md` §34** new: nesting limits, what they bound, what they do
  *not* bound (program size), and the permanent test.
- **`CORE_GRAMMAR.md` §35** new: verified integer arithmetic and shift
  contracts (wrapping, division traps, `INT_MIN / -1`, masked shifts,
  arithmetic right shift, literal range `E-T43`, explicit conversions), each
  measured by native execution.
- **Unresolved design decisions recorded** rather than decided: shift counts
  ≥ 64 are masked instead of rejected; overflow wraps in every build mode with
  no checked-arithmetic mode.
- `PARSER_IMPLEMENTATION.md` §8 and its audit section, and
  `HIR_IMPLEMENTATION.md` §4, were corrected: both previously described the
  pre-fix behaviour.
- `docs/runtime/MEMORY_MODEL.md` §23a new: the runtime limits, re-measured.

No language semantics were changed. D2 changes which span a lowered node
carries; it makes previously-uncompilable programs compile and changes no
working program's behaviour. D1 adds a diagnostic for input that previously
crashed.

## 7. New tests

`tests/compiler_robustness.rs` — 12 tests, all running the real CLI binary:

- deep input: 200 nested parentheses compile and run; 1,000 nested parentheses,
  blocks and types each report exactly one `E-P33` with no stack-overflow text;
  a 1,000-term chain compiles and runs to `1001`; a 5,000-term chain is
  rejected and leaves no executable; 32 nested parentheses still work (the
  guard is not a general size cap)
- grouped literals: `(1)`, `((9))`, `rt_print_int((7))`, `rt_print_str(("hi"))`,
  and grouped literals in a tuple

The baseline suite is otherwise unchanged: no assertion was weakened, no test
was ignored, and no tolerance was raised.

### Two pre-existing tests changed, for a disclosed reason

- `tests/hir.rs::group_nodes_are_eliminated` asserted the *defective* invariant
  (the lowered node's span equals the parentheses' span). It now asserts the
  inner span, and its comment records why: that invariant is the root cause of
  D2. The assertion is not relaxed — it is inverted to the correct behaviour.
- `MAX_NESTING` was first set to 256, which made
  `tests/parser.rs::deeply_nested_parentheses_parse` fail: that pre-existing test
  deliberately treats **300** nested groups as supported (it parses on a 16 MiB
  thread). Rejecting 300 would have been a capability regression, so the limit
  was raised to **512**, which covers the existing expectation with margin
  inside the CLI's 64 MiB stack. The test itself was not touched.

## 7a. Final regression evidence

| Mode | Targets | Passed | Failed | Ignored | Exit |
| --- | --- | --- | --- | --- | --- |
| Debug (`cargo test`) | 103 | 3,448 | 0 | 9 | 0 |
| Release (`cargo test --release --no-fail-fast`) | 103 | 3,448 | 0 | 9 | 0 |

Baseline was 102 targets / 3,436 passed / 0 failed / 9 ignored; the delta is the
12 new tests in `tests/compiler_robustness.rs` plus the one new test target.
Debug and release agree, including the nine pre-existing intentional ignores
(public-internet tests).

Quality gates: `cargo fmt --check` lists the same pre-existing 14-file baseline
and **no** file changed here; `cargo clippy --all-targets` reports 113 warnings
and **none** originate in a file changed here.

Repository-native release acceptance:

```bash
python tools/release_acceptance.py --session 277 --label rerun
```

**PASS** — build pass, test-compile pass, regression pass; **92 suites, 3,448
passed, 0 failed, 9 ignored**; artifact 4,258,816 bytes, SHA-256
`4c2fd370d204a52354e61fd7a7943e43c2a1f5699aba455a1176867c9f149d9c`, built in
27.42 s in a fresh `CARGO_TARGET_DIR`. Report:
`target/acceptance/277/20261113T081912Z-5fef841-rerun/`. The only recorded
limitation is the four untracked user files in the worktree (a zip, a chat log
and an unrelated `nl1.mink`/`nl1.exe`), which are not part of this session's
evidence and were preserved untouched.

## 8. Gate assessment

Each gate is assessed independently against the Phase A completion criteria.
**None is claimed complete.**

| Gate | Result | Evidence | What is still missing |
| --- | --- | --- | --- |
| **G1** Compiler correctness | **PARTIAL — substantive** | Architecture reviewed end to end; lexer/parser/semantic/type/codegen boundaries probed; 2 confirmed defects fixed at root cause; 12 permanent regressions; debug and release regression green | Adversarial corpus is probed, not yet a permanent, categorised corpus; determinism and subprocess-hygiene checks not yet automated; codegen ABI probes not systematic |
| **G2** Ownership and runtime integrity | **PARTIAL — strong** | Ownership, `defer`, `defer move`, element-level moves and runtime lifecycle verified by native execution; limits re-measured and documented; leak evidence is the exit-time check (exit 0 required everywhere) | Aggregate/partial-move matrix is sampled, not exhaustive; no long-running handle-count soak this tranche; OS handle measurement not repeated |
| **G3** Language specification and conformance | **PARTIAL** | Example verification 25/26; §34/§35 contracts documented from measured behaviour; unresolved decisions recorded | No specification-to-implementation matrix; no conformance corpus with conformance identifiers; negative conformance not systematised |
| **G4** Standard library | **PARTIAL** | Inventory and doc/signature drift guards already exist and pass; 24 module suites ≈900 tests green; string boundaries probed | Module-by-module contract audit not finished; error/ownership contract table not produced; gap assessment not written |

**Phase A verdict: PARTIAL.** This is deliberate and honest: two critical
compiler defects are closed with evidence, one full area (ownership/runtime) is
audited with no defects found, and the specification gained its first written
numeric contracts — but G3 and G4 owe real work, and G1/G2 owe systematised
corpora rather than probes.

## 9. What Phase A still needs (remaining work, not started)

1. **G1** — a permanent, categorised adversarial corpus (valid boundary,
   invalid syntax, invalid semantic, deep nesting, large literals, aggregate
   interactions, complex control flow, compiler failure, determinism, runtime
   output), plus automated determinism and orphaned-subprocess checks.
2. **G1** — systematic debug vs release code-generation parity checks for
   arithmetic, branches, loops, calls, aggregate construction and cleanup.
3. **G2** — the aggregate/partial-move matrix as a table with a test per cell;
   `defer` interaction matrix; repeated-execution handle-count measurement.
4. **G3** — the specification-to-implementation matrix, a conformance corpus
   with identifiers and spec section links, and systematised negative
   conformance (failure stage, category, location, no executable).
5. **G4** — module-by-module contract audit, an error/ownership contract table
   per public API, and the prioritised gap assessment.
6. **G1–G4** — a release-mode regression through
   `python tools/release_acceptance.py --session 277` recorded as acceptance
   evidence.

## 10. Files changed

| File | Change |
| --- | --- |
| `src/parser/mod.rs` | `MAX_NESTING`, `MAX_EXPR_CHAIN`, depth guard, chain guard |
| `src/parser/error.rs` | `ParseErrorKind::NestingTooDeep` → `E-P33` |
| `src/main.rs` | 64 MiB worker stack; panic reports exit 101 |
| `src/hir/lower.rs` | Group elimination keeps the inner span (D2 fix) |
| `tests/compiler_robustness.rs` | new — 12 regressions |
| `docs/language/CORE_GRAMMAR.md` | §34 nesting limits, §35 numeric contracts, unresolved decisions |
| `docs/runtime/MEMORY_MODEL.md` | §23a verified runtime limits |
| `docs/implementation/PARSER_IMPLEMENTATION.md` | limits replace the old "future robustness item" |
| `docs/implementation/HIR_IMPLEMENTATION.md` | Group span invariant corrected |
| `docs/implementation/SESSION_277_…` | this record |
| `docs/roadmap/IMPLEMENTATION_ROADMAP.md` | dated status entry |

## 11. Deferred design decisions

1. Shift counts ≥ 64 mask rather than trap (`CORE_GRAMMAR.md` §35).
2. Overflow wraps in all build modes; whether debug should trap is undecided.
3. The nesting limits (512 / 2,048) are implementation bounds chosen from
   measured stack cost. If generated code ever legitimately exceeds them, the
   answer is an iterative traversal in the recursive passes, not a larger
   number — recorded so a later session does not simply raise the constant.
