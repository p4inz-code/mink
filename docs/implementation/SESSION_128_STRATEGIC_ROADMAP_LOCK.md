# Session 128 — Strategic roadmap lock (documentation only)

STATUS: **complete.** Documentation / roadmap synchronization only. No language or runtime
feature was implemented, no test changed, no version changed, no release/tag/package/publish
operation occurred, and no Linux behavior was touched.

Baseline: HEAD `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d` ("validation: close Windows
practical parity milestone 1"), `Cargo.toml` **1.0.4**, branch `main` — **no version bump.
Linux remains FROZEN. Windows x86_64 remains the only active platform for practical
validation.**

## 1. Objective

Permanently record a **locked strategic roadmap** in the repository's planning documentation:
replace the stale phase-based plan and the "Python-parity-as-goal" framing with the locked
M0–M9 roadmap, its product thesis, its gates and its rules — without changing code, version,
release state, or platform behavior.

## 2. What was locked

**Product thesis (locked):**

> MINK is a systems-oriented programming language and toolchain focused on making native
> software development simpler while retaining explicit control over the program.

**The strategic question the project must answer with evidence, not assumption:**

> Can MINK make a meaningful class of native software substantially simpler to build, test
> and ship than the alternatives?

**Strategic correction (locked).** Native compilation and ownership are **capabilities, not
unique competitive advantages.** The goal must not be described as "Python but faster",
"Rust but easier", "C++ replacement", "Go replacement", "C# replacement", "native binaries as
the primary differentiator", "ownership as the primary differentiator", or "100% Python
feature parity".

## 3. Locked roadmap (summary)

- **M0 — Language Foundation.** Historical foundation; **COMPLETE**.
- **M1 — Real Application Validation.** **COMPLETE**: 10 real applications, 228 executable
  cases, an actual semantic defect discovered and fixed, commit
  `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d`.
- **M2 — Windows Practical Stress Validation.** **NEXT.** Validation-first; do not implement
  missing capabilities merely because they are missing; find where MINK stops being
  practically usable. Coverage: resource/scaling (files >4 MiB, large allocations,
  fragmentation, growing strings, large JSON, large HTTP bodies, stdin >64 KiB, sustained
  execution); process automation (stdout >4 KiB, stderr, child stdin, repeated child
  processes, invalid commands, pipelines, cleanup); networking (IPv4 TCP, hostname TCP, DNS,
  plain HTTP, HTTPS, connection failures, timeouts, UDP, repeated connections); server
  workloads (local HTTP server, request parsing, response generation, concurrent requests,
  failure handling); concurrency (worker pools, shared state, locks, threads, tasks, repeated
  workloads); integrated applications (filesystem, SQLite, networking, concurrency,
  processes, structured data, CLI). Every failure is classified **A** (MINK defect) / **B**
  (missing capability) / **C** (architectural limitation) / **D** (deliberate language
  difference) / **E** (test/application problem); **only genuine A-class defects automatically
  become implementation candidates.**
- **M3 — Fundamental Reliability / Capability Closure.** Scope derived from M2 evidence;
  prioritize blockers that prevent real applications from working; no arbitrary features.
- **M4 — Developer Experience Benchmark.** Compare representative workflows across MINK,
  Python, Rust, Go and C# on time-to-first-program, source size, dependency setup, build time,
  testing setup, error recovery, debugging effort, deployment steps, executable size, startup,
  memory, runtime performance, boilerplate and external dependencies. **MINK is allowed to
  lose.**
- **M5 — Competitive Workload Shootout.** Seven representative workloads (Windows developer
  CLI, file/asset pipeline, SQLite app, network client, local HTTP service, automation tool,
  integrated developer utility); factual comparative evidence; no manufactured or
  cherry-picked benchmarks.
- **M6 — External Developer Validation.** A developer who did not build MINK attempts
  predefined tasks; measure completion time, setup difficulty, compiler misunderstandings,
  documentation gaps, bugs, success and willingness to use MINK again.
- **M7 — Ecosystem / Tooling.** Only after actual demand is demonstrated.
- **M8 — Strategic V1 Gate.** Select exactly one outcome: A clear competitive advantage /
  B strong niche / C technically useful but competitively weak / D fundamental failure.
- **M9 — Platform Expansion.** Only after M8.

## 4. Strategic rationale recorded

- M1 turned practical-usability claims into evidence (10 applications, 228 executable cases,
  a real defect found and fixed). Evidence, not feature counts, is what the project may build
  on.
- The Windows × official-Python capability program was an **internal capability-completeness
  exercise** that surfaced real gaps and drove real work; its gate is passed. It is **not**
  the product objective, and MINK does **not** need 100% Python feature parity. That framing
  is archived and demoted.
- Native binaries and ownership were being framed as the reason to choose MINK; they are
  capabilities that must be tested, not an assumed moat.
- The sequencing discipline is locked: stress validation (M2) → reliability closure (M3) →
  developer-experience benchmark (M4) → competitive shootout (M5) → external validation (M6)
  → ecosystem only on demand (M7) → single strategic decision (M8) → platforms (M9).

## 5. Developer-experience and competitive-validation gates recorded

- **M4** is the developer-experience gate: end-to-end workflows measured against Python, Rust,
  Go and C#, with MINK explicitly allowed to lose.
- **M5** is the competitive workload gate: factual comparative evidence across seven
  representative workloads, no cherry-picking.
- **M6** is the external-validation gate: an outside developer using MINK.

## 6. M8 strategic decision gate recorded

At M8 the project evaluates evidence and selects **exactly one** outcome: (A) clear
competitive advantage → continue toward serious V1 adoption; (B) strong niche → position
around the proven niche; (C) technically useful but competitively weak → reconsider
differentiating language/toolchain design before major expansion; (D) fundamental failure →
stop feature expansion and rethink the direction.

## 7. Linux recorded as FROZEN until M8

Linux and additional platforms remain **FROZEN** until the M8 gate is passed (M9). The
`x86_64-linux-elf` backend exists in source and is not maintained; Linux is not a supported
platform; `docs/roadmap/LINUX_X86_64_IMPLEMENTATION_PLAN.md` is inactive and must not be
executed.

## 8. Documents changed or created

Created:
- `docs/roadmap/archive/IMPLEMENTATION_ROADMAP_PHASE_PLAN_SUPERSEDED.md` — the original
  phase-based plan, preserved **verbatim** with a superseded banner (history preserved).
- `docs/implementation/SESSION_128_STRATEGIC_ROADMAP_LOCK.md` — this record.

Rewritten:
- `docs/roadmap/IMPLEMENTATION_ROADMAP.md` — now the canonical **locked M0–M9 roadmap**
  (thesis, strategic rationale, roadmap, rules, success criterion, platform policy, document
  map, next action).

Updated (strategic framing / stale-statement corrections only, no historical data rewritten):
- `docs/roadmap/WINDOWS_PYTHON_OFFICIAL_CAPABILITY_PARITY.md` — strategic-framing banner.
- `docs/roadmap/WINDOWS_PYTHON_PARITY_IMPLEMENTATION_PLAN.md` — strategic-framing banner.
- `docs/audits/OFFICIAL_PYTHON_CAPABILITY_PARITY_AUDIT.md` — note that the classification
  ladder is not the project objective.
- `docs/roadmap/LINUX_X86_64_IMPLEMENTATION_PLAN.md` — banner now records FROZEN until M8.
- `docs/ecosystem/LIBRARY_PRIORITY_FRAMEWORK.md` — banner: ecosystem work is gated by M7/M8;
  performance-advantage scores are unverified hypotheses, not comparative evidence.
- `docs/core/MINK_MASTER_SPEC.md` — §2 positioning reconciled with the locked thesis; §16
  points to the canonical roadmap.
- `docs/core/LANGUAGE_DESIGN.md` — §1 positioning reconciled with the locked thesis.
- `docs/validation/VALIDATION_MILESTONE_1_REPORT.md` — note tying the milestone to the locked
  M1 and its framing.
- `README.md` — "What's Next" now references the locked roadmap, records Linux frozen until
  M8, and names M2 as the next milestone.

## 9. Stale statements removed or corrected

- The old phase-based roadmap (`Phase 0`–`Phase 12`) that claimed "the phases below remain
  the plan for everything not yet implemented" — removed from the canonical roadmap and
  archived as history.
- "100% Python feature parity" / "the parity gate is the objective" framing in the parity
  roadmap docs and the parity audit — demoted to an internal, passed capability audit; the
  locked success criterion explicitly says MINK does not need 100% Python feature parity.
- "Python-like ease with C/C++-class power" as the stated goal — retired in favour of the
  locked thesis (kept only as historical design shorthand, subordinate to the roadmap).
- Unverified "MINK can be faster than Python" competitive value-proposition claims in the
  library priority framework — flagged as hypotheses pending comparative evidence (rule 6).
- Forward-looking "expand platform support / ecosystem / production capabilities" language
  in `README.md` — replaced with the locked sequencing (Linux frozen until M8; M2 next).

## 10. Verification performed

- `git status`/`git diff` inspected: only Markdown documentation changed; **no source file
  and no test file changed**; `Cargo.toml` unchanged at `1.0.4`.
- No release/tag/package/publish command was run; `npm/mink/package.json` version unchanged.
- Linux: no file under `src/backend/emit/linux_runtime.rs` or the ELF emitter was touched.
- The canonical roadmap was re-read to confirm it carries only the locked M0–M9 strategy and
  no contradictory old strategy.

## 11. Repository status

Documentation-only change set (see §8). The working tree additionally carries pre-existing
untracked files that are outside this task (`nl1.exe`, `nl1.mink`, `stdlib/httpserver.mink`,
`npm/mink/stdlib/httpserver.mink`, `tests/httpserver_lib.rs`, and a pasted chat log); they
were not created or modified by this session.

## 12. Next action

**Begin M2 — Windows Practical Stress Validation.** Validation-first: find where MINK stops
being practically usable, classify every failure A–E, and let only genuine A-class defects
become implementation candidates. Do not start Linux work.
