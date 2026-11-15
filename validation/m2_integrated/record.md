# m2_integrated — Milestone 2 workload

**What it validates.** SQL + filesystem + JSON + threads in one program.

**How to reproduce.**

```
target/release/mink.exe build validation/m2_integrated/main.mink
python validation/m2_integrated/verify.py
```

**Result (Session 280, Windows x86_64, release compiler).** PASS: 100 SQL rows, 100 JSON records re-parsed from disk, matching value total 501,981, threaded total 333,833,500.

See `docs/validation/VALIDATION_MILESTONE_2_REPORT.md` for classification of
every failure found and the milestone verdict.

**Revalidated (M7 Phase D, 2026-11-15, release compiler).** PASS after the Phase D allocator change: 100 SQL rows, 100 JSON records re-parsed from disk, matching value total 501,981 and threaded total 333,833,500.

See [`SESSION_281_M7_PHASE_D_ALLOCATOR.md`](../../docs/implementation/SESSION_281_M7_PHASE_D_ALLOCATOR.md).
