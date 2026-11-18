# m2_soak — Milestone 2 workload

**What it validates.** Bounded long-running workload (10 s).

**How to reproduce.**

```
target/release/mink.exe build validation/m2_soak/main.mink
validation/m2_soak/main.exe 10 soak_work
```

**Result (Session 280, Windows x86_64, release compiler).** exit 0 after 238,152 iterations and 23,816 thread lifecycles; no leak report, exact thread counter.

See `docs/validation/VALIDATION_MILESTONE_2_REPORT.md` for classification of
every failure found and the milestone verdict.

**Revalidated (M7 Phase D, 2026-11-15, release compiler).** Exit 0 after the Phase D allocator change: 1,512,201 loops and 151,222 thread lifecycles in the 10 s window, no leak report. The same binary built against the pre-fix runtime managed 219,352 loops and 21,936 thread lifecycles in the same window on the same machine (6.9×).

See [`SESSION_281_M7_PHASE_D_ALLOCATOR.md`](../../docs/implementation/SESSION_281_M7_PHASE_D_ALLOCATOR.md).

**Revalidated (M7 Phase E, 2026-11-18, release compiler).** 10 s soak: 1,595,081 loops, 159,510 thread lifecycles, exit 0, no leak report.

See [`SESSION_282_M7_PHASE_E_RUNTIME_BOUNDARIES.md`](../../docs/implementation/SESSION_282_M7_PHASE_E_RUNTIME_BOUNDARIES.md).
