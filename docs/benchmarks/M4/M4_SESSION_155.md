# Session 155 — M4 W4 `poolrun`

**Date:** 2026-11-01
**Status:** **COMPLETE — 5/5 implementations GREEN.**
**Rule:** no timing, no ranking, no composite score. This session establishes
functional correctness and deterministic output only.

W3 remains blocked for Rust and Go, but W4 is an independent workload and was
built and gated on its own evidence.

## 1. Contract

Source of truth: `validation/g_threads/main.mink`, `python_ref.py`,
`record.md`.

```text
poolrun squares  <n> <workers>
poolrun pipeline <n> <workers>
```

- `1..n` is split into `workers` disjoint ranges with the same integer
  division in every language: `start = k * (n / workers) + 1`,
  `end = (k + 1) * (n / workers)`, and the last range ends at `n`. Empty
  ranges are legal (e.g. `n < workers`) and contribute `0`.
- `squares`: every worker sums `i*i` over its range, adds its subtotal to one
  shared accumulator **under a lock**, and returns the subtotal; the parent
  joins in index order and prints `subtotal[k]` in that order, then
  `locked-total`, `joined-total`, `expected`.
- `pipeline`: stage 1 publishes each subtotal into its own slot; a join
  barrier separates the stages; stage 2 reduces the slots (a reducer thread in
  MINK, an equivalent reducer elsewhere); stdout prints `stage1[k]`, then
  `total`, `expected`.
- Determinism comes from disjoint ranges and join order, never from
  completion order.
- Exit codes: 0 ok; 1 usage (missing/extra/non-numeric/zero/unknown mode/
  option-looking argument); 3 a total disagrees with the closed form
  `n(n+1)(2n+1)/6`.
- stdout/stderr are UTF-8 with CRLF.

## 2. Concurrency model per language

| Language | Primitive used | Fairness note |
| --- | --- | --- |
| MINK | `thread_spawn` / `thread_join` + `lock_new`/`lock_acquire`/`lock_release` | explicit OS threads |
| Python | `threading.Thread` + `threading.Lock` | GIL serializes CPU-bound bytecode; the workload measures the contract, not parallel scaling |
| Rust | `std::thread::spawn` + `Arc<Mutex<..>>` | explicit OS threads |
| Go | goroutines + `sync.WaitGroup` + `sync.Mutex`, channel for the reducer | goroutines are multiplexed, not one OS thread per worker |
| C# | `Task.Run` + `lock`, `Task.WaitAll` | .NET thread-pool scheduling, not explicit threads |

These differences are **not equalized** and no artificial serialization was
added. They are recorded so the runtime panel cannot later present a
scheduling-model difference as a language difference. Python's stage-2
reduction runs on the main thread (the MINK app spawns a reducer thread); the
published values and the printed contract are identical.

## 3. Cases and results

26 cases: **8 scored** (the M4 budget) and 18 supporting (boundary `n`,
`n < workers`, `workers=1`, usage/type errors, option-looking argument).

| Language | Passed | Scored | State |
| --- | --- | --- | --- |
| MINK | 26/26 | 8/8 | GREEN |
| Python | 26/26 | 8/8 | GREEN |
| Rust | 26/26 | 8/8 | GREEN |
| Go | 26/26 | 8/8 | GREEN |
| C# | 26/26 | 8/8 | GREEN |

130/130 language-case combinations pass byte-for-byte on stdout, stderr and
exit code. `n=10000, workers=8` yields
`expected=333383335000` in every implementation.

## 4. Repeatability

The full suite was run twice (the second run is part of the Session 157
sweep). All 130 raw JSONL records were byte-identical between runs, including
stdout/stderr hashes and exit codes. No concurrency output varied.

The workload was also executed repeatedly during the first runs without a
single mismatch — the deterministic reduction means a scheduling race would
surface as a wrong total, not as a different order.

## 5. Dependencies

None beyond each language's standard library:

- MINK: `threads` stdlib module;
- Python: `threading`;
- Rust: `std` only (`Cargo.toml` has an empty dependency table);
- Go: `sync` + `fmt`/`os` (module has no requirements);
- C#: in-box `System.Threading.Tasks`.

## 6. Files

```text
benchmarks/m4/apps/mink/w4_pool/poolrun.mink
benchmarks/m4/apps/python/w4_pool/poolrun.py
benchmarks/m4/apps/rust/w4_pool/{Cargo.toml,src/main.rs}
benchmarks/m4/apps/go/w4_pool/{go.mod,main.go}
benchmarks/m4/apps/csharp/w4_pool/{poolrun.csproj,Program.cs}
benchmarks/m4/expected/w4_pool/{make_expected.py,cases.json,*.stdout}
```

## 7. Git state

HEAD at session start: `b74f249`. The session's benchmark files are committed
by the isolated Sessions 154–157 benchmark commit; no unrelated Session
128–149 file was staged. No MINK compiler/runtime/stdlib file was modified.

## 8. Limitations

- Runtime scaling is **not** a pure language comparison: the GIL, goroutine
  scheduling and the .NET thread pool are different execution models. Only the
  functional contract is compared in the correctness gate.
- No memory or startup measurement was taken.
- `n=10000` is the baseline; it is not calibrated as a "typical" workload.
