# m2_procauto — Milestone 2 workload

**What it validates.** Process automation: capture, stdin, repeated runs, exit codes.

**How to reproduce.**

```
target/release/mink.exe build validation/m2_procauto/main.mink
python validation/m2_procauto/verify.py
```

**Result (Session 280, Windows x86_64, release compiler).** 8/8 cases match the Python reference (64 KiB stdout capture, 3-byte stderr, 4,096-byte stdin limit, 50 child runs, missing command).

See `docs/validation/VALIDATION_MILESTONE_2_REPORT.md` for classification of
every failure found and the milestone verdict.
