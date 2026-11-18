# m2_bigio — Milestone 2 workload

**What it validates.** Large data, files, stdin and JSON.

**How to reproduce.**

```
target/release/mink.exe build validation/m2_bigio/main.mink
python validation/m2_bigio/verify.py
```

**Result (Session 280, Windows x86_64, release compiler).** 25/25 cases match the Python reference (5 MiB file, 1 MiB stdin, 12,000-record JSON, 20,000-cycle string churn, every error path).

See `docs/validation/VALIDATION_MILESTONE_2_REPORT.md` for classification of
every failure found and the milestone verdict.

**Revalidated (M7 Phase D, 2026-11-15, release compiler).** 25/25 cases match the Python reference after the Phase D allocator change (24.2 s), and `churn_repro.mink` — run unchanged — now completes 200,000 and 1,000,000 cycles with exit 0 (20,000 previously died with `E-R02`).

See [`SESSION_281_M7_PHASE_D_ALLOCATOR.md`](../../docs/implementation/SESSION_281_M7_PHASE_D_ALLOCATOR.md).

**Revalidated (M7 Phase E, 2026-11-18, release compiler).** 25/25 cases still match the Python reference; `churn_repro.mink 1000000` completes in 0.038 s with exit 0 (no growth beyond generation retirement).

See [`SESSION_282_M7_PHASE_E_RUNTIME_BOUNDARIES.md`](../../docs/implementation/SESSION_282_M7_PHASE_E_RUNTIME_BOUNDARIES.md).
