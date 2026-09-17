# Session 109 — Windows Python Parity: REPL, Regex, Directory Packages

**Date:** September 17, 2026
**Platform:** Windows x86_64 (Linux frozen)
**MINK version:** 1.0.1
**Starting commit:** `0ed300e` (Session 108 close)
**Ending commit:** `a9c65b5`
**Phase:** Windows Python Official Capability Parity — completion push

---

## 1. P1 blockers closed this session

| Matrix ID | Capability | Commit | Permanent coverage |
|---|---|---|---|
| R01 | Interactive interpreter / REPL | `202ee6f` | `tests/repl.rs` r01-r14 |
| T04 | REPL / interactive tooling | `202ee6f` | `tests/repl.rs` r13 (initial-file preload) |
| S02 | Regular expressions (`re`) | `b31432a` | `tests/re_lib.rs` s02_* (33) |
| L68 | Packages (directories / `__init__`) | `a9c65b5` | `tests/packages.rs` (5) + fixtures |
| P03 | Import package (`import pkg`) | `a9c65b5` | `tests/packages/siblings/` regression |

**Parity-blocking set: 18 → 13.**

---

## 2. Delivered capabilities

### 2.1 `mink repl` — interactive compile-eval session (R01/T04)

`src/cli.rs` adds `run_repl`/`repl_eval`/`parse_repl` and helpers. Declarations
(`fn`/`struct`/`enum`/`use`/`mod`/`const`) accumulate in a session buffer; bare
expressions and statements are compiled and executed against them through the real
`driver::build` path (a full native build+run cycle per input, not a bytecode
trampoline). Commands: `:help`, `:show`, `:clear`, `:quit`/`:q`. An optional path
pre-loads that file's declarations. Bare expressions print via `rt_print_int` first
and fall back to `rt_print_str` (no runtime reflection exists).

### 2.2 `stdlib/re.mink` — self-contained regex engine (S02)

A dependency-free backtracking engine. Pattern and subject are pre-read into
`Ptr<Int>` arenas (the JSON library's architecture) so recursive matching is not
constrained by the V1 `Str` user-call ownership contract. Supported syntax:
literals, `.`, `^`/`$`, `\d\D\w\W\s\S`, classes with ranges and negation, groups
`()`/`(?:)`, alternation `|`, greedy and lazy `* + ? {m} {m,} {m,n}`. Public API:
`re_is_match`, `re_search`, `re_match_end`, `re_full_match`, `re_find`, `re_count`,
`re_find_all`, `re_split`, `re_replace`, `re_replace_first`. A step budget bounds
pathological backtracking (verified with a `(a+)+b` probe).

### 2.3 Directory packages (L68/P03)

`src/driver.rs` `resolve_module_path` now also resolves `mod name;` to
`name/mod.mink` (the package root — MINK's `__init__` equivalent) when `name.mink`
is absent, in both the declaring directory and the stdlib roots. Child modules are
named by their `mod` declaration, so the package root is known as `name` (matching
`use name::item`). Nested packages (`name/sub/mod.mink`) compose; a flat
`name.mink` takes precedence.

---

## 3. Defect found and fixed (root cause + regression)

**Multi-module symbol collision (pre-existing).** A program with two or more sibling
modules failed to build with `E-M04 cannot lower: identifier has no corresponding
local`.

- **Root cause:** declaration-name lookups in `src/hir/lower.rs`,
  `src/typecheck/checker.rs` (both the `decls` and `fn_info` maps), and
  `src/ownership/mod.rs` were keyed by byte offset alone. Byte offsets repeat across
  source files, so two modules whose declarations share an offset collapsed onto one
  symbol; one module's items became unreachable. Confirmed by instrumentation:
  `geo_double`'s call site resolved to `SymbolId(109)` while both function items
  received `SymbolId(110)`.
- **Fix:** all four maps are keyed by `(SourceFile, byte offset)`.
- **Regression:** `tests/packages.rs::l68_two_sibling_modules_resolve_distinct_symbols`
  with the minimal reproduction fixture `tests/packages/siblings/` (two modules whose
  functions both start at offset 7).

This defect meant the module system worked only for a single child module; the
matrix's L65-L67 "VERIFIED" rows were true for file modules, but multi-module
programs were not previously exercised by a two-sibling test.

---

## 4. Test evidence (native Windows PE)

Every test below builds and runs a genuine PE through the real CLI; the runtime
leak check runs at exit.

| Suite | Result |
|---|---|
| `packages` (new) | 5 passed, 0 failed |
| `repl` (new) | 14 passed, 0 failed |
| `re_lib` (new) | 33 passed, 0 failed |
| `modules_check` | 6 passed, 0 failed |
| `cli` | 74 passed, 1 ignored, 0 failed |
| `smoke` | 13 passed, 0 failed |
| `typecheck` | 163 passed, 0 failed |
| `ownership` | 43 passed, 0 failed |
| `hir` | 25 passed, 0 failed |
| `mir` | 34 passed, 0 failed |
| `semantics` | 75 passed, 0 failed |
| `adversarial` | 93 passed, 1 ignored, 0 failed |
| `aggregate` | 59 passed, 0 failed |
| `aggregate_returns` | 52 passed, 0 failed |
| `json` | 61 passed, 0 failed |
| `scalar_types` | 27 passed, 0 failed |
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets` | exit 0; no new warnings from this session's changes |

### Known infrastructure flake (classified, not product)

Three `strings_lib` tests (`p04_parse_int_invalid`, `s31_to_upper`, `s38_reverse`)
failed under a full parallel run and pass 3/3 in isolation. The cause is external:
Windows Defender quarantined the harness's temp executables mid-run
(`mink_str_test_*.exe` / `*.mink` in `%TEMP%`, visible in the Defender protection
history). This is a test-harness/environment issue, not a product failure; the
affected capability is not marked VERIFIED on the strength of a failing run.

---

## 5. Remaining P1 blockers (13)

Matrix §10 recomputed by a direct scan of the 232 rows (all sums checked):

| Wave | Remaining P1 |
|---|---|
| B (core language/data) | L34 (exceptions) |
| D (networking/compression) | S41 (SQLite), S42 (zlib/gzip), S44 (zip), S62 (TLS) |
| E (concurrency/async) | R19 (threads), R20 (async), S71 (threads+locks), S73 (async/await) |
| F (packaging) | P04 (site-packages), P05 (pip), P06 (resolver), P09 (virtual env) |

Waves A, C and G are now empty. Aggregate distribution: P1 13 · P2 91 · P3 54 ·
no-gap 74; status VERIFIED 86 · MISSING 83 · PARTIAL 34 · INTENT. DIFF. 14 · N/A 8 ·
EXECUTION VERIFIED 2 · PLANNED 3.

**The Windows Python Capability Parity Completion Gate has NOT passed**: 13 actionable
P1 blockers remain, all of them large subsystems.

---

## 6. Repository state

| Item | Value |
|---|---|
| HEAD | `a9c65b5` |
| origin/main | `a9c65b5` |
| ahead/behind | 0/0 |
| Working tree | clean |
| Commits created | 3 (`202ee6f`, `b31432a`, `a9c65b5`) |
| Linux | **FROZEN** — untouched this session |

Pass 2 (aggregate recomputation, fmt, clippy, regression of the touched stages) was
performed on the state above. No Linux work was started.
