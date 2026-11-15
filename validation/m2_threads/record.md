# m2_threads — Milestone 2 workload

**What it validates.** Thread contention, spawn/join churn and failure propagation.

**How to reproduce.**

```
target/release/mink.exe build validation/m2_threads/main.mink
validation/m2_threads/main.exe contend 12 5000 (plus cycles/failure)
```

**Result (Session 280, Windows x86_64, release compiler).** 60,000 locked increments exact, 100 spawn/join cycles lost no work, all 8 worker return codes propagated.

See `docs/validation/VALIDATION_MILESTONE_2_REPORT.md` for classification of
every failure found and the milestone verdict.

**Revalidated (M7 Phase D, 2026-11-15, release compiler).** All invariants hold after the Phase D allocator change: `contend 12 5000` → `counter=60000 returned=60000`, `cycles 100 7` → `counter=700`, `failure 8` → `joined=8 seven=8`, all exit 0.

See [`SESSION_281_M7_PHASE_D_ALLOCATOR.md`](../../docs/implementation/SESSION_281_M7_PHASE_D_ALLOCATOR.md).
