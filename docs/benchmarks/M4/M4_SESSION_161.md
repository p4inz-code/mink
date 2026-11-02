# Session 161 — Startup and resource measurements

**Date:** 2026-11-02
**Status:** **COMPLETE — 750 startup samples and peak-RSS capture, all classes
retained.**
**Rule:** startup, memory and runtime are separate panels; no composite score.

## 1. Startup protocol

The existing `measure.py --startup` driver was used unchanged: 30 runs of the
no-argument invocation per language per workload (the applications print their
usage and exit), shuffled language order with seed **20261101**, raw JSONL
appended, one process at a time.

| Workload | Records |
| --- | ---: |
| W1 | 150 (5 × 30) |
| W2 | 150 |
| W3 | 150 |
| W4 | 150 |
| W5 | 150 |
| **Total** | **750** |

Startup correctness is recorded as `not-applicable`: these invocations are the
usage path, not a scored golden case. Exit codes are retained in every record.

## 2. Startup results (ms)

Medians (p95 in parentheses), n = 30 per cell:

| Workload | MINK | Python | Rust | Go | C# |
| --- | ---: | ---: | ---: | ---: | ---: |
| W1 | **2.50** (2.98) | 25.04 (27.11) | 3.65 (4.32) | 4.40 (5.13) | 34.50 (36.85) |
| W2 | **3.94** (5.76) | 51.75 (55.00) | 4.07 (4.69) | 6.14 (6.66) | 39.30 (45.66) |
| W3 | **2.52** (4.24) | 27.90 (29.75) | 3.63 (5.10) | 7.55 (10.81) | 36.25 (52.78) |
| W4 | **2.65** (7.93) | 21.27 (23.40) | 3.72 (6.43) | 4.71 (10.60) | 35.25 (56.32) |
| W5 | **3.10** (4.87) | 31.67 (34.08) | 3.46 (4.90) | 4.66 (5.97) | 35.98 (50.46) |

Full min/max/IQR values are in
[`M4_RESULTS_SUMMARY.json`](M4_RESULTS_SUMMARY.json).

### Materiality vs MINK

Using the locked rule (≥ 2 ms **and** ≥ 10 %):

| Comparison | W1 | W2 | W3 | W4 | W5 |
| --- | --- | --- | --- | --- | --- |
| MINK vs Python | clear | clear | clear | clear | clear |
| MINK vs Rust | no clear (1.15 ms) | no clear (0.12 ms) | no clear (1.12 ms) | no clear (1.07 ms) | no clear (0.36 ms) |
| MINK vs Go | no clear (1.90 ms) | **clear (2.20 ms)** | **clear (5.03 ms)** | **clear (2.05 ms)** | no clear (1.56 ms) |
| MINK vs C# | clear | clear | clear | clear | clear |

Interpretation is limited to this machine and these 30-run samples.

## 3. Memory / resource results

Peak working set for the primary scored case of each workload, in MiB
(median, n = 15; p95 in parentheses):

| Workload / case | MINK | Python | Rust | Go | C# |
| --- | ---: | ---: | ---: | ---: | ---: |
| W1 `sum_records` | **3.24** (3.24) | 11.34 (11.45) | 4.46 (4.46) | 6.29 (6.43) | 23.58 (23.73) |
| W2 `ok` | **4.24** (4.25) | 17.98 (18.06) | 5.23 (5.24) | 8.53 (8.64) | 31.76 (31.95) |
| W3 `baseline_100` | **4.75** (4.75) | 12.11 (12.16) | 5.14 (5.15) | 9.12 (9.24) | 27.18 (27.29) |
| W4 `squares_10000_8` | **3.21** (3.21) | 11.04 (11.08) | 4.21 (4.22) | 6.02 (6.09) | 23.36 (23.56) |
| W5 `baseline_100` | **3.43** (3.43) | 13.87 (13.94) | 4.34 (4.36) | 6.77 (6.88) | 24.91 (25.06) |

Materiality vs MINK (≥ 1 MiB **and** ≥ 10 %):

| Comparison | W1 | W2 | W3 | W4 | W5 |
| --- | --- | --- | --- | --- | --- |
| MINK vs Python | clear | clear | clear | clear | clear |
| MINK vs Rust | **clear (1.22 MiB)** | no clear (0.98 MiB) | no clear (0.40 MiB) | **clear (1.00 MiB)** | no clear (0.90 MiB) |
| MINK vs Go | clear | clear | clear | clear | clear |
| MINK vs C# | clear | clear | clear | clear | clear |

## 4. Measurement method and hygiene

- Values come from the harness's `_sample_process`. On Windows it reads
  `GetProcessTimes` for `cpu_ms` and `GetProcessMemoryInfo`
  (`PeakWorkingSetSize`) for `peak_rss_bytes` through the child handle Python
  holds. It records `null` if unavailable; no value is guessed.
- **Deviation from the methodology text:** the harness does not implement the
  proposed Job Object primary method; it uses `GetProcessMemoryInfo` peak
  working set only. This is recorded as a limitation, not presented as the
  Job Object method.
- Each run is a fresh process; startup runs are sequential; no previous
  process is reused.
- After the phases, a `tasklist` check found **no** `dbshell`, `jsontool`,
  `netcheck`, `poolrun` or `manifest` orphan process.
- W2's fixture server runs inside the harness process and is stopped after
  each case; no listener process remains.

## 5. Raw evidence

```text
benchmarks/m4/results/raw/<w>/startup-<w>.jsonl    # 750 records
benchmarks/m4/results/reports/startup-summary.json
benchmarks/m4/results/reports/memory-summary.json
docs/benchmarks/M4/M4_RESULTS_SUMMARY.json
```

No fixture, application, expected output or MINK file was modified.

## 6. Limitations

- Peak working set is an OS counter read after exit; it is not a sampled RSS
  curve and does not separate managed/native heaps.
- Python and C# include interpreter/runtime memory in the primary figure; no
  empty-program baseline subtraction is presented.
- W2 memory was measured for the `ok` response only; the refused case is
  environment-constrained and is not used as a memory comparison.
