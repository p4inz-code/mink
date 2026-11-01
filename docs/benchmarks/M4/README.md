# M4 — Developer Experience Benchmark

**Status:** IN PROGRESS — W1 `jsontool` is COMPLETE (Sessions 151-152) and
W2 `netcheck` is implemented and correctness-green (Session 153). **W3 is
BLOCKED**: the Rust and Go SQLite drivers are not available locally and
installing them needs network authorization (Session 154). W4-W5 are not
started. **No M4 timing or performance measurement exists yet.**

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
| [`M4_SESSION_154.md`](M4_SESSION_154.md) | W3 contract and dependency audit; **blocked** on unavailable Rust/Go SQLite drivers |

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

Executable evidence lives in [`benchmarks/m4/`](../../../benchmarks/m4/).
