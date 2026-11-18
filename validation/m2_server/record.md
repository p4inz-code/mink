# m2_server — Milestone 2 workload

**What it validates.** MINK HTTP server under concurrent clients.

**How to reproduce.**

```
target/release/mink.exe build validation/m2_server/main.mink
python validation/m2_server/verify.py
```

**Result (Session 280, Windows x86_64, release compiler).** 120 requests over 16 workers and 40 over 4 workers: every status and body correct, exact served count, usage errors exit 2.

See `docs/validation/VALIDATION_MILESTONE_2_REPORT.md` for classification of
every failure found and the milestone verdict.

**Revalidated (M7 Phase D, 2026-11-15, release compiler).** PASS after the Phase D allocator change: every status and body correct, exact served count, usage errors exit 2 (1.2 s).

See [`SESSION_281_M7_PHASE_D_ALLOCATOR.md`](../../docs/implementation/SESSION_281_M7_PHASE_D_ALLOCATOR.md).

**Revalidated (M7 Phase E, 2026-11-18, release compiler).** PASS: 120 requests over 16 workers plus 40 over 4, every status and body correct.

See [`SESSION_282_M7_PHASE_E_RUNTIME_BOUNDARIES.md`](../../docs/implementation/SESSION_282_M7_PHASE_E_RUNTIME_BOUNDARIES.md).
