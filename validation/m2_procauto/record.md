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

**Revalidated (M7 Phase D, 2026-11-15, release compiler).** 8/8 cases match the Python reference after the Phase D allocator change (1.0 s). This workload also caught a latent ABI defect: `rt_process_run` kept the command length in `r11` across `rt_alloc`, which is volatile; the length is now spilled to the frame.

See [`SESSION_281_M7_PHASE_D_ALLOCATOR.md`](../../docs/implementation/SESSION_281_M7_PHASE_D_ALLOCATOR.md).

**Revalidated (M7 Phase E, 2026-11-18, release compiler).** 8/8 cases still match the Python reference. The 4,096-byte child-stdin cap is now measured at its boundary by [`validation/phase_e/stdin`](../phase_e/stdin/record.md): 4,095 and 4,096 bytes arrive whole, 4,097 and 5,000 are truncated to 4,096 silently, and the child still exits 0. Classified as an intentional design constraint (one pipe buffer prevents a synchronous-write deadlock), not a defect.

See [`SESSION_282_M7_PHASE_E_RUNTIME_BOUNDARIES.md`](../../docs/implementation/SESSION_282_M7_PHASE_E_RUNTIME_BOUNDARIES.md).
