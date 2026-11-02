# M4 Final Report — Correctness, Build, Runtime and Resources

**Date:** 2026-11-02
**Scope:** Windows x86_64; MINK 1.0.4, CPython 3.11.9, Rust 1.97.1 (MSVC),
Go 1.26.7, .NET SDK 10.0.302 / C# `net10.0`.
**Status:** **M4 measurement complete.** 450/450 language-case combinations
green, 3,910 runtime samples, 750 startup samples and 100 clean-build records
retained. This is evidence from one machine and one run period, not a
universal language ranking.

No composite score exists. Runtime, startup, memory, build and correctness are
reported as separate panels, using the materiality rules frozen in
[`M4_COMPARISON_METHODOLOGY.md`](M4_COMPARISON_METHODOLOGY.md).

## 1. Evidence executed

| Phase | Protocol | Records |
| --- | --- | --- |
| Correctness | full W1–W5 suite, twice, byte-exact | 450/450 green, two sweeps identical |
| Runtime | 2 warm-ups + 15 measured runs per scored case/language, seed 20261101 | 3,910 |
| Startup | 30 no-argument runs per language/workload | 750 |
| Clean build | 5 builds per compiled language/workload | 100 (20 per workload) |
| Memory | peak working set from the same runtime/startup processes | 4,660 processes carrying RSS |

Raw JSONL is retained under `benchmarks/m4/results/raw/` (git-ignored).
The committed derived statistics are
[`M4_RESULTS_SUMMARY.json`](M4_RESULTS_SUMMARY.json). No sample was deleted;
the one failed W3 Rust clean-build attempt is preserved separately.

## 2. Correctness and reliability

| Workload | Cases (scored/support) | MINK | Python | Rust | Go | C# |
| --- | --- | --- | --- | --- | --- | --- |
| W1 `jsontool` | 23 (10/13) | 23/23 | 23/23 | 23/23 | 23/23 | 23/23 |
| W2 `netcheck` | 14 (8/6) | 14/14 | 14/14 | 14/14 | 14/14 | 14/14 |
| W3 `dbshell` | 13 (10/3) | 13/13 | 13/13 | 13/13 | 13/13 | 13/13 |
| W4 `poolrun` | 26 (8/18) | 26/26 | 26/26 | 26/26 | 26/26 | 26/26 |
| W5 `manifest` | 14 (10/4) | 14/14 | 14/14 | 14/14 | 14/14 | 14/14 |
| **Total** | **90 (46/44)** | **90/90** | **90/90** | **90/90** | **90/90** | **90/90** |

- **450/450** language-case combinations green; **230/230** scored combinations.
- Two full sweeps produced byte-identical JSONL records, including stdout and
  stderr SHA-256, exit codes and produced W5 CSV bytes.
- All 3,910 runtime samples re-passed the golden gate; no run was timed after a
  correctness failure.
- No orphan `dbshell`, `jsontool`, `netcheck`, `poolrun` or `manifest` process
  remained after the phases.
- Golden outputs and fixtures were not modified; `git diff HEAD --
  benchmarks/m4/expected` and `benchmarks/m4/fixtures` are empty.
- W5's Unicode-path probe remains **unscored** and unchanged: MINK reports
  `files=4 bytes=97 first=???.txt`, with `日本語.txt` looked up as `???` and a
  `size -1` row, while the other four languages report the correct 127 bytes.
  This is a documented MINK limitation, not a benchmark failure.

## 3. Per-workload evidence

Runtime and memory use the primary scored case named in each heading. Build is
the clean-build median; artifact is the shipped bytes (C# directory excludes
`.pdb`; Python is source only and has no build).

### W1 `jsontool` — primary case `sum_records`

| Language | Correct | Runtime ms (med/p95) | Startup ms (med) | Peak RSS MiB | Clean build ms | Artifact bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| MINK | 23/23 | 2.96 / 4.42 | 2.50 | 3.24 | 64.3 | 162,816 |
| Python | 23/23 | 25.38 / 27.40 | 25.04 | 11.34 | no build | 3,405 source |
| Rust | 23/23 | 5.02 / 7.50 | 3.65 | 4.46 | 2,488.9 | 211,456 |
| Go | 23/23 | 4.49 / 7.08 | 4.40 | 6.29 | 252.8 | 3,249,152 |
| C# | 23/23 | 38.26 / 50.68 | 34.50 | 23.58 | 770.3 | 171,417 dir |

All runtime medians are under 100 ms → **startup-dominated / not material**.

### W2 `netcheck` — primary case `ok`

| Language | Correct | Runtime ms (med/p95) | Startup ms (med) | Peak RSS MiB | Clean build ms | Artifact bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| MINK | 14/14 | 71.48 / 78.30 | 3.94 | 4.24 | 81.0 | 235,008 |
| Python | 14/14 | 94.87 / 111.57 | 51.75 | 17.98 | no build | 2,159 source |
| Rust | 14/14 | 49.56 / 53.12 | 4.07 | 5.23 | 5,542.3 | 675,328 |
| Go | 14/14 | 48.57 / 48.99 | 6.14 | 8.53 | 373.5 | 8,915,968 |
| C# | 14/14 | 136.82 / 153.35 | 39.30 | 31.76 | 784.3 | 171,929 dir |

MINK is 1.44× Rust and 1.47× Go on `ok`, and faster than Python and C#.
The `refused` case is **environment-constrained**: a closed loopback port takes
~2.03 s to fail on this host for MINK/Python/Rust/C#, while Go returns in
~10 ms; it is retained but excluded from comparative interpretation.

### W3 `dbshell` — primary case `baseline_100`

| Language | Correct | Runtime ms (med/p95) | Startup ms (med) | Peak RSS MiB | Clean build ms | Artifact bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| MINK | 13/13 | **270.40 / 277.68** | 2.52 | 4.75 | 80.5 | 198,144 |
| Python | 13/13 | 26.89 / 31.39 | 27.90 | 12.11 | no build | 3,514 source |
| Rust | 13/13 | 4.38 / 5.10 | 3.63 | 5.14 | 13,725.7 | 1,817,600 |
| Go | 13/13 | 7.21 / 8.43 | 7.55 | 9.12 | 389.5 | 10,019,840 |
| C# | 13/13 | 53.74 / 73.79 | 36.25 | 27.18 | 808.5 | 2,131,056 dir |

This is MINK's clearest runtime loss: 61.7× Rust, 37.5× Go, 10.1× Python and
5.0× C# for 114 SQL statements through the in-memory engine. The difference is
large, repeatable and retained. The other nine W3 cases are <100 ms in every
language and startup-dominated.

### W4 `poolrun` — primary case `squares_10000_8`

| Language | Correct | Runtime ms (med/p95) | Startup ms (med) | Peak RSS MiB | Clean build ms | Artifact bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| MINK | 26/26 | 3.28 / 4.43 | 2.65 | 3.21 | 58.5 | 112,640 |
| Python | 26/26 | 21.37 / 22.12 | 21.27 | 11.04 | no build | 4,071 source |
| Rust | 26/26 | 3.53 / 3.71 | 3.72 | 4.21 | 428.8 | 193,536 |
| Go | 26/26 | 5.93 / 6.64 | 4.71 | 6.02 | 221.8 | 2,503,680 |
| C# | 26/26 | 38.19 / 50.66 | 35.25 | 23.36 | 790.5 | 171,414 dir |

All medians are under 50 ms → **startup-dominated / not material**. The
concurrency models differ (OS threads, GIL, goroutines, .NET thread pool) and
are not equalized; only the functional contract is compared.

### W5 `manifest` — primary case `baseline_100`

| Language | Correct | Runtime ms (med/p95) | Startup ms (med) | Peak RSS MiB | Clean build ms | Artifact bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| MINK | 14/14 | 16.99 / 19.52 | 3.10 | 3.43 | 75.2 | 216,064 |
| Python | 14/14 | 37.05 / 37.95 | 31.67 | 13.87 | no build | 4,084 source |
| Rust | 14/14 | 10.30 / 10.93 | 3.46 | 4.34 | 2,544.9 | 238,080 |
| Go | 14/14 | 8.55 / 12.69 | 4.66 | 6.77 | 262.2 | 3,555,840 |
| C# | 14/14 | 45.59 / 60.77 | 35.98 | 24.91 | 778.5 | 174,489 dir |

All medians are under 50 ms → **startup-dominated / not material**. The
6.7–8.4 ms absolute gap to Rust/Go is below the panel's materiality framing.

## 4. Developer experience

**No active implementation time, attempt count or diagnostic count was
instrumented.** Historical Sessions 151–158 did not record timestamps, so this
report does **not** claim a measured productivity advantage for any language.
The observations below are qualitative and reproducible from the build setup.

| Language | Toolchain / setup | Manifest & lockfile | Build iterations observed |
| --- | --- | --- | --- |
| MINK | one `mink.exe`; bundled stdlib modules | none for these apps | one `mink build` command, 58–81 ms |
| Python | CPython 3.11.9 interpreter | none | no build step |
| Rust | cargo + MSVC C toolchain (required for W3 bundled SQLite) | `Cargo.toml` + committed `Cargo.lock`; 0–3 direct crates | cargo build, 0.43–13.7 s |
| Go | Go 1.26.7 | `go.mod` + `go.sum` only where needed (W3) | go build, 0.22–0.42 s |
| C# | .NET SDK 10.0.302 + NuGet restore; .NET 10 runtime at run time | `.csproj` + package reference for W3 | dotnet publish, 0.77–0.91 s |

Qualitative observations:

- MINK had the fewest moving parts: one compiler, no package manifest, no
  lockfile, no external C compiler, and a single native executable for every
  workload.
- Rust needed the most setup for W3 (cargo graph plus MSVC `cl.exe` for the
  bundled C engine) and had the longest clean build.
- Go's toolchain was simple, but W3's pure-Go SQLite driver brought 25 modules
  and a 10.0 MB artifact.
- C# needed an SDK, a NuGet package for W3 and a framework runtime on the
  target machine; its W3 package graph carries a visible `NU1903` advisory.
- Python had no build step but requires an interpreter and is the slowest to
  start among the interpreted options.

These are observations about setup, not measured effort.

## 5. Build and distribution

### Clean builds (ms, n = 5, median)

| Workload | MINK | Rust | Go | C# | Python |
| --- | ---: | ---: | ---: | ---: | --- |
| W1 | 64.3 | 2,488.9 | 252.8 | 770.3 | no build |
| W2 | 81.0 | 5,542.3 | 373.5 | 784.3 | no build |
| W3 | 80.5 | **13,725.7** | 389.5 | 808.5 | no build |
| W4 | 58.5 | 428.8 | 221.8 | 790.5 | no build |
| W5 | 75.2 | 2,544.9 | 262.2 | 778.5 | no build |

- MINK is the fastest builder by a wide margin (58–81 ms).
- Rust W3 is dominated by compiling bundled SQLite 3.46.0; the same build
  normally takes ~13.7 s.
- Go and C# are sub-second on these applications.
- Python correctly has no build measurement.

**Environment constraint / retained failure:** the first W3 Rust clean-build
protocol completed four builds, then the fifth failed with `cl.exe` exit
`0xC0000005` (access violation) while compiling `sqlite3.c`. The failed record
is preserved as
`results/raw/w3_sqlite/clean-build-w3_sqlite.failed-attempt.jsonl`; a direct
retry succeeded, and the complete five-build protocol was rerun with 5/5
success. This is recorded as an environmental compiler crash, not hidden or
silently replaced.

**Not measured:** artifact-hash reproducibility across builds. The five builds
are retained as timings, but no hash-equality claim is made.

### Artifact footprint

| Workload | MINK | Python | Rust | Go | C# (framework-dependent) |
| --- | ---: | ---: | ---: | ---: | ---: |
| W1 | 162,816 B | 3,405 B source | 211,456 B | 3,249,152 B | 171,417 B dir |
| W2 | 235,008 B | 2,159 B source | 675,328 B | 8,915,968 B | 171,929 B dir |
| W3 | 198,144 B | 3,514 B source | 1,817,600 B | 10,019,840 B | 2,131,056 B dir |
| W4 | 112,640 B | 4,071 B source | 193,536 B | 2,503,680 B | 171,414 B dir |
| W5 | 216,064 B | 4,084 B source | 238,080 B | 3,555,840 B | 174,489 B dir |

Distribution implications:

- MINK, Rust and Go produce native executables. MINK and Go are single files;
  Rust's W3 binary embeds SQLite.
- C# publishes framework-dependent: apphost + assembly + `.deps.json` +
  `.runtimeconfig.json` (and SQLite DLLs for W3), and needs the .NET 10 runtime.
- Python ships source and needs CPython 3.11.
- MINK has the smallest single-file executables in every workload; Go's are
  the largest. C#'s framework-dependent directory is smaller than MINK's
  executable in W2 and W5, but requires the .NET runtime to run. This is an
  artifact-size observation, not a startup or runtime verdict.

### Dependency burden

| Language | W1 | W2 | W3 | W4 | W5 |
| --- | --- | --- | --- | --- | --- |
| MINK | stdlib `json` | stdlib `http`,`json` | stdlib `sqlite` | stdlib `threads` | stdlib `json`,`filesystem`,`hashing`,`csv` |
| Python | stdlib | stdlib | stdlib `sqlite3` | stdlib `threading` | stdlib |
| Rust | `serde_json =1.0.151` | `serde_json`, `ureq =2.12.1` (no TLS) | `rusqlite =0.32.1` bundled SQLite 3.46.0 (24 lock packages) | none | `serde_json`, `sha2 =0.10.9` |
| Go | none | none | `modernc.org/sqlite v1.61.1` (25 modules) | none | none |
| C# | in-box | in-box | `Microsoft.Data.Sqlite 8.0.0` + `SQLitePCLRaw 2.1.6` | in-box | in-box |

The C# W3 chain still raises `NU1903` for
`SQLitePCLRaw.lib.e_sqlite3 2.1.6`; the warning was left visible, not
suppressed.

## 6. Runtime panel

The frozen rule: a difference is clear only when the median ratio is ≥ 1.10
**and** the two IQRs are disjoint. A workload whose medians are all below
100 ms is marked startup-dominated and is not used for a runtime claim.

| Workload | Status | What the evidence shows |
| --- | --- | --- |
| W1 | startup-dominated | MINK 2.6–4.0 ms; Rust 3.7–5.8 ms; Go 4.3–7.4 ms; Python ~25 ms; C# ~35–41 ms. No material runtime claim. |
| W2 | mixed, one environment caveat | On `ok`, MINK 71.5 ms vs Rust 49.6 / Go 48.6 / Python 94.9 / C# 136.8. On `chunked`, MINK 109.1 vs 90.9 / 92.0 / 143.0 / 158.5. `refused` is not comparable on this host. |
| W3 | **one clear loss** | `baseline_100`: MINK 270.4 ms vs Rust 4.4, Go 7.2, Python 26.9, C# 53.7. Other cases startup-dominated. |
| W4 | startup-dominated | All medians 2.6–12.6 ms; no material runtime claim. |
| W5 | startup-dominated | All medians 2.6–17.0 ms; MINK's baseline is 6.7–8.4 ms slower than Rust/Go in absolute terms. |

MINK has the lowest median in several startup-dominated cases (W1, W4, W5),
but those panels are not material. On the material W3 case it is the slowest,
and on W2 it sits between Rust/Go and Python/C#. These statements apply only
to the measured fixtures and sizes.

## 7. Resource panel

Startup medians by language across the five workloads:

| Language | Startup median range (ms) | p95 range (ms) | Peak RSS range (MiB, primary cases) |
| --- | ---: | ---: | ---: |
| MINK | 2.50–3.94 | 2.98–7.93 | 3.21–4.75 |
| Python | 21.27–51.75 | 23.40–55.00 | 11.04–17.98 |
| Rust | 3.46–4.07 | 4.32–6.43 | 4.21–5.23 |
| Go | 4.40–7.55 | 5.13–10.81 | 6.02–9.12 |
| C# | 34.50–39.30 | 36.85–56.32 | 23.36–31.76 |

Under the locked rules:

- MINK vs Rust startup is **no clear difference** in all five workloads
  (1.07–1.15 ms).
- MINK vs Go startup is clear in W2, W3 and W4 (2.05–5.03 ms) and no clear in
  W1/W5.
- MINK vs Python and C# startup is clear everywhere.
- MINK uses the least peak memory in every primary case. Rust is within 1 MiB
  in three of five; Go, Python and C# are clear differences.

**Method deviation:** peak memory comes from `GetProcessMemoryInfo`
(`PeakWorkingSetSize`) read through the child handle. The harness does not
implement the methodology's proposed Job Object primary method. Values are
retained as measured, with no baseline subtraction.

## 8. Answer to the M4 question

> Can MINK make a meaningful class of native software substantially simpler to
> build, test and ship than the alternatives?

**Qualified yes, for a defined class; no universal claim.**

Evidence for the qualified yes:

- All five workloads and 450/450 language-case combinations are correct, with
  deterministic repeated records and unchanged goldens.
- MINK ships the smallest single-file executables (113–235 KB), has the
  fewest dependencies (stdlib only for these workloads), and the fastest clean
  builds (58–81 ms, versus 0.22–0.42 s Go, 0.77–0.91 s C#, 0.43–13.7 s Rust).
- MINK startup shows no clear difference from Rust's on this host under the
  locked materiality rule, and is materially better than Python's and C#'s.
- MINK's peak memory (3.2–4.8 MiB) is lower than all four alternatives in the
  primary cases.
- The single-binary distribution story is real: no runtime, package manager or
  sidecar files are needed to run the measured MINK applications on Windows.

Evidence against a broad claim:

- W3's in-memory SQLite interpreter is 5–62× slower than the alternatives on
  the 114-statement baseline. That is a real, repeatable loss.
- W1, W4 and W5 are startup-dominated; they do not establish a runtime
  advantage.
- MINK is not fastest in any live runtime case; W2 places it mid-pack.
- The measured code stays inside MINK's supported envelope: Windows x86_64,
  ANSI paths, a JSON arena of a few KiB, an in-memory SQL subset, and an HTTP
  body budget. The W5 Unicode probe shows the filesystem limitation directly.
- No developer-effort time was measured, so MINK cannot claim a measured
  productivity advantage.

The defensible conclusion is: **for small, single-binary Windows CLI tools
with ASCII paths and modest data sizes, MINK is a credible, genuinely simpler
build-and-ship option, with correctness and resource usage at least
competitive. It is not a general replacement for Rust, Python, Go or C#, and
it is substantially slower for the SQL workload measured here.**

## 9. Limitations and constraints

**Documented MINK limitations**

- Windows x86_64 only; Linux/macOS are out of scope.
- ANSI filesystem and argv; the W5 Unicode probe is unscored and shows
  `日本語.txt` collapsing to `???.txt` with `size -1` and an empty-string hash.
- W3 covers an in-memory common-SQL subset, not SQLite file compatibility,
  durability, indexes, joins or advanced SQL. The `~220` four-column-row
  default-heap estimate remains an unverified envelope note; the 500-row probe
  passes in this benchmark.
- JSON arena: roughly 3.5 KiB input / ~2 KiB serialization; W1 stays below it.
- `MAX_LIVE_ALLOCS = 1024`, HTTP 4 MiB body budget, HTTP without TLS, and no
  integrated debugger or profiler.

**Environmental constraints**

- W2 `refused`: every closed loopback port takes ~2.03 s for a plain Python
  socket connect on this host; Go returns in ~10 ms. The case is retained but
  excluded from comparison.
- The first W3 Rust clean-build protocol hit a transient `cl.exe` access
  violation on its fifth build; the failure is preserved and the rerun was
  5/5 green.
- One machine, one run period, no CPU pinning, power-plan change, Defender
  exclusion or cache dropping; measurements are warm-process, not cold-cache.
- Shared compiler and package caches were retained for clean builds. The
  harness does not separate dependency restore from compilation; cargo ran
  `--offline`, Go modules were already cached, and .NET packages were already
  in the NuGet cache, but network traffic was not instrumented.

**Not measured**

- Active developer time, iteration count, diagnostics count or modification
  time. No LOC/token productivity score is claimed.
- Artifact-hash reproducibility across clean builds.
- Cold-cache performance, long-duration soak, leak profiling, or external
  developer validation.

**Measurement/implementation choices**

- Python is CPython 3.11.9 (M1 continuity), not 3.14.
- C# is framework-dependent .NET 10; self-contained publishing is not the
  primary comparison.
- Runtime panels for W1/W4/W5 are startup-dominated; small absolute
  differences must not be read as language rankings.
- C# W3's `NU1903` advisory is visible; the package was not upgraded during M4.

## 10. Evidence index

| Evidence | Location |
| --- | --- |
| Correctness records | `benchmarks/m4/results/raw/<w>/correctness-<w>.jsonl` |
| Runtime records | `benchmarks/m4/results/raw/<w>/runtime-<w>.jsonl` |
| Startup records | `benchmarks/m4/results/raw/<w>/startup-<w>.jsonl` |
| Clean-build records | `benchmarks/m4/results/raw/<w>/clean-build-<w>.jsonl` |
| Retained W3 build failure | `benchmarks/m4/results/raw/w3_sqlite/clean-build-w3_sqlite.failed-attempt.jsonl` |
| Unicode probe | `benchmarks/m4/results/raw/w5_manifest/unicode-probe.json` |
| Committed statistics | [`M4_RESULTS_SUMMARY.json`](M4_RESULTS_SUMMARY.json) |
| Sessions | [158](M4_SESSION_158.md), [159](M4_SESSION_159.md), [160](M4_SESSION_160.md), [161](M4_SESSION_161.md) |

No performance number was invented or estimated. No raw sample was deleted.
No composite score was computed.
