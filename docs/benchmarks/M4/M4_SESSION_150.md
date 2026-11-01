# Session 150 — M4 initiation: methodology and preparation

**Date:** 2026-11-01
**Scope:** M4 methodology, workload selection, environment baseline and
repository/evidence audit only. **No MINK source, compiler, runtime, stdlib,
tests, version, package or release state was changed.**
**Result:** COMPLETE for methodology/preparation. **No M4 measurements were
taken**, so no comparative claim is made in this session.

## 1. What was done

1. Read the canonical roadmap and the M1/M2/M3 evidence records, including
   the M1 ten-application suite, the M2 final audit, and Sessions 140–149.
2. Inspected the actual validation applications, fixtures and harnesses, the
   bundled standard library, the compiler CLI, and the current worktree.
3. Verified all five toolchains with real hello builds and runs: MINK,
   Python 3.11.9, Rust 1.97.1 MSVC, Go 1.26.7 and .NET 10.0.302.
4. Recorded artifact sizes, PE imports, startup smoke values and missing
   tools. These are calibration, not M4 results.
5. Selected five reusable M1 workload contracts: W1 JSON CLI, W2 local
   HTTP/JSON client, W3 in-memory common-SQL workload, W4 worker
   pool/pipeline, W5 integrated manifest utility.
6. Wrote the pre-registered methodology, workload selection, environment
   baseline and this session record.
7. Updated the canonical roadmap from "M4 NEXT" to "M4 IN PROGRESS —
   methodology and preparation (Session 150)", explicitly **not complete**.
8. Smoke-ran `examples/inventory_db/main.mink`: exit 0, SQL create/insert/
   update/delete/transaction output correct. This verifies the SQLite library
   is usable; it does not add a durable-database API (there is none).

## 2. Repository state

### Before this session

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `691ec38386bb37e204b834e61b0ff8de99a0ac65` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Divergence | ahead 7, behind 1 |
| Version | 1.0.4 |
| Staged | none |
| Modified tracked | 40 |
| Untracked entries | 26 |
| `git diff --check` | clean |
| M3 full suite | 3,168 passed / 0 failed / 1 ignored (Session 149) |
| Compiler warnings | 59 existing |

### After this session

| Item | Value |
| --- | --- |
| HEAD | unchanged `691ec38386bb37e204b834e61b0ff8de99a0ac65` |
| `origin/main` | unchanged `b860837a8a6039f304a503e3d85728a643ab470b` |
| Version | unchanged 1.0.4 |
| Staged | none |
| Modified tracked | 40 (all pre-existing; the roadmap is one of them) |
| Untracked entries | 27 (`docs/benchmarks/` is one new untracked directory) |
| Untracked files | 31 with `--untracked-files=all` (26 pre-existing + 5 M4 docs) |
| `git diff --check` | clean |
| Commit | **none** |

The repository still carries the uncommitted Sessions 128–149 work. The M4
documents are new and would be isolatable by themselves, but the canonical
roadmap is already modified by that earlier work. Committing only the new
docs would omit the M4 roadmap update; committing the roadmap would absorb
unrelated Session 128–149 changes. No commit was attempted. No reset, clean,
stash, rebase, amend, push, tag or publish was performed.

## 3. Evidence reviewed

| Source | What it supplied |
| --- | --- |
| `docs/roadmap/IMPLEMENTATION_ROADMAP.md` | M4 scope, M5+ sequencing, MINK-allowed-to-lose rule, Windows/Linux policy |
| `docs/validation/VALIDATION_MILESTONE_1_REPORT.md` | 10 applications, 228 cases, Python references, zero E-R06; M1 limitations |
| `docs/validation/VALIDATION_MILESTONE_1_CAPABILITY_MATRIX.md` | 20 verified rows and 12 prioritized gaps (ANSI paths, 1024 live allocs, JSON arena, SQLite cost, HTTP scope) |
| `docs/implementation/SESSION_138_M2_P9_FINAL_WINDOWS_GAP_AUDIT.md` | M2 closure, 3,073 combined passes, official ceilings and fixed defects |
| Sessions 140–149 | M3 closure: parser recovery, net error preservation, print concurrency, thread-local HTTP state, socket timeouts, UDP parity, chunked HTTP, one-write string lines |
| `validation/*/record.md` + `main.mink` + `python_ref.py` | Concrete contracts and limitations for W1–W5 |
| `stdlib/*.mink` | Actual APIs: JSON arena, HTTP chunked/4 MiB, SQLite in-memory/common SQL, CSV, hashing, threads |
| `README.md` | Installation/distribution claims, CLI, Windows-only status, explicit "no performance claims" |
| Current toolchains | Versions, paths, hello builds, artifact sizes, PE imports, startup calibration |

Historical counts were not re-run in this session. They remain the recorded
M1/M2/M3 evidence; M4 does not rewrite them.

## 4. Verified toolchain baseline

| Toolchain | Version | Smoke result |
| --- | --- | --- |
| MINK | 1.0.4, SHA-256 `d8f61348…2749f09` | build/run exit 0; 47,104 B PE; `kernel32.dll` only |
| Python | 3.11.9 CPython 64-bit | run exit 0; no native artifact; `python3` is unusable Store stub |
| Rust | 1.97.1, `x86_64-pc-windows-msvc` | exit 0; 125,952 B PE; VCRUNTIME140 + UCRT imports |
| Go | 1.26.7 windows/amd64 | exit 0; 2,457,088 B PE; `kernel32.dll` only |
| .NET / C# | SDK 10.0.302, target `net10.0` | build/run exit 0; framework-dependent 162,304 B apphost + 4,608 B assembly; no `csc` on PATH |

Smoke startup medians (10 launches; calibration only, not M4 results): MINK
2.84 ms, Python 17.83 ms, Rust 3.75 ms, Go 4.84 ms, C# 34.98 ms via
`dotnet <dll>`. Exact minima/maxima and caveats are in
[`M4_ENVIRONMENT_BASELINE.md`](M4_ENVIRONMENT_BASELINE.md).

## 5. Decisions

1. **Five workloads, not seven.** W1 covers CLI/files/JSON; W2 HTTP/JSON;
   W3 local SQL; W4 concurrency; W5 integrated filesystem/JSON/hash/CSV.
   Standalone regex, CSV, process automation, HTTPS and raw UDP/TCP are
   excluded or demoted to probes with reasons recorded.
2. **Four independent panels, no composite score.** Developer experience,
   build/distribution, runtime/resources and correctness/reliability are
   reported separately. Correctness gates every timing result.
3. **M1 reuse.** W1–W5 are reduced versions of validated M1 applications, not
   new invented contracts. M1's historical cases are not M4 measurements.
4. **W3 is in-memory only.** `stdlib/sqlite.mink` exposes no durable
   open/save API; the database is a `Str` blob. W3 compares the common SQL
   subset and transactions, not file persistence, SQLite compatibility,
   `ORDER BY`/`JOIN`/`GROUP BY`, or engine internals.
5. **W2 is HTTP only.** TLS trust-store differences and the lack of an HTTP
   timeout helper would test missing features rather than the selected
   contract. Chunked and Content-Length responses are included.
6. **Python 3.11.9**, not 3.14.3, to preserve M1 continuity. The installed
   3.14.3 is recorded but not used.
7. **C# framework-dependent primary**, self-contained/single-file as a
   separate distribution probe. This avoids hiding the runtime requirement
   or inflating the primary artifact.
8. **Measurement protocol pre-registered:** 2 warm-ups + 15 runs for
   workloads, 30 runs for startup, 5 clean builds, shuffled order, median/
   p95/min/max/IQR, no outlier deletion, explicit materiality thresholds.
9. **Canonical CRLF stdout.** MINK's print convention is applied to all five
   implementations; produced data files keep their format-specific line
   endings (CSV LF in W5). This is a deliberate contract, not a performance
   trick.
10. **No MINK changes.** M4 is measurement, not a feature or fix milestone.

## 6. Documents created

| File | Content |
| --- | --- |
| `docs/benchmarks/M4/README.md` | Area index, status, boundaries |
| `docs/benchmarks/M4/M4_COMPARISON_METHODOLOGY.md` | Objective, fairness rules, panels, protocol, thresholds, infrastructure proposal |
| `docs/benchmarks/M4/M4_WORKLOAD_SELECTION.md` | Five workloads, contracts, dependencies, exclusions, comparability |
| `docs/benchmarks/M4/M4_ENVIRONMENT_BASELINE.md` | Machine, repo, toolchains, build configs, smoke evidence, missing tools |
| `docs/benchmarks/M4/M4_SESSION_150.md` | This record |

The canonical roadmap is the only modified tracked file in this session
(M4 status + next action). All other changes are the five new documents.

## 7. Open questions and limits

- External Rust/Go/C# packages are **not installed or version-pinned**. The
  next implementation session needs authorization to restore them, or must
  use only stdlib-capable subsets (not possible for JSON/HTTP/SQLite in Rust).
- W3's 100-row baseline must be timed inside MINK's documented default-heap
  envelope; the header's ~220 four-column-row ceiling and O(N²) statement
  copying are limitations, not things to optimize away in M4.
- W4's runtime panel is expected to be partial: Python's GIL, Go's goroutines
  and .NET's thread pool are not equivalent to explicit OS threads.
- `sqlite3.exe`, `csc.exe` and `objdump`/`dumpbin` are absent. Libraries,
  `dotnet build` and the read-only Python PE parser are the workarounds.
- No full Rust suite was re-run in this docs-only session. The baseline
  remains Session 149's 3,168/0/1; no source changed, so this is not a new
  test result.
- The benchmark infrastructure does not exist yet; only its proposed layout
  is documented.

## 8. Exact next task

**Session 151 — create the benchmark harness skeleton and the W1
`jsontool` ports.**

Concretely:

1. Create `benchmarks/m4/` with the documented layout, deterministic W1
   fixtures, golden outputs and a Python 3.11 stdlib-only validation/run
   harness.
2. Implement W1 in MINK, Python, Rust, Go and C# against the exact contract
   in `M4_WORKLOAD_SELECTION.md`.
3. Pin dependency versions/licenses and record restore/build times.
4. Run all W1 golden/error cases in all five implementations; fix only
   implementation defects, not the contract.
5. Stop before claiming any M4 performance result until W1 is green in all
   five and the harness has captured raw JSONL.

W2–W5 remain unstarted. M4 must not be marked complete in Session 151 unless
all five workloads, all golden cases and the measurements exist; that is not
the expected outcome of one session.
