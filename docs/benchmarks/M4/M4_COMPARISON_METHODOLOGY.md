# M4 — Comparison Methodology

**Status:** prepared in Session 150 (2026-11-01); not yet executed.
**Scope:** Windows x86_64. MINK 1.0.4 against Python 3.11.9, Rust 1.97.1
(MSVC), Go 1.26.7 and .NET/C# 10.0.
**Rule:** this document is pre-registered. If a metric, threshold or protocol
changes after measurements begin, the change and its reason must be recorded
before the affected results are interpreted.

## 1. Objective and research questions

The strategic question is:

> Can MINK make a meaningful class of native software substantially simpler to
> build, test and ship than the alternatives?

M4 does not try to answer that for all software. It tests a compact, defined
set of Windows developer workloads and asks:

1. **Developer effort.** For the same contract, how much active time,
   iteration and boilerplate does each language require before the program is
   correct?
2. **Diagnostics and safety.** Where do compile-time types, ownership and
   error messages catch defects earlier or later than the alternatives?
3. **Build and distribution.** What does a clean release build, a native
   artifact, startup and shipping to another Windows machine actually cost?
4. **Runtime and resources.** Are execution time and memory materially
   different for the selected workloads, once correctness is equal?
5. **Practical envelope.** Which MINK limits exclude otherwise reasonable
   workloads, and are those limits loud and documented?

## 2. Scope and non-goals

**In scope:** five representative workloads selected in
[`M4_WORKLOAD_SELECTION.md`](M4_WORKLOAD_SELECTION.md), implemented in all
five languages against shared fixtures and golden output.

**Out of scope:**

- proving MINK universally better, or producing a single ranking;
- microbenchmarks of individual language features;
- language-feature checklists or "lines of code" as a productivity score;
- HTTP/2, TLS redesign, GUI, ML, databases beyond the selected SQL subset;
- Linux, macOS, package-registry or ecosystem work;
- changing MINK to make a benchmark pass;
- public-internet or third-party hosted services;
- release tagging, publishing, version changes or branding.

A failed MINK case is evidence, not a reason to change MINK during M4. Any
capability gap found is recorded and, if it blocks a workload, the workload is
reported as failed/not comparable rather than silently weakened.

## 3. Evidence baseline

M4 starts from, but does not re-run, these historical results:

| Milestone | Evidence | Status |
| --- | --- | --- |
| M1 | 10 real applications, 228 executable cases against Python references, zero leaks at exit | COMPLETE; [`docs/validation/`](../../validation/VALIDATION_MILESTONE_1_REPORT.md) |
| M2 | Practical-use stress validation, release workloads, loud documented ceilings | CLOSED; Sessions 128–138 |
| M3 | 10 reliability/capability items, full suite 3,168 passed / 0 failed / 1 ignored, release build green | COMPLETE; Sessions 140–149 |

M1 is the canonical source of reusable workload definitions
(`validation/*/main.mink`, `python_ref.py`, fixtures and records). M2/M3
provide verified capability and limitation evidence. Neither M1 nor M2 is a
five-language benchmark: M1 compared MINK with Python on functionality, and
M2 used temporary probes. M4 is the first comparative evidence set.

The MINK release binary used for M4 must be pinned by SHA-256, not only by
version, because the current worktree contains uncommitted M3 work and HEAD
does not contain all of it. See the environment baseline.

## 4. Evaluation model

Four **independent** panels are reported. They are not combined into one
score.

### A. Developer experience

- Active time from first source file to first correct golden-case pass.
- Number of build/run attempts and failed attempts before correctness.
- Compiler/runtime diagnostics encountered and whether they identified the
  cause.
- Dependency setup: commands, package count, network need, lockfile, time.
- Test setup: framework used, test code size, cases covered, commands.
- Time to implement a predefined later modification, and resulting diff.
- Source size and structure: physical lines, non-blank/non-comment lines,
  functions, external imports. These are descriptive, never a score.
- Ownership/type/error handling burden, recorded qualitatively with concrete
  examples.

### B. Build and distribution

- Clean release build success and wall time; dependency restore time separate.
- Reproducibility: repeat clean builds and compare artifact hashes where the
  toolchain permits deterministic output; otherwise record why hashes differ.
- Artifact type and size: native executable, framework-dependent output,
  interpreter source, required sidecar files.
- Runtime requirements: PE imports, interpreter/runtime/CRT dependencies,
  target-machine prerequisites.
- Ship steps: files to copy, install commands, environment assumptions.
- Process startup: fresh-process end-to-end time for a minimal correct
  program, measured separately from workload runtime.
- Build cache conditions and flags, recorded exactly.

### C. Runtime and resources

- End-to-end wall time per workload, parent-measured.
- User and kernel CPU time from `GetProcessTimes`.
- Peak working set (method in §6); total and, as a secondary figure,
  incremental over the language's empty-program baseline.
- Binary and output sizes; optional I/O counters for file-heavy workloads.
- Variability: median, p95, min/max and interquartile range; raw values kept.

### D. Correctness and reliability

- Byte-exact stdout, stderr, exit code and produced files against golden
  output for every case.
- Error-path behavior: usage, missing input, malformed input, boundary input,
  transaction/rollback where applicable.
- Resource cleanup: no observable leak, no orphan process, no stuck server.
- Boundary evidence: documented MINK limits are tested separately and
  reported as limits, not as performance.

**Correctness is a gate.** A language/workload that fails any required case is
reported as failed and is excluded from the runtime comparison for that
workload. A fast incorrect implementation never outranks a correct one.

## 5. Fairness rules

These rules are mandatory.

1. **Same contract.** Every implementation reads the same fixture bytes,
   performs the same operation and emits the same canonical bytes and exit
   codes.
2. **Idiomatic code.** Use each language's normal standard-library style and
   accepted package for the task. Do not deliberately write poor Python, Rust,
   Go or C# code, and do not over-optimize MINK at the cost of readability.
3. **Standard library first.** Use a third-party package only when the
   standard library lacks the capability (for example JSON in Rust, HTTP in
   Rust, SQLite drivers). Record every package, version, license and why it
   was needed.
4. **Comparable dependencies.** Do not add a framework, ORM, CLI framework or
   web framework to one language unless the equivalent is required in the
   others. The dependency burden is part of the evidence, not something to
   hide.
5. **Release builds.** Compare optimized/release outputs for every language:
   MINK `build`, Python 3.11 interpreter, Rust `cargo build --release`, Go
   `go build` with recorded flags, C# `dotnet publish -c Release`. Never
   compare one language's debug build with another's release build.
6. **Exact environment.** Same machine, OS build, power state and background
   conditions. Record the toolchain paths/versions and the MINK binary hash.
7. **Same data and order.** Shared deterministic fixtures, fixed seed,
   randomized but recorded language order per repetition, no concurrent
   benchmark runs (except the concurrency workload itself).
8. **Repeated measurement.** Warm-ups and repeats as defined in §6; report
   variability, not just a best time.
9. **No hidden work.** Dependency restore, compilation, test setup and
   install steps are measured separately and reported. They must not be
   folded into runtime.
10. **No post-hoc metric changes.** Thresholds and metrics are fixed here
    before data collection.
11. **No cherry-picking.** Every selected language is reported for every
    selected workload, including failures and slow results.
12. **Line count is not productivity.** Source metrics are descriptive
    evidence alongside effort and correctness.

### Deliberate conventions

- **Newlines.** MINK's `rt_print_str` appends CRLF. To keep byte comparison
  honest, M4's canonical stdout is UTF-8 with CRLF line endings; every
  implementation must emit exactly that. This is an inherited MINK/M1
  convention, applied to all languages, not a claim that CRLF is better.
  Produced data files use the format's specified line endings (CSV LF in W5)
  and are also compared byte-for-byte.
- **argv.** MINK's `rt_argc`/`rt_argv` exclude the program name; the harness
  passes only user arguments to every implementation.
- **Errors.** Workloads define application-level exit codes and short stable
  stderr text. Underlying exception/error text is not compared because the
  platforms expose different error vocabularies; the observable application
  contract is compared instead.

## 6. Measurement protocol

### 6.1 Preflight

1. Verify machine, OS, toolchains and MINK binary hash; write metadata JSON.
2. Build every implementation from its documented clean command.
3. Run the full golden-case suite for every language and workload.
4. Confirm zero orphan processes/servers and clean output directories.
5. Record dependency restore/build times separately from runtime.

### 6.2 Build measurement

- Five clean builds per language after dependencies are restored and pinned.
- Clean means previous application output/bin/obj directories removed; shared
  compiler/dependency caches are retained and their state is recorded. This
  avoids destructive global cache wipes while making the local build step
  reproducible.
- Report median, min/max and failures. A build failure is a correctness
  result, not a slow time.
- Run a second clean build for hash reproducibility; if hashes differ, record
  the cause (build IDs, timestamps, paths) instead of claiming reproducibility.

### 6.3 Runtime measurement

- Two warm-up runs per implementation, then **15 measured runs** per
  implementation/workload.
- The parent harness launches the process, captures stdout/stderr to files,
  records exit code, and measures wall time with
  `time.perf_counter_ns()` (Windows QPC).
- Language order is shuffled per repetition with a fixed seed and recorded.
  This reduces systematic cache/order bias.
- For each run record wall time, user/kernel CPU time, peak working set,
  stdout/stderr hashes, exit code and artifact metadata.
- Start-up is measured separately on the minimal hello workload with **30
  runs**, reported as median/p95/min/max and labeled separately from workload
  runtime.
- If a workload's median is under 100 ms in every language, its runtime panel
  is marked **startup-dominated / not material**; no strong runtime claim is
  made. It is still valid for developer-effort and distribution evidence.
- Warm means the process is fresh but OS/file caches are not deliberately
  dropped. Windows has no supported unprivileged cache-drop; therefore M4
  does not claim "cold-cache" performance. The first post-build launch is
  recorded separately as "first launch", not as cold-cache.

### 6.4 Peak memory

Primary method: place the child in a Windows Job Object and read
`PeakProcessMemoryUsed` after exit, with a fallback to
`GetProcessMemoryInfo().PeakWorkingSetSize` sampled at 10 ms if the Job Object
figure is unavailable. Record which method was used. Report total process
memory; for interpreter/runtime languages also report the incremental figure
over the language's empty-program baseline as a **secondary** number. Never
subtract the baseline from the primary result.

### 6.5 CPU and I/O

- `GetProcessTimes` supplies user and kernel CPU time; do not use a sampled
  instantaneous "CPU %" as a primary metric.
- `GetProcessIoCounters` is recorded for file-heavy workloads where it is
  stable and useful; it is not a substitute for wall time.

### 6.6 Statistics and materiality

- Report **median**, **p95**, **min**, **max** and **IQR**; keep every raw
  value in JSONL.
- Do not delete outliers. A run is invalid only if the harness, fixture or
  machine failed; record it and exclude only for that documented reason.
- Pre-registered "no material difference" rule:
  - runtime: median ratio below 1.10 **or** overlapping IQRs;
  - startup: difference below 2 ms **or** below 10%;
  - memory: difference below 10% **or** below 1 MiB.
  Differences below these thresholds are reported as "no clear difference",
  not as a win.
- No statistical significance claim is made from 15 runs on one machine.

## 7. Developer-effort measurement

Historical development time is not available for the M1 MINK applications and
must not be invented. M4 measures only newly written implementations.

For each new port record:

| Field | Method |
| --- | --- |
| Start / first-correct timestamps | UTC wall clock in the session log |
| Active implementation time | Sum of active work intervals, excluding unattended builds/tests and idle gaps; record the method and uncertainty |
| Build/run iterations | Count invocations until first full golden pass |
| Compile/runtime failures | Count and classify (syntax, type, ownership, API misuse, logic, environment) |
| Dependencies added | Name, version, license, install command, network required |
| Source size | Physical lines, non-blank/non-comment lines, functions, imports; generated by one documented script |
| Modification task | Apply one predefined change after the baseline passes; record time to green, diff size, and new failures |
| Documentation/diagnostic use | Count `mink explain`/docs/compiler-message lookups where observable; otherwise mark not measured |

**Limitation:** one developer/agent implementing all five ports is a case
study, not universal proof of productivity. Language familiarity differs.
Record this explicitly in every developer-experience conclusion.

## 8. Interpretation rules

1. Correctness first. A failed implementation is never ranked by speed.
2. Report each panel separately; no weighted total.
3. A result applies to the selected workload and fixture size, not to the
   language in general.
4. Distinguish a **language/runtime property** (for example Python's GIL or
   .NET's framework dependency) from an **implementation choice** (for
   example a manual CSV writer).
5. Distinguish **verified capability**, **documented limitation**, **not
   comparable**, and **not measured**. Do not phrase one as another.
6. If MINK loses, report the loss with the same detail as a win.
7. MINK's known limits are reported as boundary evidence, not hidden by
   choosing only favorable fixture sizes:
   - ANSI filesystem/no Unicode path names;
   - `MAX_LIVE_ALLOCS = 1024` simultaneous allocations;
   - JSON arena: roughly 3.5 KiB accepted input, ~2 KiB serialization in the
     documented arena;
   - HTTP 4 MiB body budget;
   - SQLite in-memory engine with a common-SQL subset and ~220 four-column
     rows on the default heap;
   - no durable SQLite open/save API;
   - no integrated debugger/profiler.
8. Historical M1/M2 numbers were not produced under this protocol. They are
   context, not directly comparable M4 results.

## 9. Known limitations of the methodology

- One Windows machine, one OS build, one developer/agent, one time period.
- No controlled CPU pinning, power profiling, cache dropping or Defender
  exclusion; background OS activity can add noise.
- Python is measured through CPython 3.11.9 (M1 continuity); 3.14.3 is
  installed but not used. Results do not represent all Python runtimes.
- C# has two legitimate distributions (framework-dependent and
  self-contained). The primary comparison uses framework-dependent output;
  self-contained is a separate distribution probe, not the primary number.
- Rust/Go/C# dependency choices affect build time and artifact size; exact
  versions must be pinned before measurement.
- Source line counts and one developer's effort are weak proxies for
  productivity.
- No long-duration soak, memory-leak profiler, debugger session or production
  load test is part of M4.
- No external developer validation; that is M6.
- MINK's compiler binary is built from a worktree containing uncommitted M3
  work. The SHA-256 pin makes the measured compiler reproducible even though
  the source tree is not a clean commit.

## 10. Proposed benchmark infrastructure

No infrastructure is created in Session 150. The proposed minimal layout is:

```text
benchmarks/m4/
  README.md
  env/
    baseline.json          # machine/toolchain metadata, hashes
  fixtures/
    w1_json/               # shared input bytes + generator
    w2_http/
    w3_sqlite/
    w4_pool/
    w5_manifest/
  expected/
    w1_json/               # golden stdout/stderr/exit/files
    ...
  apps/
    mink/{w1_json,w2_http,w3_sqlite,w4_pool,w5_manifest}/
    python/...
    rust/...
    go/...
    csharp/...
  harness/
    build.py               # clean builds, artifact + build-time capture
    run.py                 # fixture/server setup, process measurement, JSONL
    validate.py            # byte-exact golden comparison
    stats.py               # medians/IQR/p95 and Markdown tables
    pe_imports.py          # read-only PE import inspection
  results/
    raw/                   # JSONL, one file per run set
    reports/               # generated human-readable reports
  .build/                  # ignored build outputs
```

Why this fits the repository:

- It is separate from `validation/`, which is historical M1 evidence and must
  not be rewritten, and from `tests/`, which tests the compiler, not
  cross-language applications.
- It reuses the established M1 pattern of one application + fixtures +
  verifier, but generalizes the verifier to five implementations and a shared
  golden oracle.
- The harness is Python 3.11 standard library only (`subprocess`, `time`,
  `ctypes`, `json`, `hashlib`); no `psutil` or other dependency is needed.
  Windows Job Objects and `GetProcessTimes` are accessed through `ctypes`.
- Raw JSONL is machine-readable and retained; Markdown reports are generated,
  not hand-edited.
- Build outputs and downloaded packages are ignored; fixtures, golden outputs,
  source, pinned manifests and raw result summaries are committed.

### Raw result record (proposed)

```json
{
  "run_id": "2026-11-01T00:00:00Z-w1-python-03",
  "workload": "w1_json",
  "language": "python",
  "phase": "run",
  "iteration": 3,
  "order": 7,
  "exit_code": 0,
  "stdout_sha256": "...",
  "stderr_sha256": "...",
  "wall_ns": 0,
  "user_cpu_ns": 0,
  "kernel_cpu_ns": 0,
  "peak_memory_bytes": 0,
  "memory_method": "job-object",
  "artifact_bytes": 0,
  "valid": true,
  "note": ""
}
```

### Commands (to be implemented)

```text
python benchmarks/m4/harness/build.py --workload w1_json
python benchmarks/m4/harness/validate.py --workload w1_json
python benchmarks/m4/harness/run.py --workload w1_json --runs 15
python benchmarks/m4/harness/stats.py results/raw/<file>.jsonl
```

No `benchmarks/` tree exists yet. Creating it belongs to the next session,
after this methodology is accepted.

## 11. Change control

- This methodology and the workload contracts are frozen before the first
  measured run.
- Any necessary change is recorded in the session log with the reason and
  applied to all languages, not only to the affected implementation.
- If a workload cannot satisfy the fairness rules, it is removed or marked
  not comparable **before** timing data is interpreted; a failed MINK result
  is never removed merely because it is unfavorable.
