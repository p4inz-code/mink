# Session 207 — Extended Windows Runtime Soak and Stability

Windows x86_64, 2026-11-07. Starting commit
`f298321de5f9e2b9e27891e6e104c51c914e4a22` (Session 206, local == origin/main
verified before any change).

## 1. Objective

Extend reliability evidence beyond the 180-second bounded soak from Session
199 with a longer repeatable mixed workload, explicit resource limits, and
measured trends rather than a single observation.

## 2. Safety limits (enforced by the test)

| Limit | Value |
| --- | --- |
| Wall clock | `MINK_SOAK_SECONDS`, default **300 s**, clamped to 30–900 s |
| Per-launch budget | 30 s; an overrun is killed and fails the test |
| Concurrent MINK children | exactly one |
| Disk payload | one 512-byte file; every generated exe removed after use |
| Artifact growth | asserted < 8 MiB for the whole run; soak subdirectory removed |
| TLS | one full suite run at the start, one near the end |
| Compiler / fatal-error work | every 20 launches, not every iteration |

The test is `#[ignore]`d like the other soak tests, so the default suite is
unaffected:

```
MINK_SOAK_SECONDS=300 cargo test --test soak s207_extended_mixed_soak -- --ignored --nocapture
```

## 3. Workload

Each launch (the process is created and destroyed every time, so runtime
init/shutdown repeats) executes 25 inner iterations of:

1. `Vec<Int>`: create, 40 pushes, length and last-element checks, free.
2. Allocate 64 bytes, store/load, free.
3. `Str` construction through `str_repeat` (512 bytes).
4. Filesystem: `fs_write` 512 bytes, `fs_read` and length check, `fs_file_size`
   exact check, missing-file read returning an owned empty string, remove.
5. Child process: `process_run("echo soak")` and exit-code check.

Every 20 launches the harness also:

- runs the real compiler over the same source and asserts a PE larger than
  10 KB;
- runs a double-free program three times and asserts exit 104 each time.

The TLS suite is run as a subprocess at the start and near the end; it is the
Session 205 suite (21 passed / 1 ignored), so every TLS operation runs against
the real Schannel client and the local Python endpoint.

## 4. Measurements — actual run

```
extended soak: budget=300s elapsed=300.1s launches=1440 codes={0: 1440}
builds=72 errors=216 tls=2/2 peak_ws=4.1 MiB handles=73->79
disk=230535->529031
```

| Metric | Observed |
| --- | --- |
| Duration | 300.1 s (budget 300 s) |
| Program launches | 1,440 |
| Exit-code distribution | `{0: 1440}` — no non-zero exit |
| Compiler builds | 72, all produced a PE > 10 KB |
| Fatal-error runs | 216, all exit 104 as specified |
| TLS suite runs | 2/2 passed |
| Peak working set | 4.1 MiB |
| Handle count | 73 → 79 over the final five launches (+6, within the +8 bound) |
| Artifact disk | +298,496 bytes, entirely the TLS fixture test's per-run root; the soak's own directory and executables were removed |
| Crashes / hangs / timeouts | 0 |

The handle trend is the harness's per-launch sampled peak; because each launch
is a fresh process, this is a per-process handle profile (runtime + stdio +
process pipe), not a cross-process leak. Memory stayed at 4.1 MiB across 1,440
fresh runtimes and 72 compile cycles.

## 5. Failure protocol

No workload failed. If one had, the harness keeps its stdout/stderr in the
artifact directory, fails the test with the launch number and transcript, and
`run_sampled` kills an over-budget child. The soak was not run with suppressed
failures and no test was skipped.

## 6. Limitations

- This is a 5-minute extended run, not a multi-hour soak; the cap is 900 s by
  design to protect the machine.
- The soak loop uses `Vec` rather than `Map`/`Set`; those are covered by
  `tests/collections_lib.rs` and the full regression, which runs alongside this
  session's evidence.
- TLS is exercised by two full suite runs, not once per soak iteration.
- Release-mode soak re-runs remain blocked by the environmental `rustc` crash
  recorded in Session 205; the debug profile was used.

## 7. Documentation

- `docs/roadmap/IMPLEMENTATION_ROADMAP.md` and
  `docs/audits/SYSTEMS_READINESS_AUDIT.md`: Session 207 addenda.
- `tests/soak.rs`: the new test and its safety limits.
- This record.

## 8. Verdict

**PASS — 300.1 s extended mixed soak, 1,440/1,440 launches clean, 72/72
compiler builds, 216/216 deterministic fatal errors, 2/2 TLS suites, 4.1 MiB
peak working set, +6 handle drift, bounded disk.** No crashes, hangs, leaks or
unexpected output. Release re-runs remain environment-blocked and are reported
as not run.
