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
