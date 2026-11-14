# Session 278 — M7 Phase B: language conformance, ownership/runtime, specification and distribution

**Date:** 2026-11-14
**Programme:** M7 — Compiler Maturity & Language Completeness, Phase B
**Starting HEAD:** `41314d8a622dfdc1236afc3f5addddbce3fd57b1` (`41314d8`) — verified, equal to `origin/main` at start
**Branch:** `main`, target Windows x86_64
**Status:** Gates A–D executed; four confirmed defects fixed with permanent regressions.

This record is self-contained so a later session can resume without the
conversation. It covers the repository verification, the four gate audits, the
defect register with root causes and fixes, the conformance and contract
matrices, the exact verification evidence and the remaining limitations.

---

## 1. Repository state at start

| Item | Result |
| --- | --- |
| HEAD | `41314d8` — matches the reported checkpoint exactly |
| `origin/main` | `41314d8` — identical after `git fetch` (verified by `git branch -vv`) |
| Worktree | clean apart from four untracked user files, preserved untouched: `MINK UI SETS OFFICIAL.zip`, `freebuff-chat-2026-10-01T19-08-00.600Z.md`, `nl1.exe`, `nl1.mink` |
| Previous session (277) | Committed and documented (`5fef841`, `733599a`, `41314d8`); its two defects (D1 deep-input crash, D2 grouped-literal `check`/`build` split) were re-verified as fixed before this session's work |
| Compiler version | 1.0.5 (`Cargo.toml`), `mink --version` → `mink 1.0.5` |
| Toolchain | rustc 1.97.1, cargo 1.97.1, Python 3.11.9, npm 11.9.0 / node 24.14.0 |
| Standard library | 27 modules, 462 public functions (see §7) |
| Baseline regression | **92 result blocks, 3,448 passed, 0 failed, 9 ignored**, exit 0 — reproduces the Session 277 figure exactly |

Nothing was reset, cleaned or rewritten. No tag, release, npm publication or
version change was made.

## 2. Scope

Four gates, as required by the milestone:

- **A** — a traceable front-end conformance corpus.
- **B** — a traceable ownership/memory/runtime corpus.
- **C** — a specification-to-implementation conformance matrix.
- **D** — a module-by-module standard-library contract audit and a rebuilt,
  verified local distribution.

The audit-and-fix process was applied to every gate: inventory → hypothesis →
reproduce → isolate → regression test → smallest fix → targeted test →
subsystem tests → full regression → record.

## 3. Defect register

All four defects were reproduced first, isolated to a stage, fixed at the root
cause, and covered by permanent tests in `tests/frontend_conformance.rs`,
`tests/ownership_runtime_conformance.rs` and `tests/release.rs`.

### S278-D1 — A flat function was rejected as "nests too deeply" (front end)

**Symptom.** `mink check` rejected a function with roughly 2,050 statements —
`fn main() { let mut s = 0; s = s + 1; … }` — with
`E-P33: source nests too deeply for the compiler to analyze`. The code is
perfectly flat; the real syntax limit was nowhere near.

**Root cause.** `Parser::expr_nodes` (the `MAX_EXPR_CHAIN` budget, 2,048 operator
nodes) was reset in `parse_expression` only when `self.depth == 0`.
`parse_block_body` enters the nesting guard, so inside any function body
`depth >= 1` and the budget was never reset. Every operator node in the whole
function accumulated into one budget, so the *statement count* of a function
silently became a nesting limit. The documented contract is that the budget
bounds **one outermost expression** (`CORE_GRAMMAR.md` §34).

**Fix.** `src/parser/mod.rs`: `parse_statement` resets `expr_nodes` at every
statement, so each statement's expressions share one budget and a long flat
function is not rejected. A single expression still cannot exceed 2,048
operator nodes.

**Verified.** A 3,000-statement function compiles, runs and returns `3000 % 7`.
The 5,000-term single expression is still rejected. `A-DIAG-07`.

### S278-D2 — A prefix-operator chain crashed the compiler (front end)

**Symptom.** `mink build` on `return ----…-1;` with 8,000 `-` operators died
with `thread 'mink' has overflowed its stack`, exit `3221225725` — no
diagnostic. `!`, `~`, `&`, `*` chains behaved the same.

**Root cause.** `parse_unary` recursed into `parse_unary` for every prefix
operator without calling `enter_nesting`, bypassing the `MAX_NESTING` guard
added in Session 277. Chained assignment (`a = b = …`) and chained ranges
(`1..2..…`) had the same unguarded self-recursion. The later pipeline stages
also recurse over the resulting tree, so the crash could surface in any stage.

**Fix.** `src/parser/mod.rs`: `parse_unary` is now a guarded wrapper around a
new `parse_unary_operator` (one nesting level per prefix operator); the
recursive `parse_assignment` and `parse_range` calls are guarded with
`enter_nesting`/`leave_nesting`.

**Verified.** 8,000- and 40,000-operator chains report one `E-P33` with no
stack-overflow text; 100 nested prefix operators still compile. `A-DIAG-05`.

### S278-D3 — Large stack frames produced crashing executables (backend)

**Symptom.** `let a = [1; 3,048 elements]; return a[0];` compiled successfully
and the produced `.exe` died before its first statement with
`STATUS_ACCESS_VIOLATION` (`0xC0000005`, bash exit 139). 3,047 elements worked.
The boundary was not a compiler constant: 2,500 elements plus 600 extra locals
also crashed, while 2,500 plus 300 did not.

**Root cause.** The emitted prologue reserved the whole frame with one
`sub rsp, N`. The first element of an array literal lives at the **bottom** of
its slot, so the first store jumped ~24 KiB below `rbp`. Windows grows a
thread's stack only when the guard page is touched in order; jumping past it
faults instead of growing. A debug-API probe confirmed the faulting instruction
(`mov [rcx], rax`) at the first array store and the target address 24,392 bytes
below `rbp` — exactly the last committed page boundary. Any function whose
frame exceeded the initially committed stack region was affected: large arrays,
large aggregates, many locals.

**Fix.** `src/backend/emit/x86_64.rs`: new `emit_frame_allocation`. Frames up to
one 4 KiB page still use a single `sub rsp`; larger frames are reserved one page
at a time, reading one byte at each new `rsp` (the standard stack-probe shape),
so every page is committed in order. `rax` is scratch: the prologue runs before
parameter copies and before any statement.

**Verified.** 3,100-, 5,000-, 6,000- and 20,000-element arrays build and return
the correct elements; the 3,000-statement function (48 KiB frame) runs. `B-RT-11`
plus `A-DIAG-07`.

### S278-D4 — `json_parse` crashed the program on deeply nested input (stdlib)

**Symptom.** Parsing 600 nested arrays (`"[[[…]]]"`) with `json_parse` killed
the generated program with `0xC00000FD` (stack overflow). 400 levels worked;
5,000 levels happened to return a clean error only because the input exceeded
the arena-geometry check.

**Root cause.** `_parse_array` → `_parse_value` → `_parse_object` →
`_parse_value` recursed once per nesting level with no depth bound. The
generated program has a 1 MiB stack, and roughly 500 levels exhausted it. A
library call on untrusted input therefore terminated the process instead of
returning its documented `0` error.

**Fix.** `stdlib/json.mink` (and `npm/mink/stdlib/json.mink`, kept
byte-identical by an existing guard): `_parse_value`, `_parse_array` and
`_parse_object` take a `depth` argument; `_JSON_MAX_DEPTH = 128` and deeper
input returns `0`. Public API unchanged. Documented in
`STANDARD_LIBRARY_REFERENCE.md` §7.

**Verified.** 100 and 128 levels parse; 200, 600, 1,000 and 2,000 levels return
the error path with exit 1 instead of crashing. `B-LIB-01`–`B-LIB-03`.

### Distribution defect (found, not a new code defect)

`npm/mink/bin/mink.exe` was still the **stale** compiler: it reported
`mink 1.0.5` but still produced `E-B10` for `let x = (1);` — it predated both
Session 277 fixes. This confirms the historical issue reported in the milestone
brief. The binary was rebuilt from the current verified source and a behavioural
staleness guard was added; see §8.

## 4. Gate A — front-end conformance corpus

**Corpus:** `tests/frontend_conformance.rs`, 51 tests, each with an
`A-<area>-<n>` identifier and a stated contract; run through the real CLI.

| Area | Cases | Coverage |
| --- | --- | --- |
| Lexical analysis | `A-LEX-01`–`06` | reserved keywords, identifier shape, unterminated string/comment, comments-only input |
| Literals and escapes | `A-LIT-01`–`08` | hex/bin/oct/underscore integers, invalid radix digit, oversized integer, `i64::MIN`, every string escape, invalid escape, char arity, float forms |
| Operators | `A-OP-01`–`07` | precedence (`*` over `+`, shifts vs `+`, bitwise), associativity, grouping, unary precedence with groups, short-circuit |
| Statements and control flow | `A-STMT-01`–`06` | if/else, while, inclusive/exclusive ranges, loop/break value/continue, block expressions, empty statements |
| Functions and types | `A-FN-01`–`06` | declarations/calls, recursion, generics, annotation mismatch, no implicit conversion, trailing commas in four constructs |
| Aggregates | `A-AGG-01`–`06` | structs + destructuring, tuples + destructuring, arrays + computed index, empty array, enums + match, or/range/guard patterns |
| Modules | `A-MOD-01`–`02` | stdlib module load and resolution, missing module `E-S01` |
| Diagnostics, recovery, limits | `A-DIAG-01`–`07` | multiple errors reported, malformed input never panics, binary input, nesting limits, prefix-chain bound (D2), expression-chain bound, flat-function regression (D1) |
| `check`/`build` parity | `A-PARITY-01`–`03` | grouped literals in 11 positions, invalid programs rejected by both stages with no artifact, unit `main` |

**Result: PASS.** Positive, negative and boundary cases are present for every
listed feature area; parser recovery and termination are covered; interactions
(grouped literals inside aggregates, matches, generics and calls) are covered;
and conformance cases from this session's defects are permanent.

## 5. Gate B — ownership, memory and runtime corpus

**Corpus:** `tests/ownership_runtime_conformance.rs`, 29 tests, each executed
natively (build + run), with `B-<area>-<n>` identifiers.

| Area | Cases | Contract pinned |
| --- | --- | --- |
| Ownership transfer and borrows | `B-OWN-01`–`09` | use-after-move `E-S10`, double free rejected, move into callee, return ownership, literal copies, partial struct move, tuple element move + whole-tuple refusal, array element moves, borrow vs move `E-S12` |
| Cleanup | `B-DEFER-01`–`06` | LIFO order, early return, per-iteration cleanup, `break`/`continue`, `defer move` exactly-once free, nested-block scope |
| Runtime limits and traps | `B-RT-01`–`11` | 1,024-slot table / `E-R03` 103, 1 GiB request / `E-R02` 102, 3,000 allocation cycles, leak check `E-R06` 106, bounds `E-R10` 110, `/0` `%0` `E-R14` 114, `i64::MIN / -1` `E-R15` 115, wrapping overflow, shift masking, no artifact on failed build, deterministic termination, large frames (D3) |
| Library memory safety | `B-LIB-01`–`03` | bounded JSON nesting (D4), moderate nesting still parses, malformed JSON is a clean error |

**Result: PASS.** Every listed Phase B category has at least one executed case;
compiler-generated cleanup paths (`defer`, early return, break/continue,
per-iteration) are exercised as real programs, and resource limits are measured
rather than assumed.

## 6. Gate C — specification-to-implementation conformance

Method: every heading in `CORE_GRAMMAR.md`, `CORE_LANGUAGE.md` and
`TYPE_SYSTEM.md` was classified against the implementation and the existing
suites. Documentation examples were compiled (§6.1). Aspirational
`TYPE_SYSTEM.md` sections are explicitly future-design text and are classified
as **DEFERRED BY DESIGN**, not as discrepancies.

| # | Feature | Specification | Implementation | Tests | Status |
| --- | --- | --- | --- | --- | --- |
| C1 | Lexical model, identifiers, keywords | `CORE_LANGUAGE.md` §3–§5; `CORE_GRAMMAR.md` §4 | `src/lexer/` | `tests/lexer.rs` (50), `A-LEX-*` | PASS |
| C2 | Literals and escapes | `CORE_LANGUAGE.md` §6 | lexer + backend literal decode | `tests/lexer.rs`, `A-LIT-*` | PASS |
| C3 | Program structure, declarations, blocks, statements | `CORE_GRAMMAR.md` §5–§7 | `src/parser/` | `tests/parser.rs`, `semantics.rs`, `A-STMT-*` | PASS |
| C4 | Expressions and operators | `CORE_GRAMMAR.md` §8; `CORE_LANGUAGE.md` §11–§12 | parser/typecheck/MIR | `tests/parser.rs`, `scalar_types.rs`, `A-OP-*` | PASS |
| C5 | Structs and arrays | `CORE_GRAMMAR.md` §11 | parser/typecheck/layout/backend | `tests/aggregate.rs`, `A-AGG-01/03` | PASS |
| C6 | Enums, data variants, discriminants | `CORE_GRAMMAR.md` §12, §14, §15 | parser/typecheck/backend | `tests/enums.rs`, `sum_types.rs`, `discriminants.rs` | PASS |
| C7 | Pattern matching, or/range patterns, guards | `CORE_GRAMMAR.md` §13, §20 | parser/typecheck/MIR | `tests/pattern_matching.rs`, `richer_patterns.rs`, `A-AGG-06` | PASS |
| C8 | Function and let type annotations | `CORE_GRAMMAR.md` §17–§19 | parser/typecheck | `tests/function_annotations.rs`, `let_annotations.rs` | PASS |
| C9 | Null and absence | `CORE_LANGUAGE.md` §10; `CORE_GRAMMAR.md` §18 | typecheck/layout/backend | `tests/scalar_types.rs` | PASS |
| C10 | Block and if expressions | `CORE_GRAMMAR.md` §21 | parser/HIR/MIR | `tests/if_expressions.rs`, `A-STMT-05` | PASS |
| C11 | Tuple types, expressions, destructuring | `CORE_GRAMMAR.md` §22, §24 | parser/typecheck/layout | `tests/tuples.rs`, `tuple_destructure.rs` | PASS |
| C12 | `while`/`loop` expressions and break values | `CORE_GRAMMAR.md` §23 | parser/MIR | `tests/loop_expressions.rs`, `A-STMT-04` | PASS |
| C13 | Struct destructuring | `CORE_GRAMMAR.md` §25 | parser/semantics | `tests/struct_destructure.rs` | PASS |
| C14 | Match expressions | `CORE_GRAMMAR.md` §26 | parser/typecheck/MIR | `tests/match_expressions.rs` | PASS |
| C15 | Modules (`mod`) | `CORE_GRAMMAR.md` §28 | `src/driver.rs` | `tests/modules.rs`, `modules_check.rs`, `A-MOD-*` | PASS |
| C16 | `use` declarations | `CORE_GRAMMAR.md` §29 | parser + semantics | `tests/modules_check.rs` | PASS (documented limit: `use` does not load files; qualified paths unsupported) |
| C17 | `pub` visibility | `CORE_GRAMMAR.md` §30 | parser/ast only | `tests/modules.rs` | PASS (documented limit: parsed, not enforced) |
| C18 | Closures and lambdas | `CORE_GRAMMAR.md` §31 | parser/typecheck/monomorphize | `tests/closures.rs` | PASS |
| C19 | Struct patterns in match arms | `CORE_GRAMMAR.md` §32 | parser/typecheck/MIR | `tests/struct_match.rs` | PASS |
| C20 | `defer` and `defer move` | `CORE_GRAMMAR.md` §33 | semantics/ownership/MIR | `tests/defer*.rs`, `B-DEFER-*` | PASS |
| C21 | Nesting and chain limits | `CORE_GRAMMAR.md` §34 | `src/parser/mod.rs` | `compiler_robustness.rs`, `A-DIAG-04`–`07` | PASS (corrected this session: D1, D2) |
| C22 | Integer arithmetic and shift contracts | `CORE_GRAMMAR.md` §35 | typecheck/backend/runtime | `ownership_runtime_conformance.rs`, `adversarial.rs` | PASS |
| C23 | Type inference and primitives | `TYPE_SYSTEM.md` §2–§4 | `src/typecheck/` | `tests/typecheck.rs` (163) | PASS |
| C24 | Composite types, structs, enums, sum/optional/result | `TYPE_SYSTEM.md` §5–§10 | typecheck/layout | `aggregate.rs`, `enums.rs`, `option_result.rs` | PASS |
| C25 | Generics | `TYPE_SYSTEM.md` §11 | typecheck + monomorphize | `tests/generics.rs` | PASS |
| C26 | Collections | `CORE_LANGUAGE.md` §18 | runtime + `stdlib/collections.mink` | `collections.rs`, `collections_lib.rs` | PASS |
| C27 | Runtime semantics and memory model | `CORE_LANGUAGE.md` §21; `MEMORY_MODEL.md` | runtime + ownership | `ownership.rs`, `runtime.rs`, `B-*` | PASS |
| C28 | Generic constraints, traits/interfaces, aliases, newtypes, function types | `TYPE_SYSTEM.md` §12–§16 | not implemented | — | DEFERRED BY DESIGN (the document states these as future requirements) |
| C29 | Reflection, dynamic values, ABI/FFI | `TYPE_SYSTEM.md` §22–§25 | not implemented | — | DEFERRED BY DESIGN (listed as open decisions, §29) |
| C30 | CLI behaviour | `README.md`, `--help` | `src/main.rs`, `src/driver.rs` | `tests/cli.rs`, `release.rs` | PASS |

No row is an unclassified discrepancy. Rows C16/C17 carry documented
limitations already stated in the grammar; C28/C29 are future-design text.

### 6.1 Documentation examples

Every ```` ```mink ```` block containing `fn main` in `README.md` and
`docs/language/`, `docs/runtime/`, `docs/core/` was compiled with the release
binary: **33 blocks, 27 pass `mink check`**. The 6 failures are five
*deliberately invalid* examples in `DEBUGGING_AND_TESTING.md` (including a `...`
placeholder) and one multi-file example in `LANGUAGE_BASICS.md` whose sibling
`helper.mink` is not part of the extracted block. No valid single-file example
fails.

### 6.2 Unresolved ambiguities carried forward

1. **Shift counts ≥ 64** mask instead of trapping (`CORE_GRAMMAR.md` §35).
   Implemented and tested; the choice is documented, not resolved as a design
   decision.
2. **Overflow wraps in all build modes**; whether a debug mode should trap is
   undecided (§35).
3. **`pub` is not enforced** (§30). Parsed and recorded; enforcement is a later
   milestone. Compatibility impact: making `pub` meaningful would turn
   currently accepted cross-module private access into errors, so it requires a
   documented compatibility decision.
4. **`use` does not load files and qualified paths are unsupported** (§29).
5. **Nesting limits are implementation bounds** (512 / 2,048). If generated code
   legitimately exceeds them, the answer is an iterative traversal, not a larger
   constant.

Recommendations are deliberately not implemented here; no language semantics
were changed by this session. The D1/D2 fixes add diagnostics for input that
previously crashed or was wrongly rejected; D3/D4 fix code generation and a
library bound. None changes the meaning of a program that already compiled
correctly.

## 7. Gate D — standard-library contract audit

Inventory verified from the current tree: **27 modules, 462 public functions**
(and 260 `_`-prefixed private helpers). All 27 modules typecheck standalone via
`mod <name>;` (27/27), the module index in
`STANDARD_LIBRARY_REFERENCE.md` §2 lists exactly the 27 files on disk, and the
two existing drift guards pass:

- `tests/stdlib_docs.rs` — every public function is mentioned in the reference.
- `tests/stdlib_signatures.rs` — every Section 26 signature matches the source
  text after whitespace normalisation, and no public function is missing.

Contract evidence per module (public API count from source; tests are `#[test]`
cases in the module's suite; "ref" is the evidence marker in the reference,
which records the 2026-11-05 M6 audit rather than this session's execution):

| Module | Purpose | Public | Tests | Ref | Audit result / known limitation |
| --- | --- | --- | --- | --- | --- |
| `assert` | test assertions | 7 | 16 | EXEC | PASS |
| `collections` | `Vec<T>` helpers | 19 | 71 | EXEC | PASS |
| `crypto` | HMAC/HKDF/secure random | 10 | 18 | SOURCE | PASS (suite executes it; ref marker is historical) |
| `csv` | RFC 4180 row parse/write | 5 | 14 | EXEC | PASS |
| `encoding` | hex/base64/URL/UTF-8 | 27 | 76 | PARTIAL | PASS, boundary-probed |
| `environment` | `rt_env_*` wrappers | 5 | 3 | EXEC | PASS |
| `filesystem` | paths, files, dirs, CWD | 27 | 47 | EXEC | PASS |
| `hashing` | FNV-1a, djb2, SHA-256 | 6 | 29 | SOURCE | PASS (suite executes it) |
| `http` | HTTP/1.1 client | 33 | 68 | SOURCE | PASS (suite executes it) |
| `httpserver` | HTTP/1.1 server helpers | 33 | 10 | SOURCE | PASS |
| `json` | arena JSON parser/serializer | 20 | 68 | EXEC | **PASS — defect D4 fixed** (nesting ≤128, reference §7) |
| `logging` | leveled logging to stderr | 17 | 13 | EXEC | PASS |
| `math` | int/float math, trig, logs | 53 | 106 | PARTIAL | PASS |
| `network` | TCP/UDP sockets, polling | 55 | 65 | SOURCE | PASS (not re-executed against the internet this session) |
| `option` | `Option<T>` declaration only | 0 | — | SOURCE | PASS (declaration-only by design; `option_result.rs` covers usage) |
| `process` | child process run/capture | 10 | 45 | EXEC | PASS |
| `random` | xorshift64* PRNG | 10 | 15 | SOURCE | PASS |
| `re` | backtracking regex | 10 | 33 | EXEC | PASS |
| `result` | `Result<T,E>` declaration only | 0 | — | SOURCE | PASS (declaration-only by design) |
| `sqlite` | in-memory SQL engine | 9 | 18 | EXEC | PASS |
| `strings` | search/transform/split/parse | 30 | 90 | EXEC | PASS |
| `tasks` | deferred-task queue | 4 | 4 in `async_lib` | SOURCE | PASS (covered by the async suite) |
| `threads` | threads and locks | 7 | 24 | SOURCE | PASS |
| `time` | clock, calendar, strftime | 30 | 29 | EXEC | PASS |
| `tls` | TLS client with validation | 9 | 22 | SOURCE | PASS (certificate validation not re-probed against the public internet) |
| `zip` | ZIP builder/reader | 18 | 11 | PARTIAL | PASS |
| `zlib` | deflate/inflate, crc/adler | 8 | 15 | PARTIAL | PASS |

Boundary probes run this session (not part of the module suites): malformed and
truncated JSON, 100/128/200/600/1,000/2,000-level nesting, invalid base64,
inverted and negative `str_sub` ranges, empty-delimiter `str_split`,
unterminated CSV quote, pathological regex, corrupt zlib and ZIP input. Two
contract gaps were found:

- **D4** (fixed): deep JSON nesting crashed the process instead of returning 0.
- No other defect: every other probe returned the documented error/ownership
  behaviour. `str_sub` with a negative or inverted range returns an allocated
  empty string (caller-owned, so the exit-time leak scan reports 106 if it is
  not freed) — consistent with the documented Rust-like range semantics and the
  ownership contract, not a defect.

**Result: PASS** for the audit (all 27 modules inventoried, contract sources and
tests mapped, drift guards green, discrepancies classified).

## 8. Distribution integrity

**Stale-binary finding (confirmed).** Before this session,
`npm/mink/bin/mink.exe` (4,028,416 bytes, dated 2026-11-05) reported
`mink 1.0.5` but failed `let x = (1);` with `E-B10` — it predates the Session 277
fixes. Nothing is published, so nothing was inconsistent publicly; it would have
shipped defects.

**Rebuild.** `npm/mink/bin/mink.exe` was rebuilt from the current source with
`cargo build --release` and copied from `target/release/mink.exe`. The bundled
and built files are byte-identical
(SHA-256 `ec1063f3074a48817f880a64c4b7d230cb65ac1b626c73f74b6c0de9487d38e3`,
4,261,888 bytes) and report `mink 1.0.5`. The bundled `stdlib/json.mink` was
copied from `stdlib/json.mink` after the D4 fix; all 27 bundled modules are
byte-identical to the repository (enforced by `s107_npm_stdlib_bundle_matches_repo_stdlib`).

**New safeguard.** `tests/release.rs::s278_bundled_npm_compiler_is_not_stale`
runs the *packaged* binary and requires: the crate version, a grouped literal
that builds and runs, a bounded `E-P33` (not a stack-overflow crash) on 1,000
nested parentheses, and acceptance of a flat 3,000-statement function. It was
observed **failing** against the stale binary and **passing** after the rebuild.
Version equality alone (the pre-existing guards) could not have caught this.

**Local staging and archive inspection.** A clean staging directory was built
from `npm/mink` (excluding `*.tgz`), `npm pack` produced
`p4inz-code-mink-1.0.5.tgz` (1.5 MB packed / 4.8 MB unpacked, 35 files, shasum
`cfad0287401099193417cab61a1222f3174dee52`), and the archive was extracted and
inspected:

- Contents: `package.json`, `LICENSE`, `bin/mink.exe`, all 27
  `stdlib/*.mink`, and the five `windows/` assets — exactly the manifest's
  `files` promise, no `*.tgz`, `node_modules`, logs, build outputs or secrets.
- The packaged `bin/mink.exe` SHA-256 equals the fresh release build and the
  repository's bundled copy.
- Extracted-package CLI checks pass: `mink 1.0.5`; `((1))` builds and exits 1;
  300 nested parentheses compile; 1,000 nested parentheses report `E-P33`;
  3,000 flat statements are accepted; 8,000-operator chains report `E-P33`.

No publication, authentication, version bump, tag or release was performed.

## 9. Verification evidence

Commands were run from the repository root on Windows x86_64 with the toolchain
in §1.

| Check | Command | Result |
| --- | --- | --- |
| Debug regression | `cargo test --no-fail-fast` | **94 result blocks, 3,529 passed, 0 failed, 9 ignored**, exit 0 |
| Release build | `cargo build --release` | exit 0, 62 library warnings (pre-existing), 4,261,888-byte `mink.exe` |
| Release acceptance | `python tools/release_acceptance.py --session 278 --label m7-phase-b` | build / test-compile / regression pass; **94 suites, 3,529 passed, 0 failed, 9 ignored**; artifact 4,261,888 bytes, SHA-256 `d924b300f93108efd06f3fc22d6806a0f7f2e0fee4fbd6c16f92979dd75d492e`; report `target/acceptance/278/20261114T124054Z-41314d8-m7-phase-b/` |
| Formatting | `rustfmt --check` on changed files | clean; `cargo fmt --check` still lists the same **pre-existing** 14-file baseline and no file changed here |
| Static analysis | `cargo clippy --all-targets` | no warning or error in any file changed by this session (the pre-existing 113-warning baseline is unchanged in kind) |
| New corpora | `cargo test --test frontend_conformance`, `--test ownership_runtime_conformance` | 51 passed / 29 passed, 0 failed |

Baseline was 3,448 passed; the delta is the 51 + 29 + 1 new tests (81). Target
count grew by the two new integration targets. No assertion was weakened, no
test was ignored, and no tolerance was raised.

The acceptance run above was made with the work uncommitted, so the runner
reported `PARTIAL` for tree-dirtiness only; its three stages all passed. §10
records the clean-tree rerun. The artifact hash differs between target
directories because the Cargo target path is embedded (as recorded in Session
276), so bit-reproducibility across paths is not claimed.

### 9a. Clean-tree acceptance rerun

See §10.1; the numbers there are the runner's own report, not a transcription
of §9.

## 10. Commits and clean-tree verification

| Commit | Content |
| --- | --- |
| `5ffc9db` `fix(parser): scope the operator budget per statement and guard prefix chains` | S278-D1, S278-D2 + `tests/frontend_conformance.rs` |
| `84f5d91` `fix(backend): probe stack pages when reserving large frames` | S278-D3 + `tests/ownership_runtime_conformance.rs` |
| `d3fa337` `fix(stdlib): bound JSON nesting depth` | S278-D4 (stdlib + npm bundle) |
| `b3d40c5` `build(npm): rebuild the bundled compiler and guard against staleness` | rebuilt binary + `tests/release.rs` guard |
| `docs(session-278): record the M7 Phase B conformance audit` | this record, roadmap, language/implementation docs |

### 10.1 Clean-tree acceptance

The acceptance run in §9 was made while these changes were uncommitted, so the
runner reported `PARTIAL` for tree dirtiness even though all three stages
passed. The clean-tree rerun was made after the documentation commit:

| Item | Value |
| --- | --- |
| Command | `python tools/release_acceptance.py --session 278 --label clean-tree` |
| Verdict / exit code | **PASS** / 0 |
| HEAD | `ae2c3b8` (`ae2c3b8b9062d620b7f506b4fae79536b0d1c5c6`) |
| Stages | build pass (28.91 s), test-compile pass (13.36 s), test pass (108.84 s) |
| Regression | **94 suites, 3,529 passed, 0 failed, 9 ignored** |
| Artifact | 4,261,888 bytes, SHA-256 `727ff863b754b218d556403fa54a6266230a929ddb0d3767eabb302803fe0703`, `mink 1.0.5` |
| Report | `target/acceptance/278/20261114T124736Z-ae2c3b8-clean-tree/` |
| Limitation | the four untracked user files (§1) are present and are not part of the evidence; no tracked file differed from HEAD |

The artifact hash differs from the §9 run because the Cargo target path is
embedded. The only commits after this run are documentation.

## 11. Files changed

| File | Change |
| --- | --- |
| `src/parser/mod.rs` | per-statement operator budget; guards for prefix, assignment and range recursion |
| `src/backend/emit/x86_64.rs` | `emit_frame_allocation` page-by-page stack probe |
| `stdlib/json.mink`, `npm/mink/stdlib/json.mink` | `_JSON_MAX_DEPTH = 128` and depth threading |
| `npm/mink/bin/mink.exe` | rebuilt from current source |
| `tests/frontend_conformance.rs` | new — 51 Gate A cases |
| `tests/ownership_runtime_conformance.rs` | new — 29 Gate B cases |
| `tests/release.rs` | new bundled-binary staleness guard |
| `docs/language/CORE_GRAMMAR.md` | §34 corrected and extended (D1, D2) |
| `docs/language/STANDARD_LIBRARY_REFERENCE.md` | JSON nesting limit |
| `docs/implementation/PARSER_IMPLEMENTATION.md` | §8 and audit section |
| `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md` | stack frames and probing |
| `docs/roadmap/IMPLEMENTATION_ROADMAP.md` | dated Session 278 status |
| `docs/implementation/SESSION_278_…` | this record |

No generated test artifact, temporary build output, credential, machine-specific
value or unrelated user file was committed. `tmp/` and `target/` are ignored.

## 12. Remaining limitations and dispositions

| Item | Disposition |
| --- | --- |
| `pub` visibility parsed, not enforced | Documented (`CORE_GRAMMAR.md` §30); deferred by design; compatibility decision required before enforcement |
| `use` does not load modules; no qualified paths | Documented (`CORE_GRAMMAR.md` §29) |
| Traits/interfaces, generic constraints, type aliases, newtypes, function types | Deferred by design (`TYPE_SYSTEM.md` §12–§16) |
| Shift masking / wrapping overflow | Documented contracts, design decision open (§35) |
| `network`, `tls` not re-executed against the public internet | Historical SOURCE evidence plus their local suites; the internet-dependent TLS test is one of the nine intentional ignores |
| Fixing large frames relies on the OS stack reserve (1 MiB); a function needing more still overflows | Standard behaviour; the probe makes growth correct, and MINK does not promise unbounded stack |
| JSON nesting is capped at 128 | Deliberate library bound, documented |
| M6 external developer validation | OUTSTANDING — unaffected by this session |
| M8 | Gated — not started |
| Linux | Frozen/unsupported |
| MINK 1.0.5 npm publication | Not authorized, not performed |

## 13. Gate assessment

| Gate | Result | Evidence |
| --- | --- | --- |
| **A** Front-end conformance | **PASS** | 51 permanent cases across every listed area, positive/negative/boundary, recovery, limits, parity; D1/D2 regressions |
| **B** Ownership/memory/runtime | **PASS** | 29 executed cases across ownership, cleanup, limits, traps, lifecycle; D3 regression |
| **C** Specification conformance | **PASS** | 30-row matrix, all headings classified, 27/27 valid doc examples compile, ambiguities recorded |
| **D** Standard library and distribution | **PASS** | 27-module contract matrix, drift guards green, D4 fixed, package rebuilt, archive inspected, staleness guard added |

**M7 Phase B verdict: PASS.** Four defects were found and fixed at the root
cause, each with a permanent regression; debug and release regressions are green
at 3,529 passed / 0 failed / 9 ignored; formatting and static analysis show no
new issues; the distribution is rebuilt from verified source and provably not
stale.

This says nothing about **public release readiness**: M6 external developer
validation remains OUTSTANDING, M8 remains gated, Linux remains frozen, and
1.0.5 remains unpublished. No publication is authorized by this milestone.
