# Benchmark results

Median of 7 runs, whole-process wall time in milliseconds (lower is faster).

| Benchmark | MINK | Python | Go | Rust | MINK vs Python |
|---|---|---|---|---|---|
| recursive Fibonacci of 32 (function calls) | 14 | 193 | 11 | 7 | 13.8x faster |
| 30 million iterations of integer arithmetic | 55 | 2137 | 20 | 19 | 38.7x faster |
| sieve of Eratosthenes to 2 million (array reads and writes) | 56 | 369 | 15 | 13 | 6.5x faster |
| longest Collatz chain under 100,000 (branches and division) | 61 | 763 | 19 | 12 | 12.6x faster |

## Machine and tools

- cpu: Intel(R) Core(TM) i9-14900K
- os: Windows-10-10.0.26200-SP0
- mink: mink 1.0.5
- python: Python 3.11.9
- go: go version go1.26.7 windows/amd64
- rust: rustc 1.97.1 (8bab26f4f 2026-07-14)
- runs: 7

## Notes

- There was no C or C++ compiler on the test PC, so Go and Rust stand in as the compiled baselines.
- The four programs are the plain version of each algorithm, not tuned. Rust was built with `rustc -O`; Go with the default `go build`.
- Times include starting the process, so very short runs are dominated by start-up.
- One machine, one run of the whole suite. Treat the numbers as a starting point, not a verdict.
