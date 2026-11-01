# Session 157 — M4 correctness and measurement-readiness audit

**Date:** 2026-11-01
**Status:** audit complete. **M4 is NOT READY FOR MEASUREMENT.**
**No performance measurement was taken.** A single one-process plumbing launch
of the timing sampler is described in §6 and is not a benchmark sample.

## 1. Complete correctness matrix

Codes: **G** = green (builds, every case byte-exact), **B** = blocked
(implementation written, dependency absent, never built here).

| Workload | Cases (scored/support) | MINK | Python | Rust | Go | C# |
| --- | --- | --- | --- | --- | --- | --- |
| W1 `jsontool` | 23 (10/13) | G 23/23 | G 23/23 | G 23/23 | G 23/23 | G 23/23 |
| W2 `netcheck` | 14 (8/6) | G 14/14 | G 14/14 | G 14/14 | G 14/14 | G 14/14 |
| W3 `dbshell` | 13 (10/3) | G 13/13 | G 13/13 | **B 0/13** | **B 0/13** | G 13/13 |
| W4 `poolrun` | 26 (8/18) | G 26/26 | G 26/26 | G 26/26 | G 26/26 | G 26/26 |
| W5 `manifest` | 14 (10/4) | G 14/14 | G 14/14 | G 14/14 | G 14/14 | G 14/14 |
| **Total** | **90 (46/44)** | 90/90 | 90/90 | 77/90 | 77/90 | 90/90 |

Language-case combinations: 450 total, **424 green**, **26 blocked**
(W3 × Rust, W3 × Go). Scored combinations: 230 total, 210 green, 20 blocked.

Supporting evidence beyond the gate:

- W5 Unicode-path probe: capability evidence, not scored (Session 156 §4).
- W1 boundary documents (4 KiB / 5 KiB) remain unscored probes.
- W3 `probe_500` passes in the three available implementations; the earlier
  "failure" expectation is corrected, not hidden (Session 154 §1.2).

## 2. Repeatability

Two complete sweeps of all five workloads were run back-to-back:

```text
python benchmarks/m4/harness/validate.py --workload <w> --quiet   # x2, all five
diff -r results/raw  …  # first sweep vs second sweep
```

Result: **byte-identical** across all raw JSONL records, including
stdout/stderr SHA-256 values, exit codes, problem lists and the W3 blocked
records. `diff` reported no differences. No record contains a timestamp, so
this equality is meaningful rather than incidental.

Gate status per sweep: W1 0, W2 0, W3 **1** (26 blocked combinations), W4 0,
W5 0. The W3 exit 1 is the correct, intentional signal — it is not a harness
failure and it is not collapsed into "failed correctness".

## 3. Build readiness

| Language | Version verified | Build command (harness) | Artifact | Network |
| --- | --- | --- | --- | --- |
| MINK | 1.0.4 | `target/release/mink.exe build <.build/…/name.mink>` | `.build/<w>/mink/<name>.exe` | none |
| Python | CPython 3.11.9 x64 | none | source `apps/python/<w>/<name>.py` | none at run time; interpreter required |
| Rust | rustc 1.97.1, msvc | `cargo build --release --offline --target-dir …` | `.build/<w>/rust/release/<name>.exe` | none for W1/W2/W4/W5 (cached crates) |
| Go | go 1.26.7 windows/amd64 | `go build -trimpath -buildvcs=false -o … .` | `.build/<w>/go/<name>.exe` | none (no module requirements) |
| C# | .NET SDK 10.0.302, `net10.0` | `dotnet publish -c Release -r win-x64 --self-contained false` | `.build/<w>/csharp/<name>.exe` + DLLs | none (NuGet cache only) |

Notes that matter for later distribution claims:

- Rust builds with `RUSTFLAGS=""` because the repository's `.cargo/config.toml`
  forces `+crt-static`; the M4 baseline was captured with the dynamic CRT, so
  the benchmark restores it explicitly instead of silently changing the
  measured artifact.
- C# publishes **framework-dependent**; .NET 10 must be installed to run it.
  Python likewise needs an interpreter. MINK, Rust and Go artifacts are
  self-contained executables.
- No clean-build timing has been done. `measure.py --clean-build` implements
  the 5-build protocol and deletes only `benchmarks/m4/.build/…`.

## 4. Dependency state

| Language | W1 | W2 | W3 | W4 | W5 |
| --- | --- | --- | --- | --- | --- |
| MINK | stdlib `json` | stdlib `http`,`json` | stdlib `sqlite` | stdlib `threads` | stdlib `json`,`filesystem`,`hashing`,`csv` |
| Python | stdlib | stdlib | stdlib `sqlite3` | stdlib `threading` | stdlib |
| Rust | `serde_json =1.0.151` | `ureq =2.12.1`, `serde_json =1.0.151` | `rusqlite 0.32` **absent** | none | `serde_json =1.0.151`, `sha2 =0.10.9` |
| Go | none | none | `modernc.org/sqlite` **absent** | none | none |
| C# | in-box | in-box | `Microsoft.Data.Sqlite 8.0.0` + `SQLitePCLRaw 2.1.6` | in-box | in-box |

Recorded deviations:

- Rust W1 uses `serde_json`'s `preserve_order` feature; the default map is
  sorted and would break the compact-output contract.
- Rust W2 disables `ureq` default features (no TLS); W2 is HTTP only.
- Rust W5 does **not** use the `csv` crate (the plan listed it); the writer is
  a small RFC-4180 implementation. C# is manual too. Recorded in Session 156.
- C# W3 pins `Microsoft.Data.Sqlite 8.0.0` because it is the only cached
  version; its `SQLitePCLRaw.lib.e_sqlite3 2.1.6` raises NU1903 (high
  severity). Left visible, not suppressed.

## 5. Fairness audit

| Check | Finding |
| --- | --- |
| MINK tested outside its supported contract? | No. W3 stays in the documented SQL subset; W5 scored paths are ASCII; Unicode is a separate probe; W1 stays under the JSON arena bound. |
| Competitor special advantage? | No intentional one. Differences that exist are recorded: Python's GIL, goroutines vs OS threads, .NET thread pool, framework-dependent C#/.NET and interpreter-dependent Python artifacts. |
| Dependencies hidden? | No. Full table above; version pins, transitive chains and the NU1903 advisory are visible. |
| Runtime requirements hidden? | No. Python and C# runtime requirements are stated in §3; MINK/Rust/Go produce standalone executables. |
| Source size treated as productivity? | No. No LOC, token or line-diff metric is computed or claimed anywhere in M4. |
| Concurrency model difference documented? | Yes, Session 155 §2 and §8. |
| MINK limitations visible? | Yes: ANSI filesystem (W5 probe), no persistence (W3), JSON arena bound (W1), HTTP only (W2). |
| Impossible comparisons excluded? | Yes: SQLite compatibility/durability, TLS, UDP/TCP raw, subprocess, regex, CSV standalone, filesystem mutation. |
| Outputs weakened to accommodate one language? | No. Goldens are byte-exact and unchanged; blocked languages are reported blocked, never skipped-and-counted. |
| Zero-dependency workloads mislabeled? | No. W4's "no dependencies" means stdlib only in every language, not "MINK needs nothing while others need packages". |

## 6. Measurement harness audit

Components in `benchmarks/m4/harness/`:

| File | Role | State |
| --- | --- | --- |
| `common.py` | paths, workload registry, argv substitution, run commands, work dirs | ready |
| `build.py` | documented build for each language/workload | ready; Rust offline, C# local cache |
| `run.py` | execute one case, prepare W5 work dir, compare stdout/stderr/exit/artifacts | ready |
| `validate.py` | full correctness gate, JSONL records, exit status | ready |
| `server.py` | deterministic local HTTP/1.1 fixture server with deliberate fragmentation | ready (W2) |
| `pe_imports.py` | PE import-table reader (no objdump/dumpbin on host) | ready |
| `probe_unicode.py` | W5 Unicode capability probe, non-gating | ready |
| `stats.py` | median/p95/min/max/IQR and the three materiality rules | **added this session**; selftest 17/17 |
| `measure.py` | protocol driver (runtime, startup, clean build), gate precondition, raw JSONL | **added this session**; not executed for W3 and not executed as a benchmark |

Protocol confirmation (all implemented, none run for evidence):

- runtime: 2 warm-ups + 15 measured runs per case/language; language order
  shuffled with a recorded seed; every measured run re-checked against the
  golden exit code and stdout hash; a mismatch aborts instead of recording.
- startup: 30 runs of the no-argument invocation (artifact prints usage and
  exits), per language.
- clean build: 5 builds per language/workload into a benchmark-owned
  directory.
- statistics: median, p95, min, max, IQR from raw samples; no outlier
  deletion; no mean-of-percentages; no composite score.
- materiality implemented exactly: runtime ratio < 1.10 **or** overlapping
  IQR ⇒ no clear difference; startup < 2 ms **or** < 10 % ⇒ no clear;
  memory < 1 MiB **or** < 10 % ⇒ no clear.
- raw output: `results/raw/<workload>/runtime-*.jsonl`, `startup-*.jsonl`,
  `clean-build-*.jsonl`, alongside `correctness-*.jsonl` and
  `unicode-probe.json`.
- precondition: `measure.py` refuses to time a workload unless the recorded
  correctness gate is green for every requested case/language (verified: W3
  with no records returns exit 2 and lists the missing combinations).
- `--plan` prints the protocol for all five workloads without launching
  anything.

Plumbing check, explicitly **not** a benchmark result: one launch of
`python -c pass` through the sampler returned exit 0, wall 20.74 ms,
CPU 15.625 ms, peak working set 10,190,848 bytes. This verifies that child CPU
time (`GetProcessTimes`) and peak memory (`GetProcessMemoryInfo`) are read
correctly on this host; it is not stored, not compared and not quoted as a
measurement. The sampler uses CPython's Windows process handle
(`Popen._handle`) and records `null` rather than guessing if it is absent.

## 7. Readiness checklist

| # | Condition | Result |
| --- | --- | --- |
| 1 | W1 correctness green | **PASS** (115/115) |
| 2 | W2 correctness green | **PASS** (70/70) |
| 3 | W3 correctness green | **FAIL** (39/65; Rust and Go blocked) |
| 4 | W4 correctness green | **PASS** (130/130) |
| 5 | W5 correctness green | **PASS** (70/70) |
| 6 | All fixtures deterministic | **PASS** (generators assert counts; two sweeps identical) |
| 7 | All golden outputs validated | **PASS** for the four green workloads; W3 goldens validated in 3/5 |
| 8 | All dependencies pinned | **FAIL** — `rusqlite` and `modernc.org/sqlite` are declared but unresolved locally |
| 9 | All build procedures documented | **PASS** |
| 10 | All environment versions pinned | **PASS** (MINK 1.0.4, CPython 3.11.9, rustc 1.97.1, Go 1.26.7, .NET SDK 10.0.302) |
| 11 | No known correctness blocker | **FAIL** — 26 W3 combinations cannot build |
| 12 | Fairness limitations documented | **PASS** |
| 13 | Measurement harness ready | **PASS** (built this session; `--plan` and gate verified) |
| 14 | Raw-result format defined | **PASS** (`results/raw/<workload>/*.jsonl`) |
| 15 | No MINK source modification required | **PASS** — no compiler/runtime/stdlib file was touched in Sessions 154–157 |

## 8. Measurement protocol for the next session

Only after W3 is green in all five languages:

1. Restore the two drivers with authorization, record exact resolved versions
   and licenses, build W3, and run the W3 gate twice.
2. Re-run every gate once more; require 450/450 green and identical records.
3. `python benchmarks/m4/harness/measure.py --workload <w> --runtime` for each
   workload; then `--startup`; then `--clean-build`.
4. Summarize with `stats.py` only; report per-case medians and IQRs, never a
   single score, never a ranking without the materiality rule applied.
5. Keep raw JSONL untouched; publish failures and blocked paths as they are.

## 9. Verdict

**M4 NOT READY.**

The only blocker is W3's two missing SQLite drivers. Everything else — W1,
W2, W4, W5, fixtures, goldens, repeatability, harness, fairness documentation
— satisfies the checklist. No measurement was taken, and no language is
claimed to be faster, smaller, easier or better.
