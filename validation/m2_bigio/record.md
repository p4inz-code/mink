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
