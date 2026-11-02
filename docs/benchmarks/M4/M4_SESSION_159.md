# Session 159 — Final correctness sweep and measurement preflight

**Date:** 2026-11-02
**Status:** **COMPLETE — all 450 language-case combinations GREEN; measurement
preconditions verified. No measurement was run in this session.**

## 1. Final correctness matrix

Two complete sweeps of W1–W5 were run back-to-back with the existing harness:

```text
python benchmarks/m4/harness/validate.py --workload <w> --quiet
```

| Workload | Cases (scored/support) | MINK | Python | Rust | Go | C# | Combinations |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W1 `jsontool` | 23 (10/13) | 23/23 | 23/23 | 23/23 | 23/23 | 23/23 | 115/115 |
| W2 `netcheck` | 14 (8/6) | 14/14 | 14/14 | 14/14 | 14/14 | 14/14 | 70/70 |
| W3 `dbshell` | 13 (10/3) | 13/13 | 13/13 | 13/13 | 13/13 | 13/13 | 65/65 |
| W4 `poolrun` | 26 (8/18) | 26/26 | 26/26 | 26/26 | 26/26 | 26/26 | 130/130 |
| W5 `manifest` | 14 (10/4) | 14/14 | 14/14 | 14/14 | 14/14 | 14/14 | 70/70 |
| **Total** | **90 (46/44)** | **90/90** | **90/90** | **90/90** | **90/90** | **90/90** | **450/450** |

Gate exits were 0 for all five workloads. Scored combinations are 230/230;
supporting combinations are 220/220.

## 2. Determinism

For every workload the second sweep's
`results/raw/<workload>/correctness-<workload>.jsonl` was compared with the
first sweep's copy:

```text
w1_json: IDENTICAL
w2_http: IDENTICAL
w3_sqlite: IDENTICAL
w4_pool: IDENTICAL
w5_manifest: IDENTICAL
```

All 450 records are byte-identical, including stdout/stderr SHA-256, exit
codes, problem lists and the W5 report bytes implied by the cases. No record
contains a timestamp, so this equality is substantive.

## 3. Pre-measurement audit

| # | Check | Result |
| --- | --- | --- |
| 1 | Golden outputs unchanged | **PASS** — `git diff HEAD -- benchmarks/m4/expected` is empty |
| 2 | Raw correctness records deterministic | **PASS** — two sweeps byte-identical (§2) |
| 3 | Fixture bytes unchanged | **PASS** — `git status --porcelain benchmarks/m4/fixtures` is empty |
| 4 | `.gitattributes` preserves required line endings | **PASS** — `* text=auto`, explicit `eol=lf` for source/MINK/Markdown, and `*.pem binary` for byte-sensitive fixtures; W3 CRLF fixture is unchanged |
| 5 | Dependency versions pinned/resolved | **PASS** — every Rust app has `Cargo.lock`; W3 pins `rusqlite =0.32.1`; Go W3 has `go.sum`/`modernc.org/sqlite v1.61.1`; C# pins `Microsoft.Data.Sqlite 8.0.0`; other Go modules and the Rust std-only apps have no requirements |
| 6 | All applications build | **PASS** — `build.py` returned exit 0 for 25/25 language-workload builds |
| 7 | No benchmark source modifies MINK | **PASS** — no compiler/runtime/stdlib file was touched; pre-existing M3 modifications remain as they were |
| 8 | No network needed at benchmark runtime | **PASS** — W2 is local loopback; W1/W3/W4/W5 use local files/stdlib only |
| 9 | W3 remains the in-memory SQLite comparison | **PASS** — all five use an in-memory database; no packags or persistence |
| 10 | W5 Unicode probe remains unscored | **PASS** — separate `probe_unicode.py` evidence, not a case and not a gate |

Environment pin confirmed unchanged from the baseline:

```text
mink 1.0.4
4,028,416 bytes
d8f6134850e2af1a7cd4f38a7e8a80d7692d566eac1bae97ddc28ed827149f09
Python 3.11.9 · rustc 1.97.1 · go1.26.7 · .NET SDK 10.0.302
```

## 4. Measurement plan preflight

`python benchmarks/m4/harness/measure.py --workload <w> --plan` was run for
all five workloads. Each exited 0, listed the ten/eight scored cases, the
2 warm-up + 15 measured-run protocol, the 30-run startup protocol, the
5-build clean-build protocol, the recorded seed (20261101), the raw JSONL
location and the materiality thresholds, and printed `nothing was launched`.

The gate precondition was also confirmed by the code path: `measure.py`
re-reads `correctness-<workload>.jsonl` and refuses to time a workload unless
every requested case/language is `PASS`. That file is now complete and green
for all five workloads.

## 5. Measurement preconditions still outside the harness

Recorded, not hidden:

- No machine-level pinning (CPU affinity, power plan, Defender exclusion) is
  applied. The protocol measures warm-process behaviour on this workstation.
- Clean-build timing will use retained shared compiler/NuGet/cargo caches;
  dependency download time is not part of the build measurement.
- W2 runtime runs start a local fixture server per language/case; server setup
  is outside the child process wall time, as implemented.
- Python is measured through CPython 3.11.9 and C# through framework-dependent
  .NET 10, exactly as recorded in the environment baseline.

## 6. Verdict

Correctness is fully green: **450/450 language-case combinations**, with
byte-identical repeated records. Every preflight condition that can be checked
without running a benchmark is satisfied.

**Ready for Session 160 runtime measurements.** No runtime, startup, memory or
clean-build sample has been taken yet; no performance number exists and no
ranking is claimed.
