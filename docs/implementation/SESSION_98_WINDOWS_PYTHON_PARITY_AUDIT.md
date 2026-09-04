# SESSION 98 — Windows × Official Python Capability Parity Audit

## Summary

- **Starting commit:** `a716df4` (clean tree) — verified with `git rev-parse HEAD`, `git status`
- **Final commit:** see git log (audit/planning commit; no code changed)
- **MINK version:** 1.0.1 (`mink --version`)
- **Test suite:** 2547 tests / 56 targets. Full-suite runs: 2545 pass + 2 timing-flaky
  loopback network tests (`windows_hardening`) that pass in isolation — re-verified
  individually; **not a Session 97 regression** (isolated runs green on all three
  observed flaky tests across two full runs).
- **Session type:** audit + architecture planning only (no language/runtime/emitter code
  modified; Linux untouched/FROZEN).
- **Deliverables:** this record + `docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md`
  (master matrix) + `docs/roadmap/WINDOWS_PYTHON_PARITY_IMPLEMENTATION_PLAN.md`.

## 1. Phase 1 — Baseline verification (all green)

| Check | Result |
|---|---|
| Starting commit | `a716df46623eb6805338b24f3677be2497209b39` = `a716df4` ✓ |
| Working tree | clean ✓ |
| Version | `mink 1.0.1` from the release binary ✓ |
| Windows target | host is Windows x64; `x86_64-windows-pe` is the native target (`[code] src/backend/target.rs`) ✓ |
| Release build | `cargo build --release` clean ✓ |
| Test count | 2547 tests across 56 targets (grep-tallied `#[test]` per file sums to 2547) ✓ |
| Representative smoke suite | smoke 13/13 · release 66/66 · cli 74/74 ✓ |
| Session 97 regression | none — the only full-run failures are two timing-sensitive network tests under 56-binary parallel load; each passes in isolation ✓ |
| Linux untouched | no Linux file modified; git status clean ✓ |

**Flaky-test finding (P3, test infra):** across two full parallel runs, three distinct
`windows_hardening` loopback tests failed intermittently (`tcp_mink_client_checksum_multirecv`,
`http_windows_split_server_multi_recv_and_errors`, `http_windows_large_body_exact_length`);
all pass when run alone or when the binary runs standalone. Recorded for a future
test-harness hardening session — do not treat as a product regression.

**Native probes (execution evidence gathered this session):**

1. `&&` / `||` **short-circuit**: a side-effecting right operand was not evaluated
   (output `0`, `1`, no `99`) → the Session 82 audit's "no short-circuit" claim is stale.
2. Integer division by zero is **not guarded**: unconditional `10 / x` with `x = 0`
   terminates via the CPU #DE fault (observed exit 148 = Windows status `0xC0000094`
   truncated to a byte) with no structured runtime error → runtime-hardening item
   (matrix L20, P2/S).

## 2. Scope definition (Phase 3 of the brief)

In scope: official Python language capabilities, CPython-visible standard behavior,
Python standard-library categories, official runtime and CLI/runtime workflows,
import/module/package behavior, packaging/build concepts, venv concepts, REPL,
filesystem/process/networking, threading/concurrency/async, serialization, compression,
text/encoding, date/time, math/statistics, OS/platform integration, Windows-specific
official Python capabilities, diagnostics/introspection, and distribution/install
workflows — mapped to MINK-native equivalents.

Out of scope: PyPI/third-party ecosystem (NumPy, pandas, requests, Flask, Django, …),
CPython internals except where user-visible, syntax imitation, Python behaviors that
conflict with MINK design (static/native/ownership/safety), and the Linux implementation.

The working question was not "does MINK look like Python?" but:

> Can a Windows developer accomplish the same important official tasks Python supports,
> using MINK's own architecture and design?

## 3. Phase 2 — Current MINK capability inventory (evidence-based)

Grades: **A** = execution verified on Windows · **B** = implemented, incompletely
verified · **C** = partial · **D** = documented only · **E** = planned · **F** = not
implemented. Graded from source, tests, and recorded execution — not from docs alone.

### 3.1 Compiler / pipeline

| Capability | Grade | Evidence |
|---|---|---|
| Lexer → Parser → AST | A | `[code] src/{lexer,parser,ast}`; `[test]` lexer (50), parser (98), parser_hardening (63), adversarial (94) |
| Semantic analysis + diagnostics (E-S*) | A | `[code] src/semantics`; `[test] semantics (75)`; `mink explain` verified |
| Type system + inference + generics + monomorphization | A | `[code] src/typecheck`, `src/monomorphize`; `[test] typecheck (163), generics (29), let_annotations (82)` |
| Ownership / borrow checking | A | `[code] src/ownership`; `[test] ownership (43), references (58)` |
| HIR → MIR → optimizer → validator | A | `[code] src/{hir,mir}`; `[test] hir (25), mir (34), optimization (38)` |
| Native codegen x86-64 + PE emit (Windows) | A | `[code] src/backend/emit/*`; every native run in the 2547-test suite |
| Targets: aarch64-linux-elf | F (recognized only) | `[code] src/backend/target.rs` |
| Targets: x86_64-linux-elf | A on Linux, **FROZEN by policy** | `[code] src/backend/emit/{elf.rs,linux_runtime.rs}`; Session 92/93 records |

### 3.2 Language

| Capability | Grade | Evidence |
|---|---|---|
| Scalars Int/Float/Bool/Char/Null/Unit/Never | A | `[code] src/typecheck/ty.rs`; `[test] scalar_types (27), bool_packing (21)` |
| Str byte strings (heap/immutable), byte access | A | `[test] strings (63), strings_lib (73), windows_hardening` |
| Structs, enums (unit/data/discriminants), arrays, tuples, Vec | A | `[test] aggregate (59), enums (25), sum_types (44), discriminants (32), tuples (34), collections (24), runtime (24)` |
| Generics + monomorphization | A | `[test] generics (29)` |
| Pattern matching (rich, exhaustive) | A | `[test] pattern_matching (44), richer_patterns (90), match_expressions (41), struct/tuple destructure` |
| Control flow (if/while/loop/for-ranges/break-with-value) | A | `[test] loop_expressions (39)` |
| Closures | B | `[test] closures (26)` — native runs exist; capture modes limited (move/copy only) |
| Option/Result + `?` | A | `[test] option_result (45), try_operator (12)` |
| `&&`/`\|\|` short-circuit | A | native probe this session |
| Modules `mod`/`use`/`pub`, multi-file | A | `[code] src/driver.rs`, `src/module`; `[test] modules_check (6)`, module fixture dirs |
| Methods/impl blocks, inheritance, traits/interfaces | F / E | not in AST (`[code] src/ast/mod.rs`); interface design in spec `[doc] TYPE_SYSTEM.md` §13 |
| Map/Set types | F | no type in `[code] src/typecheck/ty.rs` |
| Unicode text semantics (code points) | F (byte model is C) | byte-oriented by design; UTF-8 ops absent |
| Runtime exceptions / catch | F (intentional; Option/Result instead) | design decision (exclusion register X05) |

### 3.3 Runtime (Windows)

| Capability | Grade | Evidence |
|---|---|---|
| Arena allocator + liveness + leak check + E-R codes | A | `[code] src/runtime/*`, `src/backend/emit/runtime.rs`; leak checker on in all tests |
| String/vec intrinsics, exact float printing | A | `[test] scalar/strings/float suites` |
| stdout print, exit codes | A | `[test] cli, release, smoke` |
| Filesystem read/write/copy/move/rm/mkdir/cwd/attr-size | A | `[test] filesystem_lib (34)`, windows_hardening real ops in a spaced dir |
| Process run + stdout/stderr capture + exit codes + PID | A | `[exec]` Session 97: 1 MB drain; `[test] process_lib (39)`; cap 4088 B/stream documented |
| Environment (`rt_env_*`) | **C — Windows stubs** | `[code] src/backend/emit/runtime.rs` emit_env_get/set/has/remove (empty/-1/false); real env walk only on frozen Linux emitter; Session 82 audit claim "implemented" is **stale** |
| Time (epoch/millis/ticks/freq) | A | `[test] time_lib (16)` |
| TCP/UDP/DNS/hostname | A | `[test] network_lib (26)`, windows_hardening checksums |
| HTTP/1.1 client GET/POST | A (no TLS) | `[test] http_lib (35)`; `[exec]` Session 96 byte-exact POST echo |
| Crypto (BCrypt random), HMAC/HKDF/SHA-256 | A | `[test] crypto_lib (18), hashing_lib (24)`; `[exec]` Session 92 vectors |
| Random (xorshift), JSON, base64/hex/url, math (series float) | A | `[test] random_lib (15), json (61), encoding_lib (57), math_lib (106)` |
| argv, stdin, sleep, stderr-write intrinsic, error source lines | F | no intrinsics (`[code] src/runtime/intrinsics.rs` complete list) |
| Threads / async / locks | F | no CreateThread anywhere in the emitters |

### 3.4 Tooling & distribution

| Capability | Grade | Evidence |
|---|---|---|
| CLI build/run/check/explain/version/help + `--json` + `--target` | A | `[code] src/cli.rs`; `[test] cli (74)` |
| npm distribution of the compiler (Windows x64 only) | A | `[exec]` Session 97 clean installs ×2 + standalone exe; `[code] npm/mink/package.json` |
| Stdlib sources shipped with npm | F | package ships only `bin/mink.exe` |
| REPL / interpreter | F | not present |
| MINK test framework / `mink test` | F | Rust-hosted harness only |
| Error-code documentation (`mink explain`) | A | `[test] cli` |
| Examples committed | A | `examples/system_report` (builds + runs, standalone) |

### 3.5 Documentation state (truth check)

- README feature/stdlib table matches source (except "transform" in the collections
  description is only `vec_reverse` — minor wording, untouched).
- `docs/audits/OFFICIAL_PYTHON_CAPABILITY_PARITY_AUDIT.md` (Session 82) contains stale
  claims now corrected by this audit: env "implemented" (actually Windows stubs);
  Linux target "missing" (actually implemented + frozen); "no short-circuit" (probed
  false); "199 test files" (56 targets / 2547 tests); crypto "not verified" (now
  execution-verified). This Session-98 matrix supersedes it.
- Ecosystem design docs (`docs/ecosystem/*`) remain design-draft status markers and were
  not edited (Linux/planning-adjacent docs frozen where applicable).

## 4. Phases 3–8 — Official Python capability maps

The full maps (language L01–L73, runtime R01–R29, stdlib S01–S78, tooling T01–T16,
Windows-specific W01–W22, packaging P01–P14, and the anti-clone register X01–X21) live in
`docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md` with per-row status, gap,
priority, difficulty, parity-blocking flag, wave, notes, and evidence anchors. Numbers
below are the consolidated totals from that matrix (row-scanned).

| Domain | Rows | VERIFIED* | PARTIAL | MISSING | INTENT./N-A/PLANNED | P1 blockers |
|---|---|---|---|---|---|---|
| Language (L) | 73 | 25 | 10 | 23 | 15 | 7 |
| Runtime (R) | 29 | 6 | 4 | 17 | 2 | 8 |
| Standard library (S) | 78 | 18 | 15 | 42 | 3 | 17 |
| Tooling (T) | 16 | 6 | 1 | 8 | 1 | 3 |
| Windows-specific (W) | 22 | 2 | 12 | 7 | 1 | 3 |
| Packaging (P) | 14 | 1 | 2 | 8 | 3 | 6 |
| **Total** | **232** | **58** | **44** | **105** | **25** | **44** |

*VERIFIED* = VERIFIED rows plus the two qualified VERIFIED rows (R17 VERIFIED+INTENT.
DIFF., T11 VERIFIED+partial). INTENT./N-A/PLANNED = INTENT. DIFF. 13 + INTENT. DIFF. +
PARTIAL 1 + N/A 6 + N/A (INTENT.) 2 + PLANNED 3. Statuses are mutually exclusive, so the
columns sum to the row total.

### Windows-specific classification summary (Phase 7)

- **REQUIRED for MINK parity:** paths/drive letters (W01), long paths (W03), Unicode
  console+paths (W05), env (W06), process (W07), console streams (W08), metadata (W09),
  sockets (W10), PATH discovery (W12), temp dirs (W13), home dirs (W14), OS error text
  (W19), wide APIs (W20), console encoding (W22).
- **USEFUL but optional:** UNC (W02), control events (W11), registry (W15), DLL/FFI
  (W16), terminal colors (W17).
- **Python-specific / not required:** case-insensitivity warnings parity (W04 — inherit
  OS behavior), CRLF text-mode translation (W18 — MINK is explicitly byte-oriented).

## 5. Phases 9–15 — Classification, waves, proofs, estimates (consolidated)

### 5.1 Gap counts and severity breakdown (Phase 9)

- Audited rows: **232** · rows requiring work: **175** · no-gap rows: 57 ·
  design-different/NA rows: 25 (INTENT. DIFF. 13 + INTENT. DIFF. + PARTIAL 1 + N/A 6 +
  N/A (INTENT.) 2 + PLANNED 3; a further 21 entries sit in the X-register X01–X21).
- Severity: **P0 = 0** · **P1 = 44** (all parity-blocking; there are no P1 non-blockers
  and no P2 blockers) · **P2 = 79** · **P3 = 52** · no-gap = 57.
- Difficulty across the 175 work rows: **S-M = 9 · S = 38 · M = 82 · L = 31 · XL = 15**.

### 5.2 Parity-blocking P1 gaps (44) by domain

- Language (7): L05 Unicode-text layer · L08 full typed Vec · L10 Map · L16 float→Str +
  formatting · L34 catchable structured runtime errors · L67 importable stdlib · L68
  package directories.
- Runtime (8): R01 REPL · R06 error location on runtime faults · R10 stdin · R12 argv ·
  R13 env (Windows) · R19 threads · R20 async · R23 sleep.
- Stdlib (17): S01 split/join text ops · S02 regex · S06 numeric text conversions · S28
  directory listing/traversal · S36 CSV · S41 SQLite · S42 zlib/gzip · S44 zip · S50 env
  · S62 TLS · S69 strftime-style format/parse · S70 sleep · S71 threads · S73 async ·
  S74 logging · S75 MINK unit-test framework · S78 test assertions.
- Tooling (3): T04 REPL · T06 import search paths · T07 `mink test` runner.
- Windows (3): W06 env · W08 console streams (stdin/stderr) · W14 home/known folders.
- Packaging (6): P02 stdlib bundled with npm · P03 import packages · P04 installed
  packages · P05 dependency install · P06 resolver · P09 environment isolation.
  (L67/T06 and P02 are the same physical work seen from language, tooling, and packaging
  angles.)

### 5.3 Intentionally different / not applicable (Phases 10, register X01–X21)

Highlights: arbitrary-precision ints (fixed i64) · GC (ownership + validated arena +
leak check) · classes/inheritance/MRO (struct/enum + generics + match) · duck typing
(static types) · runtime exceptions (Option/Result + `?` + structured E-R codes) ·
Unicode-by-default str (byte strings + planned UTF-8 layer) · runtime introspection and
monkey-patching (compile-time checks) · pickle (deterministic formats) · decorators
(compile-time attributes later) · generators/comprehensions (loops + planned iteration) ·
walrus, global/nonlocal keywords, `del`, `*args`/`**kwargs`, f-string expression eval,
`python -m`, wheel/sdist/pip formats, venv mechanics, GIL-shaped threading.

### 5.4 Quick wins vs major subsystems (Phase 13)

- **Quick wins (≈ 1 session each, 15 items):** Windows env wiring (R13/W06/W14), Sleep
  (R23/S70), argv (R12), stdin (R10), stderr-write intrinsic (R09), float→Str +
  `str_format` (S06/L16), runtime error location metadata (R06), div-by-zero guard
  (L20), OS error text (R26/W19), temp dirs (S30/W13), stdlib-in-npm + include path
  (P02/L67/T06), platform constants (R15/W21), str_split/join + CSV seed (S01/S36),
  process stdin/cap streaming (R22/S51), stdlib error contracts.
- **Major subsystems (15):** Map/Set · full typed Vec<T> · regex engine · concurrency
  runtime · async/event loop · TLS · SQLite · DEFLATE/zip/tar · package manager · REPL
  · MINK test framework · debugger · Unicode text layer · FFI/dynamic libraries ·
  compile-time metadata/reflection.

### 5.5 Implementation waves (Phase 12)

A Windows platform/runtime quick wins → B core language/data (maps, Vec, iteration,
format, split, regex) → C filesystem/process/time completion → D networking/internet
(TLS, HTTP completion, non-blocking I/O) + SQLite + compression → E concurrency/async →
F packaging/distribution → G developer tooling (REPL/test/debug) → H Windows-specific
completion (long paths, Unicode console, wide APIs) → I proof applications → J final
parity audit. Full per-wave objectives, dependencies, complexity, proofs, and gates:
`docs/roadmap/WINDOWS_PYTHON_PARITY_IMPLEMENTATION_PLAN.md` §1.

### 5.6 Proof strategy (Phase 14)

Real committed applications per category — file utility (walk/copy/glob/temp), JSON+CSV
processor, HTTPS/TCP application, SQLite-backed app, archive tool, concurrent workload,
multi-package app, REPL transcript, non-ASCII CLI utility, automation script — unit tests
are evidence but never the final proof. Plan §5.

### 5.7 Effort estimate (Phase 15)

Session ranges by wave (min–realistic–worst): A 3-4-6 · B 7-9-12 · C 4-5-6 · D 7-9-11
· E 6-8-10 · F 5-7-9 · G 6-8-10 · H 3-4-5 (calendar-parallel) · I 4-5-7 · J 1-2-2.
**Total ≈ 46 minimum · 58–64 realistic · ≈ 78 worst.** Calendar projection on the order
of 12–20 months at the project's historical cadence; three parallel tracks possible
(language/data B; platform A→C→D→E; packaging F→G).

Largest risks (ranked): 1 concurrency safety on the arena/ownership model · 2
self-contained regex + TLS size · 3 ownership extensions rippling through the checker ·
4 package-manager scope creep · 5 ANSI→wide API migration · 6 Linux-freeze discipline ·
7 flaky loopback test infra under parallel load.

## 6. Phase 16 — No-implementation rule (observed)

No language, compiler, emitter, runtime, or stdlib behavior was modified. The only
changes are this audit's three documents plus the correction of stale capability claims
inside the new documents. Linux code, tests, emitters, runtime, and documentation were
not touched. Temporary probe files were created under the ignored `tmp/` directory and
removed; working tree is clean.

## 7. Phase 17 — Quality / truth gate

- `cargo fmt --check` / `cargo clippy`: not required — no Rust code changed.
- Tests: full suite re-run twice this session (2545 + 2 flaky in-run, isolated reruns
  green) plus smoke/release/cli suites — no regression.
- Claims verified against source: env stubs, `&&`/`\|\|` short-circuit (native probe),
  div-by-zero fault status, entry/argv rules, module resolution rules, target table,
  intrinsic catalog, stdlib API surface, npm package contents, per-file test counts.
- Items not fully proven are marked UNKNOWN / NEEDS EXECUTION PROOF in the matrix
  (e.g., drive-letter path semantics W01, module-level mutable state L51, Vec element
  boundary cases L08).
- `git status` clean after the commit.

## 8. Files changed (Session 98)

1. `docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md` (new — master matrix)
2. `docs/roadmap/WINDOWS_PYTHON_PARITY_IMPLEMENTATION_PLAN.md` (new — waves/plan)
3. `docs/implementation/SESSION_98_WINDOWS_PYTHON_PARITY_AUDIT.md` (new — this record)

No temporary files, no generated junk, no unrelated changes.

## 9. Final report

1. **Starting commit:** `a716df4` (clean tree).
2. **Final commit:** the Session 98 audit/planning commit (three documents).
3. **Windows baseline status:** COMPLETE / STABLE — version 1.0.1, release build clean,
   2547 tests, 0 P0/0 P1 on the base (Session 97 gate re-verified).
4. **Current MINK capability count/categories:** 16 stdlib modules · ~110 `rt_`-level
   intrinsic/service ops across memory/strings/vec/fs/process/time/net/crypto/random/
   output · 56 test targets (2547 tests) · CLI with 6 commands · 1 committed example.
   Inventory with A–F grades: §3 above.
5. **Official Python capability categories audited:** language core data/expressions/
   control flow/functions/scopes/object model/iteration/error handling/modules/
   introspection; stdlib text/binary/data-structures/math/functional/fs/serialization/
   db/compression/crypto/os/process/networking/time/concurrency/dev; runtime UX;
   tooling; Windows-specific; packaging — 232 rows total.
6. **Fully covered categories:** native compilation & standalone exe · ownership/
   memory with leak check · scalars/arithmetic/logic · structs/enums/generics/match ·
   file modules · JSON · base64/hex/url · SHA-256/HMAC/secure-random · math/random ·
   time epoch · filesystem file+path ops · process run+capture · TCP/UDP/DNS · HTTP/1.1
   client core · CLI/explain/JSON diagnostics · npm compiler distribution · Windows
   process/socket/crypto/console output (§10.5 of the matrix).
7. **Partial categories:** text/string utilities (split/format/float-text missing),
   bytes-level unicode, Vec typing limits, closures, iteration, process extras,
   filesystem traversal/metadata, HTTP conveniences, time formatting/timezones, console
   stdin/stderr, Windows paths/env/long-paths/Unicode.
8. **Missing categories:** REPL · dict/set · regex · CSV · SQLite · TLS/HTTPS ·
   compression/archives · threads · async · logging · MINK test framework · argv/stdin/
   sleep/stderr intrinsics · directory listing · package manager · importable stdlib.
9. **P0 gap count:** 0.
10. **P1 parity-blocker count:** 44 (all blockers are P1).
11. **P2 count:** 79.
12. **P3 count:** 52.
13. **Major subsystem gaps:** Map/Set · full typed Vec · regex engine · concurrency
    runtime · async/event loop · TLS · SQLite · DEFLATE/zip/tar · package manager ·
    REPL · debugger · Unicode text layer · FFI/dynamic libraries · compile-time
    reflection · MINK test framework (framework itself is M; the list of 15 majors is
    plan §3).
14. **Windows-specific gaps:** env wiring · argv · stdin · stderr write · home/temp dirs
    · error text · long paths · Unicode console/output · wide APIs · control events ·
    file metadata · directory traversal · PATH semantics · registry (optional) ·
    drive-letter path proof.
15. **Intentionally different / not applicable:** 13 INTENT. DIFF. rows + 6 N/A + 2
    N/A(INTENT.) + 1 mixed + 21-register X01–X21 (anti-clone register in matrix §9).
16. **Quick wins:** 15 (plan §2) — env, sleep, argv, stdin, stderr write, float→Str,
    error location, div-by-zero guard, error text, temp dirs, stdlib-in-npm, platform
    constants, split/CSV seed, process streaming, stdlib error contracts.
17. **Major implementation work:** 15 subsystems + 175 total work rows
    (S-M 9 / S 38 / M 82 / L 31 / XL 15); waves A–J in plan §1.
18. **Proposed implementation waves:** A Windows runtime quick wins → B language/data →
    C fs/process/time → D net/internet/sqlite/compression → E concurrency/async → F
    packaging → G tooling → H Windows completion → I proofs → J final audit (plan §1).
19. **Estimated session range:** ≈ 46 minimum · 58–64 realistic · ≈ 78 worst;
    calendar on the order of 12–20 months at historical cadence.
20. **Largest risks:** concurrency safety on the validated memory model · regex/TLS
    self-contained size · ownership extensions ripple · package-manager scope creep ·
    wide-API migration · Linux-freeze discipline · parallel-load test flakiness.
21. **Documents created/updated:** the three files in §8 (master matrix, implementation
    plan, this record). No source/docs outside the audit set were modified.
22. **Tests/quality gates:** full suite green modulo the documented flaky pair; smoke/
    release/cli green; no code changed so fmt/clippy not applicable; claims traced to
    source/tests/execution; unknowns marked NEEDS EXECUTION PROOF.
23. **Git status:** clean after commit.
24. **FINAL STATUS:**
    - WINDOWS BASE = **COMPLETE / STABLE**
    - WINDOWS PYTHON OFFICIAL CAPABILITY PARITY = **PARTIAL** (44 parity-blocking P1
      gaps mapped; parity may not be declared complete while any P1 row remains)
    - LINUX = **FROZEN** (untouched; no unfreeze until the parity program and final
      gate are complete)
25. **Exact recommended objective for Session 99:** execute Wave A tranche 1 — wire the
    Windows environment intrinsics, add argv + stdin + Sleep + a stderr-write intrinsic
    and runtime error-location metadata, add float→Str and the first `str_format`, and
    bundle `stdlib/` into the npm package with a module include path; then flip the
    affected matrix rows (R06/R09/R10/R12/R13/R23/S06/S50/S70/W06/W08/W14/P02/L67/T06)
    to VERIFIED with native tests and one automation proof app.

## 10. Final recommendation

Proceed with Wave A immediately (Session 99+) while Wave B language/data work starts in
parallel. Keep the exclusion register enforced, keep Linux frozen, and treat the
44-row P1 list as the parity gate's definition of done. Do not declare parity COMPLETE
until a Wave J re-audit shows zero P1 rows with every claim traced to source, tests, and
a running proof application.

*End of Session 98 record — commit a716df4.*
