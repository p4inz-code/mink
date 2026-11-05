# Session 130 — M2 P2: Windows process I/O quick validation (child stdout/stderr capture boundary)

STATUS: **PASS — no defect.** The previously identified ~4 KiB child stdout/stderr capture
limitation is **not reproduced** on the current tree: it was already resolved by Session 125
(8 MiB per-stream capture windows) and Session 97 (full drain of large child output). Current
behavior matches the documented `process` API contract. **No source or test change.** Timeboxed
validation only.

Baseline: HEAD `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d`, branch `main`, `Cargo.toml` **1.0.4**
— no version bump, no npm publish, no tag, **Linux FROZEN and untouched.** The Session 128 docs
and the Session 129 P1 change (`src/mir/lower.rs`, `tests/closures.rs`) remain in the working
tree unchanged by this session.

## 1. Contract inspected

`stdlib/process.mink` documents the capture contract:

- stdout/stderr are captured in **runtime-owned windows**; each run overwrites them, and they
  grow with demand **up to the stream window**;
- exceeding the window **fails the program with `E-R02` instead of truncating silently**;
- `process_run(cmd)` returns the child exit code; no output = empty string.

The window is `STDIO_CAPTURE_WINDOW_BYTES` = **8 MiB per stream** (three windows — stdin,
captured stdout, captured stderr — inside one lazily committed 24 MiB region;
`src/runtime/abi.rs`, Session 125). The large-output drain was fixed in Session 97
(`tests/process_lib.rs` `s97_large_stdout_drains_fully`, `s97_large_stdout_64k`,
`s97_large_stdout_1mb`, `s97_stdout_just_above_pipe_buffer`,
`s97_stdout_exact_pipe_buffer_boundary`, `s97_small_stdout_exact_content`).

Separately documented and unchanged: `process_run_input` delivers **at most one pipe buffer
(4096 bytes)** of child stdin by design (a larger synchronous write could deadlock against a
child that has not started reading). Not re-tested here; it is a documented deliberate limit.

## 2. Baseline

`cargo test --test process_lib` → **86 passed, 0 failed** on the current tree.

## 3. Real Windows child-process workload

Scratch probes under the git-ignored `tmp/m2_p2/`; each spawns a real Windows child
(`python -c ...` for byte-exact output, `echo` for a small child) through `process_run` and reads
the capture via `process_stdout_len` / `process_stderr_len` / `rt_str_byte`.

| Case | Child output | Observed | Verdict |
|---|---|---|---|
| stdout below the old boundary | 100 B stdout | `c=0`, captured length **100** | complete |
| stdout above the old boundary | 102 400 B stdout | `c=0`, captured length **102 400**, first byte **66** (`B`), last byte **66** | complete, byte-exact |
| stderr above the old boundary | 102 400 B stderr | `c=0`, stderr length **102 400**, stdout length **0** | complete, correctly separated |
| repeated child execution | 30 × 102 400 B stdout | **0 mismatches in 30 runs** (30/30 captured exactly 102 400) | complete every run |
| capture freshness | a following `echo tiny` | captured length **6** (`tiny\r\n`) | per-run overwrite works |
| above the documented window | 9 MiB stdout (9 437 184 B) | program aborts: `E-R02`, **exit 102**, message `out of memory: the runtime heap is exhausted`, located at `stdlib/process.mink:37` | matches the documented "`E-R02`, never silent truncation" |

No truncation, no silent discard, no crash, no partial capture was observed anywhere at or below
the boundary. The 9 MiB case fails loudly and diagnosably, exactly as the contract states.

## 4. Classification

- **The previously reported ~4 KiB capture limitation: NOT REPRODUCED (resolved).** It is
  superseded by Session 125/97; the current implementation satisfies its documented contract.
  There is no `A` (defect), `B` (missing capability) or `E` (fixture) finding here.
- **Residual 8 MiB per-stream ceiling → `E-R02`:** **D — deliberate, documented design limit**
  (shared stdio capture region; the failure is explicit, not silent).
- **Observation (low severity, not fixed):** the `E-R02` diagnostic text is generic
  (`out of memory: the runtime heap is exhausted`) for *both* genuine heap exhaustion and
  capture-window overflow, because the emitter raises the same `OutOfMemory` kind
  (`src/runtime/error.rs`: `Self::OutOfMemory => "E-R02"`; overflow site
  `src/backend/emit/runtime.rs` `fail(code, 2)` with the comment "captured output exceeded the
  stream window"). The **code** and the **no-silent-truncation** contract are correct; only the
  wording/point-of-origin is imprecise. This is a diagnostics-quality item for a later M2 pass,
  not a contract violation, and fixing it would mean touching the emitted runtime — out of scope
  for this validation session.

## 5. Root cause / fix

No defect was proven, so **no root cause and no fix apply**; per the brief, no source code was
modified and no test was added (there is no failing behavior to regress).

## 6. Tests

- `cargo test --test process_lib` → **86 passed, 0 failed** (baseline re-confirmed; includes the
  Session 97 large-output drain suite).
- No test added, deleted, weakened or skipped.

## 7. Remaining limitation

Child stdout/stderr capture is bounded at **8 MiB per stream**; beyond that a program terminates
with `E-R02` (exit 102) rather than truncating. That is the documented intended behavior. The
only soft spot is the generic `E-R02` message wording for the window-overflow case (see §4).
`process_run_input` still delivers at most 4096 bytes of child stdin by design.

## 8. Repository state

No files changed by this session (probes live in git-ignored `tmp/m2_p2/`). `Cargo.toml` /
`npm/mink/package.json` at **1.0.4**; tags unchanged (`v1.0.0`–`v1.0.4`); no npm/release/publish;
no Linux source touched. Nothing committed.

## 9. Next recommended M2 workload

**M2 P3 — child stdin and process-lifecycle stress:** verify the documented 4096-byte
`process_run_input` stdin limit and its interaction with larger reads (the limit is currently
documented but not covered by a boundary test), then stress repeated `process_run` under load
(many short-lived children plus output capture) to confirm the capture region stays correct and
leak-free across long sequences.
