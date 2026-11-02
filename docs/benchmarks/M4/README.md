# M4 — Developer Experience Benchmark

**Status:** IN PROGRESS. All five workloads are correctness-green in all five
languages: **450/450 language-case combinations** (Session 159). W3's Rust and
Go SQLite drivers are resolved and pinned (Session 158). The measurement
harness and preconditions are verified, but **no timing, startup, memory or
clean-build measurement has been run yet**, and no language is claimed to be
faster, smaller or easier.

## Objective

Determine, with reproducible evidence, whether MINK offers a meaningful
developer-experience advantage for a defined class of native Windows software
by comparing the same useful outcomes in MINK, Python, Rust, Go and C#.

MINK is allowed to lose. The goal is to discover where it actually wins, where
it is merely different, and where it is not competitive.

## Documents

| Document | Purpose |
| --- | --- |
| [`M4_COMPARISON_METHODOLOGY.md`](M4_COMPARISON_METHODOLOGY.md) | Scope, fairness rules, measurement protocol, interpretation rules, infrastructure proposal |
| [`M4_WORKLOAD_SELECTION.md`](M4_WORKLOAD_SELECTION.md) | Selected workloads, contracts, dependencies, comparability limits, exclusions |
| [`M4_ENVIRONMENT_BASELINE.md`](M4_ENVIRONMENT_BASELINE.md) | Machine, toolchains, build configurations, smoke calibration, missing dependencies |
| [`M4_SESSION_150.md`](M4_SESSION_150.md) | Session 150 evidence, decisions, repository state, exact next task |
| [`M4_SESSION_151.md`](M4_SESSION_151.md) | W1 contract resolution, fixtures, oracle, five implementations, harness, correctness |
| [`M4_SESSION_152.md`](M4_SESSION_152.md) | W1 clean builds, repeatability, contract audit, dependency audit |
| [`M4_SESSION_153.md`](M4_SESSION_153.md) | W2 contract, local fixture server, five implementations, correctness |
| [`M4_SESSION_154.md`](M4_SESSION_154.md) | W3 contract, final row shape, 500-row evidence, 3/5 implementations; Rust/Go **blocked** on unavailable drivers |
| [`M4_SESSION_155.md`](M4_SESSION_155.md) | W4 contract, concurrency model per language, 130/130 correctness, repeatability |
| [`M4_SESSION_156.md`](M4_SESSION_156.md) | W5 contract, 100/400-file fixtures, hashing/CSV, 70/70 correctness, Unicode-path evidence |
| [`M4_SESSION_157.md`](M4_SESSION_157.md) | Full W1-W5 matrix, repeatability, build/dependency readiness, fairness audit, harness audit |
| [`M4_SESSION_158.md`](M4_SESSION_158.md) | W3 dependency closure: rusqlite 0.32.1 bundled and modernc.org/sqlite v1.61.1, 65/65 green |
| [`M4_SESSION_159.md`](M4_SESSION_159.md) | Final 450/450 correctness sweep, determinism and measurement preflight audit |

## Hard boundaries

- Windows x86_64 only. Linux is frozen and is not part of M4.
- MINK **1.0.4**; no language, compiler, runtime, stdlib, package or version
  changes in the methodology session.
- No internet services in workloads; deterministic local fixtures only.
- Python, Rust, Go and C# are competitors, not references to imitate. Each
  implementation must be idiomatic and must pass the same correctness gate.
- No single composite score. Correctness, developer effort, build and
  distribution, runtime and resources are reported as separate panels.
- Historical M1/M2/M3 evidence is preserved as recorded; M4 does not rewrite
  those results to fit this methodology.

## Results

No performance results exist. The only results recorded so far are
correctness gates:

| Workload | Implementations | Cases | Gate |
| --- | --- | --- | --- |
| W1 `jsontool` | MINK, Python, Rust, Go, C# | 10 scored + 13 supporting | PASS 115/115 |
| W2 `netcheck` | MINK, Python, Rust, Go, C# | 8 scored + 6 supporting | PASS 70/70 |
| W3 `dbshell` | MINK, Python, Rust, Go, C# | 10 scored + 3 supporting | PASS 65/65 |
| W4 `poolrun` | MINK, Python, Rust, Go, C# | 8 scored + 18 supporting | PASS 130/130 |
| W5 `manifest` | MINK, Python, Rust, Go, C# | 10 scored + 4 supporting | PASS 70/70 |
| **Total** | | **46 scored + 44 supporting** | **PASS 450/450** |

Executable evidence lives in [`benchmarks/m4/`](../../../benchmarks/m4/).
