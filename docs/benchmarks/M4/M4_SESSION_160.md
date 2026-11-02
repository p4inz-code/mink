# Session 160 — Runtime measurements

**Date:** 2026-11-02
**Status:** **COMPLETE — 3,910 runtime samples retained, every measured run
passed its correctness gate.**
**Rule:** no outlier was deleted, no value was averaged by hand, and no
composite score or language ranking was produced.

## 1. Protocol executed

The existing `measure.py --runtime` driver was used unchanged:

- 2 warm-up runs and **15 measured runs** per scored case and language;
- language order shuffled per case with the recorded seed **20261101**;
- each measured run re-compared against the golden exit code and stdout hash;
  a mismatch aborts the phase instead of recording a sample;
- raw JSONL appended to `benchmarks/m4/results/raw/<workload>/runtime-*.jsonl`;
- the correctness gate was checked before timing (all 450 combinations green).

| Workload | Scored cases | Samples (17 × cases × 5) | Correctness |
| --- | --- | --- | --- |
| W1 `jsontool` | 10 | 850 | 850 PASS |
| W2 `netcheck` | 8 | 680 | 680 PASS |
| W3 `dbshell` | 10 | 850 | 850 PASS |
| W4 `poolrun` | 8 | 680 | 680 PASS |
| W5 `manifest` | 10 | 850 | 850 PASS |
| **Total** | **46** | **3,910** | **3,910 PASS** |

Statistics use `stats.summarize` (median, p95, min, max, Q1/Q3, IQR) over the
15 measured samples only. Full per-case values are in
[`M4_RESULTS_SUMMARY.json`](M4_RESULTS_SUMMARY.json). No sample was removed.

## 2. Runtime medians (ms)

Cells are medians over 15 measured runs. The two warm-ups are excluded from
statistics but retained in the raw JSONL.

### W1 `jsontool`

| Case | MINK | Python | Rust | Go | C# |
| --- | ---: | ---: | ---: | ---: | ---: |
| validate_object | 2.76 | 24.83 | 3.88 | 4.65 | 38.13 |
| type_object | 2.56 | 25.26 | 3.81 | 4.43 | 38.22 |
| type_array | 4.04 | 25.81 | 5.51 | 7.42 | 41.18 |
| type_scalar_number | 2.62 | 26.36 | 5.78 | 6.28 | 36.40 |
| sum_records | 2.96 | 25.38 | 5.02 | 4.49 | 38.26 |
| compact_spaced | 3.98 | 25.58 | 3.67 | 4.55 | 41.49 |
| validate_broken | 2.55 | 25.21 | 3.89 | 4.36 | 39.10 |
| validate_empty | 3.29 | 24.76 | 5.03 | 4.34 | 36.70 |
| validate_missing_file | 2.55 | 25.71 | 3.55 | 4.57 | 35.70 |
| validate_directory | 2.62 | 24.95 | 4.96 | 4.39 | 35.44 |

Every median is below 100 ms, so per the pre-registered rule W1's runtime
panel is **startup-dominated / not material**; no runtime claim is made.

### W2 `netcheck`

| Case | MINK | Python | Rust | Go | C# |
| --- | ---: | ---: | ---: | ---: | ---: |
| ok | 71.48 | 94.87 | 49.56 | 48.57 | 136.82 |
| chunked | 109.13 | 142.97 | 90.89 | 91.97 | 158.48 |
| notfound | 46.02 | 75.11 | 26.91 | 30.52 | 94.48 |
| badjson | 46.53 | 75.27 | 29.33 | 28.65 | 93.11 |
| peer_close | 4.19 | 53.97 | 4.94 | 10.12 | 75.60 |
| refused | 2031.35 | 2093.67 | 2053.02 | 10.48 | 2115.48 |
| no_status_line | 25.44 | 52.28 | 4.77 | 6.74 | 68.57 |
| usage_too_few_args | 2.38 | 47.91 | 3.71 | 5.79 | 35.07 |

`refused` is **environment-constrained, not a language result**: a plain
Python `socket.create_connection` to six different closed loopback ports took
2,016–2,052 ms on this host, while Go's dialer returned in ~10 ms. The ~2 s
dominates that case for MINK, Python, Rust and C#. It is reported and retained
but excluded from any comparative interpretation.

For the completed-response cases (`ok`, `chunked`, `notfound`, `badjson`,
`no_status_line`), MINK is slower than Rust/Go and faster than Python/C#. On
the immediate-failure paths (`peer_close`, usage) MINK is at the low end. The
absolute differences are tens of milliseconds; the largest material case is
`ok` (MINK 71.5 ms vs Go 48.6 ms, ratio 1.47, disjoint IQRs).
W2 is not uniformly startup-dominated, so these are reported under the locked
rule, without a composite verdict.

### W3 `dbshell`

| Case | MINK | Python | Rust | Go | C# |
| --- | ---: | ---: | ---: | ---: | ---: |
| baseline_100 | **270.40** | 26.89 | 4.38 | 7.21 | 53.74 |
| clean | 5.47 | 30.29 | 5.90 | 9.56 | 59.83 |
| comments | 4.18 | 29.53 | 6.79 | 6.71 | 60.16 |
| crlf | 5.69 | 29.61 | 6.41 | 8.21 | 57.78 |
| empty_script | 2.63 | 26.75 | 3.74 | 7.06 | 46.36 |
| missing_file | 4.06 | 26.52 | 3.43 | 5.82 | 35.34 |
| directory_as_script | 2.58 | 28.00 | 4.42 | 6.21 | 35.08 |
| no_args | 2.97 | 27.03 | 3.35 | 6.48 | 34.51 |
| two_positionals | 3.06 | 27.89 | 5.04 | 6.17 | 36.02 |
| error_stops | 4.79 | 29.29 | 5.41 | 10.66 | 60.26 |

`baseline_100` is the only scored W3 case above 100 ms in any language and the
one clear, large runtime difference in M4: MINK's in-memory SQLite interpreter
takes 270.4 ms (median; IQR 268.7–274.2) for 114 statements, versus Rust 4.4 ms
(61.7×), Go 7.2 ms (37.5×), Python 26.9 ms (10.1×) and C# 53.7 ms (5.0×).
The other nine W3 cases are startup-dominated.

### W4 `poolrun`

| Case | MINK | Python | Rust | Go | C# |
| --- | ---: | ---: | ---: | ---: | ---: |
| squares_100_4 | 2.64 | 21.01 | 3.46 | 4.30 | 35.67 |
| squares_1024_16 | 2.78 | 22.90 | 3.60 | 4.34 | 34.77 |
| squares_10000_8 | 3.28 | 21.37 | 3.53 | 5.93 | 38.19 |
| squares_10000_16 | 12.64 | 26.45 | 5.59 | 5.80 | 44.91 |
| pipeline_100_4 | 4.77 | 27.17 | 4.61 | 7.07 | 44.68 |
| pipeline_1000_3 | 5.56 | 23.85 | 7.22 | 7.30 | 46.53 |
| pipeline_999_7 | 9.59 | 26.20 | 5.13 | 5.79 | 48.38 |
| pipeline_10000_8 | 3.00 | 25.45 | 3.81 | 4.37 | 39.09 |

All medians are below 50 ms → **startup-dominated / not material**. The
concurrency-model differences documented in Session 155 remain; they are not
turned into a runtime verdict here.

### W5 `manifest`

| Case | MINK | Python | Rust | Go | C# |
| --- | ---: | ---: | ---: | ---: | ---: |
| baseline_100 | 16.99 | 37.05 | 10.30 | 8.55 | 45.59 |
| uppercase_ext | 11.68 | 35.90 | 10.30 | 6.68 | 45.03 |
| no_match | 7.70 | 33.50 | 7.69 | 5.23 | 40.47 |
| missing_root | 3.94 | 32.94 | 5.27 | 4.66 | 39.70 |
| bad_json | 2.63 | 31.39 | 5.00 | 5.70 | 39.66 |
| missing_keys | 3.94 | 31.94 | 3.48 | 4.77 | 39.65 |
| bad_types | 3.85 | 31.65 | 4.93 | 4.61 | 40.97 |
| missing_config | 2.68 | 31.61 | 4.92 | 4.55 | 34.57 |
| directory_as_config | 2.98 | 30.45 | 3.36 | 4.54 | 34.46 |
| no_args | 3.82 | 32.66 | 4.66 | 6.54 | 37.62 |

All medians are below 50 ms → **startup-dominated / not material**. MINK's
`baseline_100` is 1.65× Rust and 1.99× Go by the ratio rule with disjoint
IQRs, but the absolute difference is 6.7–8.4 ms on a 17 ms process, so the
panel's pre-registered status is startup-dominated.

## 3. Retained anomalies

Ten measured samples exceeded 3× their own group median. They are **retained**,
not deleted:

```text
w1_json/type_array/go run 12              25.89 ms vs 7.42
w1_json/compact_spaced/rust run 9         17.14 ms vs 3.67
w2_http/usage_too_few_args/go run 15      35.06 ms vs 5.79
w2_http/usage_too_few_args/go run 16      17.89 ms vs 5.79
w2_http/usage_too_few_args/rust run 15    12.60 ms vs 3.71
w3_sqlite/empty_script/go run 16          24.52 ms vs 7.06
w3_sqlite/directory_as_script/go run 3    19.13 ms vs 6.21
w3_sqlite/two_positionals/go run 8        39.16 ms vs 6.17
w4_pool/pipeline_10000_8/mink run 8       12.95 ms vs 3.00
w5_manifest/missing_keys/mink run 6       16.41 ms vs 3.94
```

They are consistent with ordinary OS scheduling/Defender interference on an
unpinned workstation, not with a correctness failure; every one passed the
golden gate. No rerun was substituted for them.

## 4. Files and commands

```text
python benchmarks/m4/harness/measure.py --workload <w> --runtime   # x5
benchmarks/m4/results/raw/<w>/runtime-<w>.jsonl                    # raw, retained
benchmarks/m4/results/reports/runtime-summary.json                 # full stats
docs/benchmarks/M4/M4_RESULTS_SUMMARY.json                         # committed stats
```

No fixture, application, expected output, compiler, runtime or MINK source
file was modified during this phase.

## 5. Limitations

- One machine, one run period, no CPU pinning, power-plan change or cache
  dropping; these are warm-process measurements.
- Python is CPython 3.11.9; C# is framework-dependent .NET 10.
- W1, W4 and W5 are startup-dominated; their runtime panels are not material.
- The W2 `refused` case is environment-constrained (≈2 s loopback connect).
- No user/kernel CPU split is reported here; `cpu_ms` per sample is present in
  the raw JSONL and in the summary JSON.
- No composite score exists, by design.
