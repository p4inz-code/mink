# m2_net — Milestone 2 workload

**What it validates.** TCP, UDP and HTTP clients under load.

**How to reproduce.**

```
target/release/mink.exe build validation/m2_net/main.mink
python validation/m2_net/verify.py
```

**Result (Session 280, Windows x86_64, release compiler).** 11/11 cases match the Python reference (1 MiB echo, 200 round trips, 8 KiB datagram, HTTP GET, refusal, bounded timeout).

See `docs/validation/VALIDATION_MILESTONE_2_REPORT.md` for classification of
every failure found and the milestone verdict.
