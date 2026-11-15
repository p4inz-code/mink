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
