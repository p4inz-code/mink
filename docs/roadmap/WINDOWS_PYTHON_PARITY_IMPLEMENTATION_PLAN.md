# MINK — Windows Official-Python Parity: Implementation Plan

**Session:** 98 (with a Session 107 reconciliation note) · **Starting commit:** `a716df4` · **MINK version:** 1.0.1
**Input:** `docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md` (the master matrix,
232 audited capability rows). Session 98 planning baseline: 175 rows requiring work, 44
parity-blocking P1 gaps. **Current, evidence-derived figures (Session 107): 171 rows
requiring work, 26 parity-blocking P1 gaps.**
**Date:** September 4, 2026

> **Session 107 note.** This plan is the Session 98 sequencing document. The wave map in
> §1, the per-wave blocker counts in §6, and the "44 today" reference in §0 are the
> Session 98 baseline, kept for traceability. The matrix §10 (10.1-10.7) is authoritative
> for current counts: 26 blockers, distributed A 1 · B 4 · C 2 · D 4 · E 4 · F 6 · G 5
> (§10.4). The rows delivered across Sessions 99-107 no longer appear in that list. The
> Session 107 additions were the UTF-8 text layer (L05) and the reconciliation of stale
> `P1`/`Blocks = Y` flags on already-verified rows (R06, W14, P02, T06).

This plan converts the parity gap map into a dependency-aware implementation sequence.
Wave letters below are the same as the matrix `Wave` column. Every wave ends with a
completion gate; nothing in a later wave may silently redefine an earlier wave's gate.

## 0. Parity definition (from the audit)

> Can a Windows developer accomplish the same important official tasks Python supports,
> using MINK's own architecture and design?

- Parity = **capability coverage**, not syntax identity. The anti-clone register (matrix
  §9, X01–X21) is part of this plan's contract.
- Parity is declared complete only when the matrix shows **zero P1 gaps** (44 at the
  Session 98 baseline, **26** as of Session 107) and
  every status is VERIFIED/IMPLEMENTED/INTENT. DIFF./N/A with an execution-verified or
  test-backed trace, per the proof strategy in §6.
- Community/pip ecosystem parity is out of scope forever for this gate.
- Linux stays FROZEN through this entire program.

## 1. Wave map

| Wave | Objective | Key capability gaps (matrix IDs) | Dependency | Complexity | Proof requirement | Completion gate |
|---|---|---|---|---|---|---|
| A | Windows runtime/platform completion: make the platform real for programs (env, args, stdin, sleep, stderr, error location, float text, stdlib importable) | R06, R10, R12, R13, R23, S06, S50, S70, S74, W06, W08, W14 (+ P2/P3: R09, R15, R26, W13, W19, W21, L20, T15) | none (all isolated, S-M) | S-M | each fix proven by native Windows tests; a small CLI utility using argv+env+stdin+files | 0 P1 left in A; suite green; matrix rows flipped to VERIFIED |
| B | Core language + text/data: maps, full dynamic collections, iteration, formatting, split, regex | L05, L08, L10, L16, L34, S01, S02, S36 (+ L39/L41/L46/L47/L60/L61, S01-S23 partials) | A (float→Str formatting reuses S06 tooling; error-location affects runtime not language) | M-L (regex = XL) | dict/Vec/iteration covered by compiler+ownership tests; CSV file app; regex test corpus | language/data rows VERIFIED; ownership-sound container suite green |
| C | Filesystem/process/time completion | S28, S69 (+ S08, S29-S34, S51, S55, W01, W02, W12, S67-S68) | A (env for home/temp; W12 PATH), B partially (S69 needs str_format patterns) | S-M | file-tree utility (walk/copy/glob/temp); log-with-timestamps app | fs/process/time rows VERIFIED |
| D | Networking/internet/data: non-blocking I/O, TLS, HTTP completion, compression, SQLite | S41, S42, S44, S62 (+ S59-S61, S63, S45, S60) | C (file streams feed zip/tar), E not required (select precedes async) | L-XL | HTTPS client against a real endpoint or local TLS server; sqlite-backed app; zip/gzip round-trip tool | net/TLS/sqlite/compression rows VERIFIED |
| E | Concurrency + async runtime | R19, R20, S71, S73 (+ R21, R24, S12, S63) | D (select/non-blocking sockets), B (iteration protocol helps streams) | XL | real concurrent workload (parallel downloader or worker pool) proving no data races and sound memory | thread/async rows VERIFIED with race-focused native tests |
| F | Packaging/distribution: package manager, manifest, resolver, lockfile, install paths, isolation; stdlib bundle | L67, L68, P02-P06, P09 (+ P07, P08, P11-P14, T06) | A (include-path mechanism), B (module model), C minor | L-XL | clean multi-package app: `mink new` → deps → build → run; lockfile reproducible on second machine | packaging rows VERIFIED; npm still ships the compiler (category B unchanged) |
| G | Developer tooling: REPL, test framework, debugger-class diagnostics | R01, S75, S78, T04, T07 (+ T08, T09, T12, L36, L72) | A (error location R06 feeds REPL/debugger), B (introspection needs compile-time metadata) | M-XL | interactive REPL session transcript; `mink test` runs a real MINK test suite; stepping session via a debugger front end | tooling rows VERIFIED |
| H | Windows-specific completion: long paths, Unicode console, wide APIs, control events, metadata | (W03, W05, W11, W15, W20, W22 + W09) | A (paths), B (Unicode text layer L05) | M | non-ASCII path + console round-trip on a real Windows session; Ctrl+C graceful app | Windows rows VERIFIED/classified OPT done |
| I | Real-world proof applications (parity evidence, not unit tests) | exercises every wave | A-H in dependency order | M | committed example apps per §6 matrix | each proof app runs in CI/native and is committed |
| J | Final parity audit and gate | all | I | S | re-audit like Session 98; every row re-verified against source + tests + proofs | zero P1; statuses complete; parity declaration |

### Sequencing rules

1. **A → C → D is a soft chain** (A unblocks C's env-dependent items and D's quality; C's
   file streams feed D's archives; D's select feeds E's async).
2. **B runs in parallel with A/C** for language work not touching the runtime surface;
   B's Vec/dict work is independent of A's OS wiring. B is the longest single wave and
   should start early.
3. **E depends on D** (non-blocking I/O) more than it depends on C.
4. **F depends on A** (include path mechanism) and B (mature module model); it is
   otherwise parallel to D/E/G.
5. **G depends on A** (R06 error metadata) and B (introspection hooks for the REPL).
6. **H can be interleaved with B** (Unicode text layer) and A; it is calendar-parallel.
7. **I and J are gates**, not standalone tracks: each wave must carry its own proof app
   (§6), and wave I consolidates + adds cross-cutting apps.

## 2. Quick wins (≈ one focused session each, low architectural risk)

| Item | Matrix IDs | Work | Risk |
|---|---|---|---|
| Wire Windows environment intrinsics (IAT entries already emitted) | R13, S50, W06, W14 | implement emit_env_get/set/has/remove against Get/SetEnvironmentVariableA; home dirs via USERPROFILE | S |
| `Sleep` + monotonic wait intrinsic | R23, S70 | kernel32 Sleep import + wrapper | S |
| argv support | R12, W08 | GetCommandLineW parse → argv intrinsic; allow `main(argv)` or an argv accessor | M (entry-stub change) |
| stdin read intrinsic | R10 | ReadFile on stdin handle | S-M |
| Expose stderr write intrinsic | R09 | wrap existing WriteStderr thunk | S |
| Float→Str + `str_format` (int/float/bool/str) | S06, L16 | rt_str_from_float (reuse exact dtoa machinery) + format helper | M |
| Runtime error location metadata (function + source line table in image) | R06 | embed per-image line table; Fail prints location | M |
| Div-by-zero / INT_MIN guard → E-R error | L20 | idiv guard in emitter | S |
| FormatMessage error text for fs/process/net | R26, W19 | expose last-error text | S |
| Temp dirs (`GetTempPath`) + known folders | S30, W13 | intrinsics + stdlib wrapper | S |
| stdlib sources bundled into the npm package + module include path | P02, L67, T06 | npm ships `stdlib/*.mink`; compiler searches `<exe>/../stdlib` then project dir | S-M |
| Platform/arch + version constants intrinsic | R15, W21, R27 | small constant return | S |
| `str_split` / `str_join` (delimiter) + first CSV functions | S01, S36 (seed) | MINK-source string split + join; CSV read/write | M |
| Process: stdin pipe + no-cap streaming | R22, S51 | extend emit_process_run | M |
| Windows error exit on failed OS ops instead of bare -1 (docs+contract) | R26 family | stdlib error codes | S |

## 3. Major subsystems (do not hide these behind one bullet)

| Subsystem | Matrix IDs | Scope sketch | Est. sessions | Notes |
|---|---|---|---|---|
| Hash map + hash set with owned keys/values | L10, L11, L12 | language-level `Map<K,V>`/`Set<T>` + hashing of Int/Str; ownership of inserted strings; iteration order deterministic | 3-5 | gate for Wave B |
| Full typed dynamic collections (`Vec<T>` for Str/Float/structs + nested) | L08, S14, S16 | ownership-aware element storage; runtime rework of vec buffer to typed elements | 2-4 | pairs with maps |
| Regex engine | S02 | self-contained engine (no deps allowed): compile+match with captures; integrate `re`-like stdlib | 4-6 | largest single stdlib item |
| Concurrency runtime (threads, locks, atomics, condvars) | R19, R21, S71 | Windows thread primitives; TLS-safe allocator/liveness; memory-model discipline | 4-6 | riskiest safety item |
| Async runtime + event loop + futures | R20, S73, S63 | select/poll on sockets; timers; async I/O model for the language | 3-5 | after threads or alongside |
| TLS/SSL | S62 | self-contained TLS (client) or schannel binding via existing LoadLibrary mechanism | 3-5 | no third-party crypto libs allowed today |
| SQLite | S41 | embedded SQL engine (self-written subset) or vendored-compatible approach per architecture rules | 4-6 | prompt lists it as a major subsystem; proof app required |
| DEFLATE/zlib/gzip + zip + tar | S42, S43, S44, S45 | inflate first (read), deflate later; zip container over stored/deflate | 3-5 | |
| Package manager (install/resolve/lock/manifest/isolation) | P03-P09, L68 | `mink.toml`, resolver, lockfile, project-local deps, publish foundations | 4-7 | security architecture doc already designs it |
| REPL | R01, T04 | compile-eval loop on the native backend; history; diagnostics reuse | 2-4 | |
| MINK test framework + runner | S75, S78, T07 | assert macros; `mink test`; test discovery by name convention | 1-2 | quick relative to the rest |
| Debugger | T08 | breakpoints/step/inspect on generated code (needs R06 tables) | 3-5 | P2 but planned in G |
| Unicode text layer | L05, S03-S05, W05, W22 | UTF-8 validate/decode/iterate; non-ASCII case; console UTF-16 path | 2-4 | split with Wave H |
| FFI/dynamic libraries | R25, W16 | expose LoadLibrary/GetProcAddress + C ABI calling convention for user programs | 2-3 | P2; C_ABI_SPEC exists |
| Compile-time metadata/reflection for tooling | L72, L73 | function/type metadata for REPL/debugger/docs | 2-3 | P3/PLANNED |

## 4. What is intentionally NOT built (anti-scope, per matrix §9)

No GC, no runtime exceptions as control flow, no classes/inheritance/MRO, no duck typing,
no monkey-patching, no pickle, no `python -m` semantics, no `*args/**kwargs` universality,
no GIL-shaped threading. If a parity gap tempts one of these, re-read the exclusion
register first. The equivalence must be the MINK-native one, or the row stays MISSING and
is explicitly justified.

## 5. Proof strategy (Phase 14) — parity requires real applications

| Category | Proof app (committed under examples/) | Wave |
|---|---|---|
| Filesystem | file utility: walk + copy/move + glob + temp + metadata, runnable from CLI args | C |
| Serialization/data | JSON+CSV processor (parse, transform, write) using maps/Vec/strings | B |
| Networking | HTTP/TCP application: HTTPS client fetch + local TCP service | D |
| Database | SQLite-backed application (storage + queries) | D |
| Compression | archive tool: zip/gzip create+extract | D |
| Concurrency | real concurrent workload (parallel HTTP fetch or compute pool) with leak-checker on | E |
| Packaging | clean-install multi-package app (manifest + deps + lock + build + run) | F |
| REPL | interactive session transcript showing compile-eval-run | G |
| Modules | multi-package, multi-module application | F |
| Text/Unicode | non-ASCII CLI utility (args, paths, console output, formatting) | B/H |
| Windows integration | Ctrl+C graceful app; long-path + space-path + Unicode-path file operations | H |
| Time/process | automation script (env, argv, sleep, subprocess, timestamps, logs) | A/C |

Unit tests are evidence, never the final proof for a parity category. Every wave must
land at least one proof app or explain why its categories are already proven by an
earlier wave.

## 6. Estimates (Phase 15)

| Wave | Blockers (primary) | All work rows | Session range (min-realis-worst) |
|---|---|---|---|
| A | 11 (+5 shared: L16/S06 split, L67/P02/T06 mechanism) | ~22 | 3-4-6 |
| B | 6 (+4 shared: L16, S06, L05 w/ H) | ~20 | 7-9-12 |
| C | 2 | ~12 | 4-5-6 |
| D | 4 | ~11 | 7-9-11 |
| E | 4 | ~8 | 6-8-10 |
| F | 6 (+3 shared from A) | ~12 | 5-7-9 |
| G | 5 | ~10 | 6-8-10 |
| H | 0 | ~10 | 3-4-5 (calendar-parallel) |
| I | 0 | ~8 apps | 4-5-7 |
| J | 0 | re-audit | 1-2-2 |
| **Total** | **44 at the Session 98 baseline · 26 as of Session 107** (matrix §10.4; all unique gaps counted once across A–G) | | **≈ 46 min · 58-64 realistic · ≈ 78 worst** (Session 98 estimate) |

Interpretation:

- **Minimum (≈ 46 sessions at the Session 98 baseline):** the then-44 blockers only,
  tightest sequencing, no P2/P3 items, H fully inside other waves, proofs minimal per
  category.
- **Realistic (≈ 58-64 sessions):** blockers + important P2 items (process stdin, HTTP
  completion, sockets select, profiler/debugger first pass, long paths, wide APIs) +
  interleaved wave H + one proof app per category.
- **Worst case (≈ 78 sessions):** P2/P3 breadth (registry, terminal UX, docgen,
  formatter, more codecs, decimal/fractions, locale), rework from ownership/runtime
  surprises, plus the two flaky-network-test infra fix.

Parallelization: at most three tracks run at once — (1) language/data B (+ its H Unicode
slice), (2) platform A → C → D → E, (3) packaging F → G. Calendar time is therefore
substantially less than session-sum; a realistic calendar projection is on the order of
12-20 months at this project's historical session cadence, with the Windows quick wins of
wave A landing first (Session 99 onward).

Largest risks (ranked):

1. **Concurrency safety**: threading the arena allocator + liveness table without a GC is
   the single most dangerous change to the validated memory model (E).
2. **Self-contained subsystem sizes**: regex and TLS from scratch without dependencies are
   multi-session efforts that tend to grow; contain them with read-first/ minimal-scope
   milestones.
3. **Ownership extensions** (Vec of owned Str, Map keys) can ripple through the checker —
   the classic B-wave surprise.
4. **Package manager scope creep**: resist registry + publishing in F; those are P2 and
   belong to a later ecosystem phase.
5. **Wide-API migration** (ANSI → W APIs) touches every fs/process path; schedule as a
   mechanical pass with byte-identical test vectors.
6. **Linux freeze discipline**: several frozen Linux files contain working env/net code;
   copy, do not share code paths.
7. **Test-infra flakiness**: loopback network tests are timing-flaky under full parallel
   load (Session 106: TCP timing; Session 107: the WSL Linux HTTP client test, 3/3 in
   isolation) and pass individually; fix the harness before concurrency work multiplies
   network tests (P3).

## 7. Final parity gate (Wave J exit criteria)

1. Matrix scan: **zero rows marked P1**; every row VERIFIED/IMPLEMENTED/INTENT. DIFF./N/A
   (PARTIAL and MISSING only where explicitly justified as non-blocking and documented).
2. Every parity category in §5 has a committed, running proof application.
3. Full native suite green; standalone exe + clean npm install verified.
4. Re-audit document produced with the same method as Session 98 (source+test+execution
   trace per claim).
5. Windows base still COMPLETE/STABLE; Linux untouched and still FROZEN.

## 8. Session 99 recommendation (exact objective)

Execute **Wave A, first tranche** — make the platform real for programs:

- Wire the Windows environment intrinsics (Get/SetEnvironmentVariableA; USERPROFILE home
  discovery) with native tests.
- Add `argv` (GetCommandLineW parse), stdin read, kernel32 `Sleep`, and a user-visible
  stderr write intrinsic.
- Add runtime error-location metadata (source line/function on E-R failures).
- Add float→Str and the first `str_format`.
- Bundle `stdlib/` into the npm package and add a module include path so installed users
  can `use strings` without copying files.
- Update the affected matrix rows to VERIFIED and land one automation proof app.

Session 99 gate: A-wave rows flipped, suite green, npm artifact re-verified clean.

*End of implementation plan — Session 98, commit a716df4.*
