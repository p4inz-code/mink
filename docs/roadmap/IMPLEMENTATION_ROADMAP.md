# MINK — Canonical Roadmap (Locked)

**Status:** LOCKED strategic roadmap. Canonical planning document for the repository.
**Locked:** Session 128 (strategic roadmap lock).
**Baseline:** MINK **1.0.4** · branch `main` · last milestone commit
`bb3ff1e3caafdb00004223ff4a5a204ecdf7724d`.
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
- **IN PROGRESS — methodology and preparation (Session 150).** No benchmark
  results exist yet; do not mark M4 complete.
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

**Recommended next action: execute M4 Session 151.** Build the benchmark
harness skeleton and the W1 `jsontool` ports in all five languages, with
shared fixtures and golden output, before any M4 timing is claimed. M3 is
closed; M4 compares representative workflows against Python, Rust, Go, and C#
without adding new language, ecosystem, or platform features.

*Dated status, 2026-11-06 (Sessions 190–193).* M4 and M5 are closed. The
standard-library ownership work that this roadmap treats as a prerequisite is
now complete: all 96 exit-106 tolerances across nine suites were removed, every
suite that runs compiled MINK programs requires exit 0, and the three deferred
suites (`collections`, `filesystem`, `strings`) are strict and green under a
tested heap/borrowed/arena contract
([`M6_SESSION_193_OWNERSHIP_CLOSURE.md`](../benchmarks/M6/M6_SESSION_193_OWNERSHIP_CLOSURE.md)).
The next open milestone is M6 external developer validation, which requires a
developer who did not build MINK; M7 remains gated on demonstrated demand.

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

---

## Appendix A — Historical phase plan notice

The original `docs/roadmap/IMPLEMENTATION_ROADMAP.md` was a phase-based implementation and
architecture-freeze plan (`Phase 0` Specification Completion … `Phase 12` Compatibility and
Release Engineering). It has been superseded by this locked roadmap and archived verbatim
(with its original text) at
[`docs/roadmap/archive/IMPLEMENTATION_ROADMAP_PHASE_PLAN_SUPERSEDED.md`](archive/IMPLEMENTATION_ROADMAP_PHASE_PLAN_SUPERSEDED.md).
It is retained so prior planning is not erased; it must not be read as the current plan.
