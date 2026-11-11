# MINK — Canonical Roadmap (Locked)

**Status:** LOCKED strategic roadmap. Canonical planning document for the repository.
**Locked:** Session 128 (strategic roadmap lock).
**Baseline at lock:** MINK **1.0.4** · branch `main` · last milestone commit
`bb3ff1e3caafdb00004223ff4a5a204ecdf7724d`. The current repository build is
**1.0.5**, unpublished; the dated statuses in §8 record later state without
rewriting this locked baseline.
**Scope of this document:** strategy, sequencing, gates and the rules the project runs on.
It does not itself implement anything. It replaces the earlier phase-based plan, which is
preserved verbatim as an archived historical record in
[`docs/roadmap/archive/IMPLEMENTATION_ROADMAP_PHASE_PLAN_SUPERSEDED.md`](archive/IMPLEMENTATION_ROADMAP_PHASE_PLAN_SUPERSEDED.md).

> **History is preserved.** Prior completed milestone history (M0, M1) is recorded below and
> must not be rewritten to make the project look more successful. Verified capability,
> limitation, deliberate design difference and future goal are always distinguished.

---

## 1. Locked product thesis

> **MINK is a systems-oriented programming language and toolchain focused on making native
> software development simpler while retaining explicit control over the program.**

The strategic question the project must answer with evidence — not assumption — is:

> **Can MINK make a meaningful class of native software substantially simpler to build, test
> and ship than the alternatives?**

The project must prove this through evidence rather than assuming it.

### 1.1 What MINK is *not* described as

Native compilation and ownership are **capabilities, not unique competitive advantages.**
They may be necessary; they are not by themselves sufficient differentiation. The project
must therefore not describe its goal as any of:

- "Python but faster"
- "Rust but easier"
- "C++ replacement"
- "Go replacement"
- "C# replacement"
- "native binaries as the primary differentiator"
- "ownership as the primary differentiator"
- "100% Python feature parity"

The earlier positioning shorthand *"Python-like ease with C/C++-class power"* is retired as a
statement of the goal; it is retained only as historical design shorthand, subordinate to
this thesis. See `docs/core/MINK_MASTER_SPEC.md` §2.

## 2. Strategic rationale for this revision

The roadmap was revised because the completed M1 validation run changed what the project
can honestly claim, and because the project had drifted toward framing language-engineering
breadth (Python capability parity) as the destination.

- **Evidence replaced speculation.** M1 produced 10 real applications and 228 executable
  cases, and caught and corrected a real semantic defect. That is evidence of *practical
  usability*, which is the question that matters — not feature counts.
- **Parity was a capability-completeness exercise, not the product.** The Windows × official
  Python capability audit and its parity gate were internally useful (they forced real gaps
  into the open and drove the standard library and runtime work), and that gate is passed.
  But "achieve Python parity" is not the objective, and MINK does **not** need 100% Python
  feature parity. Parity framing is demoted to an archived capability audit (see
  §7 Document map).
- **Capabilities are not differentiators.** Native binaries and ownership were being treated
  as the reason to choose MINK. They are table stakes to be evaluated, not a moat. Whether
  they help depends on measurements the project has not yet taken.
- **The next question is stress, then comparison.** Before adding anything, the project must
  find where MINK *stops being practically usable* on Windows (M2), then compare end-to-end
  developer workflows against real alternatives (M4), then run a competitive workload
  shootout (M5), then have an outside developer try it (M6).
- **No Linux, no ecosystem, no claims before evidence.** Linux stays frozen and ecosystem
  features stay unbuilt until the strategic gate (M8) is passed.

## 3. Locked roadmap

### M0 — Language Foundation
- Historical foundation milestone.
- **COMPLETE.**

### M1 — Real Application Validation
- **COMPLETE.**
- 10 real applications.
- 228 executable cases.
- An actual semantic defect was discovered and fixed.
- Commit `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d`.
- Evidence (historical, preserved): `docs/validation/VALIDATION_MILESTONE_1_REPORT.md` and
  `docs/validation/VALIDATION_MILESTONE_1_CAPABILITY_MATRIX.md`.

### M2 — Windows Practical Stress Validation
- **COMPLETE (Session 138).**
- **Validation-first.** Do not implement missing capabilities merely because they are
  missing. Find where MINK stops being practically usable.

M2 coverage:

1. **Resource/scaling:** files above 4 MiB; large allocations; fragmentation; growing
   strings; large JSON; large HTTP bodies; stdin above 64 KiB; sustained execution.
2. **Process automation:** stdout above 4 KiB; stderr; child stdin; repeated child
   processes; invalid commands; pipeline-style workflows; cleanup.
3. **Networking:** IPv4 TCP; hostname TCP; DNS; plain HTTP; HTTPS; connection failures;
   timeouts; UDP; repeated connections.
4. **Server workloads:** local HTTP server; request parsing; response generation; concurrent
   requests; failure handling.
5. **Concurrency:** worker pools; shared state; locks; threads; tasks; repeated workloads.
6. **Integrated applications:** filesystem; SQLite; networking; concurrency; processes;
   structured data; CLI.

**Every failure must be classified:**

| Class | Meaning |
|---|---|
| **A** | MINK defect |
| **B** | missing capability |
| **C** | architectural limitation |
| **D** | deliberate language difference |
| **E** | test/application problem |

**Only genuine A-class defects automatically become implementation candidates.**

### M3 — Fundamental Reliability / Capability Closure
- **COMPLETE (Sessions 140–149).** Scope was determined from the M2 evidence
  (Sessions 128–138).
- Completed: if-expression/statement recovery, `net_dial` error preservation,
  closure MIR panic, `net_last_error` initialization safety, concurrent
  numeric/char print safety, thread-local HTTP last error, socket timeouts,
  UDP receive-buffer parity, HTTP/1.1 chunked decoding, parser trailing-token
  recovery, and `rt_print_str` one-write line atomicity.
- Prioritize blockers that prevent real applications from working.
- Likely areas may include runtime scaling/resource ceilings, DNS/hostname networking, HTTP
  correctness, process I/O limits, ownership consistency, compiler check/build consistency,
  generic inference, nested closures, diagnostics and module dependency hygiene.
- **Do not add arbitrary features.**

### M4 — Developer Experience Benchmark
- **COMPLETE (Sessions 150–162).** Measurement finished 2026-11-02: 450/450
  language-case combinations green, 3,910 runtime samples, 750 startup samples
  and 100 clean-build records retained. No composite score exists; runtime,
  startup, memory, build and correctness are reported as separate panels.
- Evidence: [`docs/benchmarks/M4/M4_SESSION_162_FINAL_REPORT.md`](../benchmarks/M4/M4_SESSION_162_FINAL_REPORT.md)
  and the committed derived statistics
  [`M4_RESULTS_SUMMARY.json`](../benchmarks/M4/M4_RESULTS_SUMMARY.json). Results are
  from one machine and one run period, not a universal language ranking.
- Method, workload selection and environment baseline:
  [`docs/benchmarks/M4/`](../benchmarks/M4/README.md).
- Selected workload categories: W1 JSON CLI, W2 local HTTP/JSON client,
  W3 in-memory common-SQL workload, W4 worker pool/pipeline, W5 integrated
  manifest utility.

Compare representative workflows using:

- MINK
- Python
- Rust
- Go
- C#

Measure where relevant:

- time to first working program
- source size
- dependency setup
- build time
- testing setup
- error recovery
- debugging effort
- deployment steps
- executable size
- startup
- memory
- runtime performance
- boilerplate
- external dependencies

**MINK is allowed to lose.** The purpose is to discover where MINK actually wins.

### M5 — Competitive Workload Shootout
- **COMPLETE (Sessions 163–177).** Three frozen applications (`repoaudit`,
  `hookbox`, `releasepack`) are GREEN with 163 executable checks per run, each
  shipped as one native PE executable. The milestone closed PARTIAL on a real
  stdlib JSON-capacity defect and was revised to COMPLETE only after Sessions
  174–175 fixed it with a backward-compatible API and permanent boundary
  regressions.
- Evidence: [`docs/benchmarks/M5/M5_FINAL_REPORT.md`](../benchmarks/M5/M5_FINAL_REPORT.md)
  and [`M5_SESSION_177_FINAL_REPORT_REVISION.md`](../benchmarks/M5/M5_SESSION_177_FINAL_REPORT_REVISION.md).
  M5 adds no timing, throughput or memory measurement; M4 owns the cross-language
  comparison, and neither milestone demonstrates broad superiority.

Use a small representative set:

1. Windows developer CLI
2. File/asset pipeline tool
3. SQLite local application
4. Network client
5. Local HTTP service
6. Automation tool
7. Integrated developer utility

Produce factual comparative evidence. **Do not manufacture benchmarks or cherry-pick only
favorable results.**

### M6 — External Developer Validation
A developer who did not build MINK must attempt predefined tasks.

Measure:

- time to completion
- setup difficulty
- compiler misunderstandings
- documentation gaps
- bugs encountered
- successful completion
- willingness to use MINK again

### M7 — Ecosystem / Tooling
**Only after actual demand is demonstrated.** Potential scope:

- package registry
- package discovery
- documentation generation
- formatter
- linter
- language server
- debugger
- profiler
- improved package tooling
- editor integration
- community libraries

**Do not build a giant ecosystem speculatively.**

### M8 — Strategic V1 Gate
Evaluate evidence and select **exactly one** outcome:

| Outcome | Meaning |
|---|---|
| **A** | **Clear competitive advantage** — continue toward serious V1 adoption. |
| **B** | **Strong niche** — position MINK around the proven niche. |
| **C** | **Technically useful but competitively weak** — reconsider differentiating language/toolchain design before major expansion. |
| **D** | **Fundamental failure** — stop feature expansion and rethink the project direction. |

### M9 — Platform Expansion
**Only after M8.** Linux and additional platforms remain **FROZEN** until this gate is
passed.

## 4. Locked success criterion

MINK does **NOT** need 100% Python feature parity.

MINK must establish that there is **at least one meaningful class of real software for which
developers can reasonably prefer MINK.**

The strongest desired outcome is evidence that MINK's integrated language + compiler +
runtime + standard library + tooling workflow provides a better development experience for a
clearly defined workload class.

## 5. Roadmap rules

1. Evidence before feature work.
2. Real applications before isolated feature-count claims.
3. Root-cause fixes before workarounds.
4. No feature creep from Python parity.
5. No Linux until the Windows practical-use gate is passed.
6. No competitive claims without comparative evidence.
7. MINK is allowed to lose benchmarks; record losses honestly.
8. Do not optimize for test count.
9. Do not build ecosystem features without demonstrated demand.
10. Every milestone must end with audit, re-verification, repository-state verification and an
    explicit next-step decision.
11. Preserve all prior completed milestone history.
12. Do not rewrite historical records to make the project appear more successful.
13. Clearly distinguish verified capability, limitation, deliberate design difference, and
    future goal.

## 6. Platform policy

| Platform | Status |
|---|---|
| Windows x86_64 | The only active platform for practical validation. |
| Linux | **FROZEN until M8.** A native `x86_64-linux-elf` backend exists in source and is not maintained; Linux is not a supported platform and no Linux work may start before the M8 gate. |
| macOS | Not supported. Future / to be decided after M8. |

The Linux implementation blueprint (`docs/roadmap/LINUX_X86_64_IMPLEMENTATION_PLAN.md`) is
inactive and must not be executed.

## 7. Document map (how this roadmap relates to the rest of `docs/`)

- **Canonical roadmap (this file)** — the single source of truth for strategy, sequencing and
  gates.
- **Archived:** [`docs/roadmap/archive/IMPLEMENTATION_ROADMAP_PHASE_PLAN_SUPERSEDED.md`](archive/IMPLEMENTATION_ROADMAP_PHASE_PLAN_SUPERSEDED.md) —
  the original `Phase 0`–`Phase 12` plan, preserved verbatim as history.
- **Capability audits (historical, gate passed, not the goal):**
  - `docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md`
  - `docs/roadmap/WINDOWS_PYTHON_PARITY_IMPLEMENTATION_PLAN.md`
  - `docs/audits/OFFICIAL_PYTHON_CAPABILITY_PARITY_AUDIT.md` (superseded baseline)
  These document *what MINK can do* against one internal reference. They no longer define
  the project's objective.
- **Milestone evidence:** `docs/validation/` (M1 report and capability matrix).
- **Session records:** `docs/implementation/` — one record per working session. The roadmap
  lock is recorded in `docs/implementation/SESSION_128_STRATEGIC_ROADMAP_LOCK.md`.
- **Design-only / gated:** `docs/ecosystem/*` (interoperability, package architecture,
  library priority) is design-only and may not be built before M7/M8 demand. The
  cross-platform and desktop/web architecture documents are future targets, not commitments.

## 8. Next action

*Superseded recommendation (retained for history).* The original next action was
"execute M4 Session 151" — build the benchmark harness skeleton and the W1
`jsontool` ports in all five languages before any M4 timing was claimed. M4 ran
through Session 162 and M5 through Session 177; both are closed.

**Current next action (2026-11-08):** recruit an independent developer and run
the M6 external validation. The package is **READY FOR EXTERNAL EVALUATION**
(Session 218); no tester has started it. Release-mode verification is complete (Session 213): a clean optimized build and
the full release suite pass at 85 suites / 3,262 passed / 0 failed / 9 ignored,
and the Sessions 205–210 `rustc` release crash no longer reproduces (Session 212
records it as a transient host fault with no source or toolchain change). M6 — a
developer who did not build MINK attempting predefined tasks — is the only open
milestone item; M7 remains gated on demonstrated demand.

*Dated status, 2026-11-06 (Sessions 190–193).* M4 and M5 are closed. The
standard-library ownership work that this roadmap treats as a prerequisite is
now complete: all 96 exit-106 tolerances across nine suites were removed, every
suite that runs compiled MINK programs requires exit 0, and the three deferred
suites (`collections`, `filesystem`, `strings`) are strict and green under a
tested heap/borrowed/arena contract
([`M6_SESSION_193_OWNERSHIP_CLOSURE.md`](../benchmarks/M6/M6_SESSION_193_OWNERSHIP_CLOSURE.md)).
The next open milestone is M6 external developer validation, which requires a
developer who did not build MINK; M7 remains gated on demonstrated demand.

*Dated status, 2026-11-08 (Sessions 212–214 — release verification).* Release
verification is **PASS**. A clean isolated optimized build succeeded in 30.78 s
(`target/s213_clean/release/mink.exe`, 4,048,384 bytes, SHA-256
`87a1cae4946f121e292d1a516e6047ad287b6506c5fe52a16208717dc2b6da23`), the full
release suite reported **85 suites, 3,262 passed, 0 failed, 9 ignored** (identical
to debug, exit 0, no test skipped or weakened), and the release soak passed 8/8
under its 60 s budget. The ten M2 validation applications were rebuilt with the
release compiler and replayed against their Python references: **228/228 cases**
match. The earlier `rustc` crash is a non-reproducing host-level transient, not a
source defect, and was not concealed. M6 external developer validation remains
**OUTSTANDING**; 1.0.5 stays unpublished, with no tag or GitHub release
([`M6_SESSION_213_RELEASE_VERIFICATION.md`](../benchmarks/M6/M6_SESSION_213_RELEASE_VERIFICATION.md),
[`VALIDATION_MILESTONE_2_AUDIT.md`](../validation/VALIDATION_MILESTONE_2_AUDIT.md)).

*Dated status, 2026-11-08 (Sessions 215–216 — external validation).* The M6
external package and protocol are ready
([`docs/validation/M6_EXTERNAL/`](../validation/M6_EXTERNAL/README.md)): two install
paths, exact version and SHA-256, quick start, documentation index, example map,
test instructions, known limitations, bug-reporting format, and 18 timed tasks
with expected outcomes and M6 acceptance thresholds. **No independent developer
was available**, so none was invited and no external evidence exists; the blocker
is recorded rather than worked around, and no feedback was fabricated. M6 stays
**OUTSTANDING** (not PARTIAL — external testing has not begun). MINK source is
unchanged by both sessions, so the Session 213 checkpoints still describe the
tree. M7 remains demand-gated, M8 not started, Windows x86_64 supported, Linux
frozen, 1.0.5 unpublished with no tag or release.

*Dated status, 2026-11-08 (Sessions 217–218 — package reproducibility).* Seven
active documents that still described the current build as 1.0.4 were corrected
to name 1.0.5 while preserving the 1.0.4 audit baseline; registry statements
still say npm serves 1.0.4 and 1.0.5 is unpublished. The M6 package was then
re-verified independently of its preparation: a clean clone at `ebb3bc8` built
with `cargo build --release` in 21.38 s (exit 0, 60 warnings, 0 errors), the
exact reference binary (`target/s213_clean/release/mink.exe`, 4,048,384 bytes,
SHA-256 `87a1cae4...b6da23`) matches the package, all 15 examples, 10 tutorial
programs, 10 validation applications and 27 stdlib modules exist, and all 18
task identifiers and timings are internally consistent. Package status:
**READY FOR EXTERNAL EVALUATION**. M6 remains **OUTSTANDING** because no
independent developer has run the protocol. 1.0.5 remains unpublished.

*Dated status, 2026-11-08 (Sessions 219–228 — internal engineering audit).* A
ten-session engineering audit ran without an external tester, so it adds
internal evidence only and does not change M6. It fixed two standard-library
contract violations: `str_trim`, `str_trim_start` and `str_trim_end` now trim
all six ASCII whitespace bytes (VT `0x0B` and FF `0x0C` were missed although
the reference promises ASCII whitespace), and `encoding.byte_is_whitespace`
now agrees with them. The `str_replace` / `str_replace_all` reference rows and
the map missing-key behaviour (`E-R11`, exit 111) are now documented accurately.
New regressions cover the trim entry points, first-occurrence replacement
semantics and every ASCII whitespace byte. Verification: `strings_lib` 90/90
in debug and release, `encoding_lib` 76/76 in debug and release,
`stdlib_signatures` 1/1, `stdlib_docs` 1/1; SHA-256 known vectors, JSON/CSV
round-trips, filesystem return values and collection operations were re-probed.
No compiler source changed, so the Session 213 release checkpoint and the
reference binary (4,048,384 bytes, SHA-256 `87a1cae4...b6da23`) still apply.
M6 remains **OUTSTANDING**, M7 demand-gated, M8 gated, Linux frozen, and 1.0.5
unpublished. See
[`M6_SESSION_219_ENGINEERING_AUDIT.md`](../benchmarks/M6/M6_SESSION_219_ENGINEERING_AUDIT.md).

*Dated status, 2026-11-08 (Sessions 229–238 — M6 readiness).* The external
package was re-reviewed against the documented APIs. Five task expectations
across four tasks were false-defect traps and were corrected: there is no file
append mode, and a missing file returns an empty string/`-1` rather than
exiting non-zero (T9); malformed JSON returns `0` with no position (T10);
SQLite is an in-memory `Str` blob with no cross-process persistence (T12); and
a refused connection returns `-1` with a Winsock code rather than exiting the
process (T14). T6, T8 and T11 now state their variant,
missing-key and record-oriented constraints. The language guide now documents
the command-line, stderr and stdin intrinsics T17 requires. Added a per-task
submission form,
verified FAQ, acceptance checklist, and coordinator triage procedure; the
README carries a handoff manifest and the current build-source statement. A
clean clone at `1f6f79a` built in 21.36 s (exit 0, 60 warnings, 0 errors),
reported `mink 1.0.5`, produced 4,048,384 bytes, and ran both a standalone and
a `mod filesystem;` program. The reference binary remains 4,048,384 bytes with
SHA-256 `87a1cae4...b6da23`. **M6 remains OUTSTANDING**: no independent
evaluator has submitted evidence, M7 stays demand-gated, M8 stays gated, Linux
stays frozen, and 1.0.5 remains unpublished.

*Dated status, 2026-11-08 (Session 239 — struct patterns in match).* A real
core-language capability was implemented instead of another readiness audit:
struct patterns now work in both `match` statements and `match` expressions.
The change spans parser, type checker/exhaustiveness, HIR field-pattern
representation, MIR member extraction, and ownership binding registration.
Field patterns may be bindings, wildcards, integer/boolean literals, nested
structs, tuples, ranges, enum payloads, and or-patterns; guards compose with
them. Refutable struct fields require a catch-all unless every field is
irrefutable (no product coverage yet), and `..` rest syntax is not included.
`tests/struct_match.rs` adds 23 passing tests, including 7 native executions;
`examples/struct_match_geometry` and `examples/struct_match_dispatch` build and
run with exit 0. Focused debug and release verification is recorded with this
work. M6 remains **OUTSTANDING** (no external developer), M7 demand-gated, M8
gated, Linux frozen, and 1.0.5 unpublished. See
[`STRUCT_PATTERN_MATCHING_IMPLEMENTATION.md`](../implementation/STRUCT_PATTERN_MATCHING_IMPLEMENTATION.md).

*Dated status, 2026-11-06 (Sessions 194–196).* Windows x86_64 runtime reliability
was re-audited under bounded stress and one confirmed defect was fixed: collection
handles are now validated against the liveness table, so using a freed `Vec`,
`Map` or `Set` faults `E-R05` (105) instead of returning stale bytes and exiting
0. Double-free (`E-R04`), out-of-bounds (`E-R10`) and leak (`E-R06`) behaviour is
unchanged. Full regression is 3,194 passed / 0 failed / 1 ignored across 78
suites; the release build and `invsync` pass. A freed-then-reused handle can
still alias the replacement block because the table has no generation counter;
that is recorded as a limitation, not fixed. This work does not satisfy M6: no
external developer has attempted the predefined tasks, so M6 remains open and
M7 remains gated. Linux remains frozen. See
[`M6_SESSION_194_RUNTIME_STRESS_AUDIT.md`](../benchmarks/M6/M6_SESSION_194_RUNTIME_STRESS_AUDIT.md),
[`M6_SESSION_195_COLLECTION_UAF_FIX.md`](../benchmarks/M6/M6_SESSION_195_COLLECTION_UAF_FIX.md)
and
[`M6_SESSION_196_RELIABILITY_CLOSURE.md`](../benchmarks/M6/M6_SESSION_196_RELIABILITY_CLOSURE.md).

*Dated status, 2026-11-06 (Session 197).* The residual collection-handle alias
limitation recorded above is closed. Handles now carry the allocation
generation in bits 48..63 of the word, so a freed `Vec`/`Map`/`Set` handle
whose block is reused by a new allocation faults `E-R05` (105) instead of
reading the replacement's header; a plain double free remains `E-R04` (104)
and out-of-bounds remains `E-R10` (110). The MINK ABI is unchanged (8-byte
opaque handle, same services, same layout). Measured regression is 3,185
passed / 0 failed / 0 ignored across 75 suites; `tls_lib` is excluded because
its TLS test server needs `python3`, which is not installed on the verification
machine. Eleven `s197_*` regressions cover reuse, repeated cycles, nesting,
wraparound and four-thread concurrency. M6 external validation remains
outstanding, M7 remains gated and Linux remains frozen; this does not change
the milestone state. See
[`M6_SESSION_197_GENERATION_SAFE_HANDLES.md`](../benchmarks/M6/M6_SESSION_197_GENERATION_SAFE_HANDLES.md).

*Dated status, 2026-11-06 (Session 198).* Runtime resource lifecycle audited
across allocation, collections, filesystem, processes, threads and
concurrency, with a new instrumented suite (`tests/resource_lifecycle.rs`, 7
tests) that samples the child process's OS handle count while it runs. No
confirmed defect in the audited paths: 3,000 filesystem cycles, 3,000 failing
filesystem calls, 40 process spawns, 3,000 thread spawn/join cycles and
200,000 allocation cycles all ran with a flat handle count and exit 0, and a
negative control proves the sampler detects a 400-handle leak. One unverified
code-level risk is recorded, not patched: `rt_fs_read` ignores `ReadFile`'s
result and byte count, so a short or failed read would return stale heap
bytes at the requested length (no deterministic trigger exists in the current
API). See
[`M6_SESSION_198_RESOURCE_LIFECYCLE.md`](../benchmarks/M6/M6_SESSION_198_RESOURCE_LIFECYCLE.md).

*Dated status, 2026-11-07 (Session 199).* Sustained, bounded workloads were
soaked in one process with a hard wall-clock budget per run and 10 ms sampling
of the child's OS handle count and peak working set: 6 compiler builds, 100
program launches, 400,000 collection cycles, 3,000 filesystem cycles plus five
1 MiB reads, 100 child processes, 20,000 error-path iterations and three
200-iteration combined runs (collections + filesystem + threads + process).
All exited 0 with empty stderr, zero internal check failures and flat handle
counts; the suite is 7 passed / 0 failed in 8.3 s. No confirmed defect. One
over-large workload (64 KiB × 12,000) hit the already-documented no-coalesce
arena ceiling as `E-R02` (102) at iteration 342 with 1027 MiB peak, failing
loudly; the bounded workload was resized inside that documented limit rather
than changing the allocator. See
[`M6_SESSION_199_BOUNDED_SOAK.md`](../benchmarks/M6/M6_SESSION_199_BOUNDED_SOAK.md).

*Dated status, 2026-11-07 (Session 200).* Compiler correctness and diagnostics
were audited with a new 24-case contract battery (`tests/diagnostics.rs`) that
pins exit code 1, error code, file:line, no panic marker and byte-identical
stderr across repeated compiles. Two confirmed defects were fixed: integer
literals that do not fit in `Int` now fail with `E-T43` instead of silently
wrapping (including negated literals and match patterns, while
`-9223372036854775808` stays legal), and aggregate type diagnostics no longer
render the `is not a valid key type` prefix borrowed from `E-T42`. The battery
passes 24/24 in debug and release; full regression is 78 suites with every
target except `tls_lib` (its TLS test server needs `python3`, absent on the
audit machine). M6 external validation remains outstanding and M7 remains
gated. See
[`M6_SESSION_200_COMPILER_DIAGNOSTICS.md`](../benchmarks/M6/M6_SESSION_200_COMPILER_DIAGNOSTICS.md).

*Dated status, 2026-11-07 (Session 201).* The standard-library surface and CLI
developer experience were reviewed. Of 700 top-level stdlib declarations, 462 are
public and the reference covered 397 of them (85.9%); the 65 missing
(math 20, http 13, time 12, log 7, byte 7, rt 2, str 1, net 1, network
shutdown constants 2) are now listed
with source signatures in an appendix, and a new guard test scans
`stdlib/*.mink` so an undocumented public function fails the suite. The CLI
contract was verified correct: `help` and no arguments exit 0; unknown
commands and missing files exit 1 with clear messages. No code defect. See
[`M6_SESSION_201_STDLIB_DX.md`](../benchmarks/M6/M6_SESSION_201_STDLIB_DX.md).

*Dated status, 2026-11-07 (Session 202).* Release readiness was verified for the
six-session reliability programme (197–202): **3,218 passed / 0 failed / 7
explicitly ignored across 79 suites** (every target except `tls_lib`, whose TLS
server needs the absent `python3`), release build clean at 4,043,776 bytes, and
release-mode diagnostics (24), soak (7) and lifecycle (7) suites all green.
The programme added 51 tests and fixed three confirmed defects (generation-safe
collection handles, integer-literal wraparound `E-T43`, aggregate diagnostic
prefix). Residual limits are recorded, including the unpatched `rt_fs_read`
short-read risk, the 1 GiB no-coalesce arena ceiling and `MAX_LIVE_ALLOCS`.
M6 external validation remains outstanding, M7 remains gated and Linux remains
frozen. See
[`M6_SESSION_202_RELEASE_READINESS.md`](../benchmarks/M6/M6_SESSION_202_RELEASE_READINESS.md).

*Dated status, 2026-11-07 (Session 203).* The Session 198 filesystem-read
finding was confirmed and fixed. `rt_fs_read` discarded `ReadFile`'s BOOL and
transferred-byte count, so a failed or short read returned a string whose
length claimed the full file size over uninitialised or stale heap bytes. The
emitter now records the actual byte count in a per-call stack slot (not the
shared `.bss` scratch, avoiding a cross-thread length race), returns an owned
empty string when `ReadFile` fails, and exposes only the bytes transferred. A
byte-range lock gives a deterministic reproduction: before the fix the program
reported 4096 bytes for a failing read, after it reports 0, frees the result
and exits cleanly, and the same executable reads 4096 bytes once the lock is
released. Six new tests in `tests/fs_read.rs`; full regression **3,224 passed /
0 failed / 7 ignored / 80 suites**. No ABI change. Files ≥ 4 GiB remain outside
the supported range. See
[`M6_SESSION_203_FILESYSTEM_READ.md`](../benchmarks/M6/M6_SESSION_203_FILESYSTEM_READ.md).

*Dated status, 2026-11-07 (Session 204).* Runtime capacity was audited. The
1 GiB no-coalesce arena and `MAX_LIVE_ALLOCS = 1024` are intentional, documented
limits: a request above the ceiling is `E-R02` (102) before any commit, an
`i64::MAX` size cannot wrap, and the 1,025th live allocation is `E-R03` (103)
with no partially linked metadata. The 16-bit handle generation had a real ABA
defect: after 65,536 reuses of one block the tag wrapped back to a stale
handle's value, and the stale handle was accepted (red check: exit 110 instead
of 105). Freed blocks now retire at generation `0xFFFF` instead of being
reused, so generation 0 is never issued and stale handles stay `E-R05`
permanently; the cost is one abandoned block per 65,536 reuses and the handle
encoding is unchanged. Seven tests in `tests/capacity.rs`; full regression
**3,231 passed / 0 failed / 7 ignored / 81 suites**. See
[`M6_SESSION_204_CAPACITY_AND_GENERATIONS.md`](../benchmarks/M6/M6_SESSION_204_CAPACITY_AND_GENERATIONS.md).

*Dated status, 2026-11-07 (Session 205).* The `tls_lib` environment failure was
root-caused: `python3` is the Microsoft Store alias (spawns, then exits 49 with
"Python was not found"), while CPython 3.11.9 is installed as `python`. The
harness now probes `python3`, `python` and `py -3`, accepts only a successful
`Python 3` probe, captures server stderr, reaps the child on startup failure
and adds `s205_tls_interpreter_is_usable`. The full suite now executes: **21
passed / 0 failed / 1 ignored**, repeated three times, no leaked processes or
logs. The ignored test is the opt-in public-internet system-store control. A
separate environment blocker appeared during verification: `rustc` at
opt-level 3 crashes with `STATUS_ILLEGAL_INSTRUCTION` /
`STATUS_HEAP_CORRUPTION` / `STATUS_ACCESS_VIOLATION` on every release build,
including previously passing targets; debug builds are unaffected and this is
not a repository defect. Release verification is recorded as not run. See
[`M6_SESSION_205_TLS_HARNESS.md`](../benchmarks/M6/M6_SESSION_205_TLS_HARNESS.md).

*Dated status, 2026-11-07 (Session 206).* Large-file handling was audited. The
public filesystem API is whole-file only (no seek/read-at-offset), but both
`fs_file_size` and `fs_read` used the 32-bit `GetFileSize`, so a file of 4 GiB
+ 12,345 reported 12,345 and its read returned 12,345 leading bytes; exactly
4 GiB reported 0, and 4 GiB−1 aborted with `E-R02` while trying to allocate
4 GiB. Both services now use `GetFileSizeEx`: `fs_file_size` is exact at and
above 4 GiB, and `fs_read` refuses a file larger than the 1 GiB arena with an
owned empty string after closing the handle. The new import is appended at IAT
index 44 so no earlier index moves. Five sparse-file tests in
`tests/large_file.rs` (`SetEndOfFile`, 16-byte tail, cleanup guard); red run
4 failed / 1 passed, after the fix 5/5. Release verification remains blocked
by the environment `rustc` crash. See
[`M6_SESSION_206_LARGE_FILES.md`](../benchmarks/M6/M6_SESSION_206_LARGE_FILES.md).

*Dated status, 2026-11-07 (Session 207).* Reliability evidence was extended with
a bounded 300-second mixed soak (`MINK_SOAK_SECONDS`, hard cap 900 s): 1,440
fresh program launches (Vec, allocate/free, strings, filesystem write/read/
size/missing-file, child process), 72 compiler builds, 216 deterministic
double-free runs and two full `tls_lib` suites. Observed: 300.1 s, all 1,440
launches exit 0, 4.1 MiB peak working set, handle count 73 → 79, 298 KB of
intentional TLS fixture growth, zero crashes/hangs. The test is `#[ignore]`d
and enforces its own per-launch 30 s budget and cleanup. Release re-runs remain
blocked by the environment `rustc` crash. See
[`M6_SESSION_207_EXTENDED_SOAK.md`](../benchmarks/M6/M6_SESSION_207_EXTENDED_SOAK.md).

*Dated status, 2026-11-07 (Session 208).* Section 26 is the Session 201 appendix
of public stdlib functions omitted from the main sections, not a language
feature section. Its 66 signatures were audited against `stdlib/*.mink`; all
match exactly and no function is missing. The former gap was that
`tests/stdlib_docs.rs` only checked name mentions. New
`tests/stdlib_signatures.rs` extracts every §26 signature and compares it with
the source `fn`, failing on any name, parameter, arity or return-type drift;
it passes 1/1. Behavior remains covered by the module suites. No API change.
See
[`M6_SESSION_208_SECTION26_SIGNATURES.md`](../benchmarks/M6/M6_SESSION_208_SECTION26_SIGNATURES.md).

*Dated status, 2026-11-07 (Session 209).* Cross-subsystem seams were audited
along the compiler→runtime→stdlib path, with four new permanent tests in
`tests/cross_subsystem.rs`: eight live file reads across recursive collection
churn, stale-handle rejection after filesystem churn, a failed `ReadFile`
followed by collection and read work in the same process, and live-table
exhaustion while file data is live. All four pass; no new defect was found.
The full regression independently re-exercises the 203/204/206 fixes and is
clean at **85 suites, 3,262 passed, 0 failed, 9 ignored**. See
[`M6_SESSION_209_CROSS_SUBSYSTEM.md`](../benchmarks/M6/M6_SESSION_209_CROSS_SUBSYSTEM.md).

*Dated status, 2026-11-07 (Session 210 — final audit).* Six-session programme
complete through Session 209, with Session 210 **PARTIAL**: repository
integrity, versions (Cargo and npm 1.0.5), documentation and the full debug
regression are clean (**85 suites, 3,262 passed, 0 failed, 9 ignored**), but
the release build cannot be produced on this machine. Every release `rustc`
compile crashes with `STATUS_ILLEGAL_INSTRUCTION`, `STATUS_HEAP_CORRUPTION`
or `STATUS_ACCESS_VIOLATION`, including after a forced toolchain reinstall;
debug is unaffected and a trivial release crate compiles. Release build and
release-mode suites are therefore **not run**. Final verdict: **NOT READY**
(environment release-verification blocker plus outstanding M6 external
validation), not a confirmed source defect. See
[`M6_SESSION_210_FINAL_ACCEPTANCE.md`](../benchmarks/M6/M6_SESSION_210_FINAL_ACCEPTANCE.md).

*Dated status, 2026-11-10 (Sessions 254–258 — defer audit).* An independent
audit of the new `defer` implementation found and fixed two correctness
defects. A value-less block expression lowered its statements without its own
cleanup scope, so its `defer` ran at the enclosing function's exit instead of
the block's; that path now uses normal block lowering. A mutable `Str` binding
that looked immutable at registration could later be assigned an owned
allocation and freed before cleanup, producing a runtime invalid access; the
`E-S18` check is now type- and mutability-aware, and also walks tuple and
collection contents. Three adversarial regressions cover these cases, and the
suite grew to 33 permanent tests, all passing in debug and release. The
stdlib bundle drift was also fixed: `encoding.mink` and `strings.mink` in
`npm/mink/stdlib/` were both stale copies of the post-`bf3fe81` repository
modules. The guarded copies are now byte-identical, and the exact
`s107_npm_stdlib_bundle_matches_repo_stdlib` test passes. Full suite:
**89 suites, 3,322 passed, 0 failed, 9 ignored**. Milestone state is
unchanged: M6 **OUTSTANDING**, M7 demand-gated, M8 gated, Linux frozen, and
1.0.5 unpublished.

*Dated status, 2026-11-10 (Sessions 249–253 — deferred cleanup).* The
language gained statement-only `defer { ... }` scope-exit cleanup with LIFO
ordering across normal exit, `return`, `break`, `continue`, nested scopes and
conditional registration. It is implemented through the full compiler
pipeline to native Windows MIR and covered by 25 permanent tests in
`tests/defer.rs`, all passing in debug and release, plus two examples that
build and run in both profiles. Unsafe cases are rejected statically:
`return`/escaping loop control flow and defer-with-trailing-expression blocks
(`E-S17`), and deferred reads of owned or borrowed locals (`E-S18`). A
return-value mutation defect found during verification was fixed by
snapshotting the value before cleanup, with regression coverage. Full suite:
**89 suites, 3,313 passed, 1 failed, 9 ignored**; the single failure is the
pre-existing `s107_npm_stdlib_bundle_matches_repo_stdlib` mismatch between
`stdlib/encoding.mink` and `npm/mink/stdlib/encoding.mink`, unrelated to this
work and not changed here. See
[`DEFER_IMPLEMENTATION.md`](../implementation/DEFER_IMPLEMENTATION.md).
Milestone state is otherwise unchanged: M6 remains **OUTSTANDING**, M7
demand-gated, M8 gated, Linux frozen, and 1.0.5 unpublished.

*Dated status, 2026-11-08 (Session 211 — documentation reconciliation).* A
repository-wide documentation audit found and corrected three internal
contradictions; no milestone state changed. (1) This roadmap marked M4 "IN
PROGRESS" and left M5 unmarked while §8 recorded both as closed, although
`M4_SESSION_162_FINAL_REPORT.md` and `M5_FINAL_REPORT.md` already document their
completion; both are now marked COMPLETE with their evidence. (2) The next
action still pointed at M4 Session 151; it now points at release-mode
verification. (3) `README.md` still described M2 as the next milestone, claimed
the standard library retains unreleased owned strings in `encoding` and
`strings` (closed in Sessions 190–193), and stated that MINK had never been
benchmarked (M4/M5 exist); those statements are corrected without deleting the
historical records. Re-verified at this HEAD: **85 suites, 3,262 passed, 0
failed, 9 ignored**, and `origin/main` equals local HEAD. Release-mode
verification remains **BLOCKED**; M6 external validation remains outstanding;
M7 remains demand-gated; Linux remains frozen; 1.0.5 remains unpublished.

*Dated status, 2026-11-11 (Sessions 259–266 — ownership-aware deferred cleanup,
75% checkpoint).* `defer` can now clean up owned resources safely through an
explicit transfer: `defer move (a, b) { ... }` moves the listed locals into the
deferred operation when the statement executes. The originals become moved
(`E-S10`), the cleanup reads capture slots nothing else can reach, and the
cleanup therefore frees exactly once — including on `return`, `break`, and
`continue`, per loop iteration, on conditional registrations, and inside
nested scopes, structs, and enum payloads. Plain `defer` and its `E-S18`
protection are unchanged, and one false positive was removed: bindings declared
*inside* a deferred body are cleanup-local, so a cleanup can allocate
temporaries and match an owned payload. A new `E-S19` rejects transfers the
ownership model cannot make sound — references, arrays, tuples, collections,
pointers, aggregates containing them, non-local or `const` names, duplicate
names, registrations inside another deferred block, and captures the cleanup
never uses (which would leak). Transfer while borrowed is `E-S12`. Captures are
ordinary MIR locals assigned at the registration site and remapped while the
body is lowered, so no backend or runtime change was needed; a
monomorphization defect found by the generic-instantiation test (a cloned
body's capture list kept the original span and resolved to another
instantiation's binding) was fixed by re-spanning the capture list with the
clone. `tests/defer_move.rs` adds 58 permanent tests (parser, semantics,
ownership, native execution) and three examples build and run in debug and
release: `defer_move_resource`, `defer_move_multi`, `defer_move_pipeline`. Full
suite: **90 suites, 3,380 passed, 0 failed, 9 ignored**; the `defer` suites
pass in release too, and `cargo build --release` succeeds here (the
session-210 release crash did not reproduce). `cargo fmt --check` still
reports 14 files, all untouched by this work and already failing at the base
commit (verified against a pristine `git archive HEAD` copy); every file this
work touched is formatting-clean. This is the 75% checkpoint of the
ownership-aware cleanup milestone, not its completion. Milestone state is
otherwise unchanged: M6 **OUTSTANDING**, M7 demand-gated, M8 gated, Linux
frozen, and 1.0.5 unpublished. See
[`DEFER_IMPLEMENTATION.md`](../implementation/DEFER_IMPLEMENTATION.md).

---

## Appendix A — Historical phase plan notice

The original `docs/roadmap/IMPLEMENTATION_ROADMAP.md` was a phase-based implementation and
architecture-freeze plan (`Phase 0` Specification Completion … `Phase 12` Compatibility and
Release Engineering). It has been superseded by this locked roadmap and archived verbatim
(with its original text) at
[`docs/roadmap/archive/IMPLEMENTATION_ROADMAP_PHASE_PLAN_SUPERSEDED.md`](archive/IMPLEMENTATION_ROADMAP_PHASE_PLAN_SUPERSEDED.md).
It is retained so prior planning is not erased; it must not be read as the current plan.
