# Session 131 — M2 P3: child stdin boundary and process-lifecycle stress

STATUS: **PASS — no defect.** The documented 4096-byte `process_run_input` stdin boundary is
confirmed exactly (a complete, byte-exact stream up to and including 4096 bytes; a correct
4096-byte prefix beyond it, which is the documented behavior), and a repeated short-lived-child
workload never corrupted capture state, never leaked a handle or an allocation, and never made a
later child unreliable. **No source or test change.** Timeboxed validation only.

Baseline: HEAD `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d` ("validation: close Windows practical
parity milestone 1"), branch `main`, `Cargo.toml` **1.0.4** — no version bump, no npm publish, no
tag, **Linux FROZEN and untouched.** Windows x86_64 is the only active platform. The uncommitted
Session 128 roadmap documentation and the Session 129 P1 change (`src/mir/lower.rs`,
`tests/closures.rs`) remain in the working tree exactly as found; this session changed no tracked
file. Nothing was committed.

## 1. Contract inspected

**Documented stdin contract** (`stdlib/process.mink`, `process_run_input` — mirrored verbatim in
`npm/mink/stdlib/process.mink`):

> The child then sees EOF on stdin, exactly as if the input had been redirected from a file. At
> most one pipe buffer (4096 bytes) of input is delivered; longer input is truncated there,
> because a larger synchronous write could deadlock against a child that has not started reading
> stdin yet.

**Runtime implementation** (`src/backend/emit/runtime.rs`): `emit_process_run_common(code, true)`
builds `"cmd.exe /c " + cmd` (so the child is always a real `cmd.exe` process), creates
stdin/stdout/stderr pipes, makes the parent ends non-inheritable, then writes
`min(input_len, STDIN_PIPE_LIMIT)` with `STDIN_PIPE_LIMIT = 4096` (line 84) in a loop that stops
on a failed or short `WriteFile`, closes every write end so the child sees EOF, and only then
enters the poll-and-drain capture loop. The accessors' documented model
(`src/runtime/intrinsics.rs`) matches.

**Adjacent, already-covered contract:** stdout/stderr capture (8 MiB per stream, loud `E-R02` on
overflow) was validated in Session 130 (M2 P2); the child stdin *cap* was documented but had no
boundary test — the gap this session closes.

**Existing stdin tests** (`tests/process_lib.rs`): `s125_input_reaches_child_stdin`,
`s125_input_eof_is_delivered_to_child`, `s125_empty_input_still_runs_child`,
`s125_exit_code_preserved_with_input`, `s125_child_that_ignores_stdin_does_not_deadlock` — all
functional, none at the byte boundary. Nothing was weakened, added or removed.

## 2. Baseline

`cargo test --test process_lib` → **86 passed, 0 failed, 0 ignored** (12.7 s). All later runs of
the same target reproduced this.

## 3. Probe harness

Scratch probes live in the git-ignored `tmp/m2_p3/` (`.gitignore` line 14, `tmp/`). The CLI
resolves `mod` against the repository stdlib, so probes are **built from the repository root**
(`./target/debug/mink.exe build tmp/m2_p3/<probe>.mink`) and **run from `tmp/m2_p3/`**, which is
where the relative child name resolves.

| File | Role |
|---|---|
| `child_stdin.mink` | deterministic child: reads stdin to EOF, prints byte count, byte sum, weighted byte sum (Σ `b[i]·(i+1)`), first and last byte |
| `boundary.mink` + `verify_boundary.py` | boundary parent + independent verifier (rebuilds the expected stream itself) |
| `child_stress.mink` | echo child: stdout = payload + CRLF, stderr = `"ERR:"` + payload (different prefix length, so a swap is visible as a length mismatch) |
| `stress.mink` + `verify_stress.py` | 60-run workload, two injected failures, a final run + independent verifier |
| `child_ignore.mink`, `deadlock.mink`, `verify_deadlock.py` | the documented deadlock reason for the cap |
| `soak.mink`, `soak_handles.py` | 1000-run soak + Windows handle-count sampling |
| `handle_base.mink`, `handle_one.mink`, `after_{1,10,200}.mink`, `handle_settled.py` | settled handle count vs number of runs |

Every verifier recomputes the expected bytes from the requested size and pattern; nothing is taken
on trust from the probe parent.

## 4. Stdin boundary cases

`verify_boundary.py` (all cases: child exit 0, captured stderr 0 bytes, parent exit 0):

| Requested bytes | Pattern | Child actually received | Content check | Verdict |
|---|---|---|---|---|
| 0 | `A` | **0** | byte count/sum/first/last all match | complete |
| 1024 | `AB` | **1024** | exact (count, sum, weighted sum, first, last) | complete |
| 4094 | `AB` | **4094** | exact | complete |
| 4095 | `A` | **4095** | exact | complete |
| **4096** | `AB` | **4096** | exact | complete — the documented limit is inclusive |
| 4097 | `A` | **4096** | exact prefix of the requested stream | truncated at the documented limit |
| 4098 | `AB` | **4096** | exact prefix | truncated at the documented limit |
| 8192 | `AB` | **4096** | exact prefix | truncated at the documented limit |

No case hung, crashed, or produced corrupted bytes: the delivered bytes are always an exact
prefix of the requested payload (weighted sum plus first/last byte make a shift, splice or pad
detectable), stderr stays empty, and the parent always exits 0. Truncation above 4096 bytes is
**silent** — which is exactly what the function documents ("longer input is truncated there"),
and is a deliberate difference from the capture-window overflow path, which fails loudly with
`E-R02` (Session 130).

**Deadlock guard** (`verify_deadlock.py`): 5000 bytes (more than one pipe buffer) to a child that
never reads stdin and writes 63 000 bytes to stdout → parent exit 0, child exit 0, **63 000 bytes
captured**, no hang. This confirms the documented reason for the cap: the capped write fits the
pipe buffer, so the parent reaches the drain loop instead of blocking on a full stdin pipe.

## 5. Process-lifecycle stress

`verify_stress.py` — 60 normal runs (`child_stress.exe` through `process_run_input`), payload =
rotating unit `A`/`B`/`C` repeated 100 + 17·i times, with every twentieth run oversized (5000
bytes, i.e. above the stdin cap) to exercise the truncation path inside the stress workload:

- **every child exits correctly:** all 60 `process_run_input` results were 0;
- **stdout correct:** all 60 captured lengths and byte sums matched the expected payload + CRLF;
- **stderr correctly separated:** all 60 stderr lengths/sums matched `"ERR:"` + payload, and the
  two stream lengths differ by construction, so a swap cannot pass;
- **no cross-run contamination:** each expected `(stdout, stderr)` pair is unique across the
  workload, and every run matched *its own* pair exactly — a stale capture would have matched a
  neighbouring run instead;
- **capture state fresh per execution:** including the three oversized runs, which each saw a
  fresh 4096-byte prefix of *that run's* payload;
- **failures do not poison later runs:** an injected `exit 3` child returned exit 3 with both
  streams empty; a nonexistent command (`mink_no_such_command_xyz`) launched `cmd.exe`, which
  exited 1 with 115 bytes of its "not recognized" text on **stderr** and nothing on stdout
  (verified independently: `cmd /c mink_no_such_command_xyz` exits 1 with exactly that message);
- **final successful child after everything:** a distinct 777-byte `Z` payload → exit 0,
  stdout 779 B, stderr 781 B, both byte-exact;
- **parent exit 0**: with the standalone runtime's leak checker active (`E-R06` → exit 106), this
  is a live-allocation check across all 63 child runs.

**Soak** (`soak_handles.py`, 1000 consecutive children, each validated inline for exit code, both
captured lengths and the first/middle/last payload bytes): **exit 0, 0 run failures, ~28 s**
(≈28 ms per child, no progressive slowdown). No stale state, no degradation, no failure.

## 6. Handle and capture-region cleanup (Windows)

Handles were measured externally with `GetProcessHandleCount` (no in-process instrumentation):

| Measurement | Result |
|---|---|
| idle runtime (`handle_base.exe`, no child) | **44** handles steady |
| settled after 1 / 10 / 200 child runs | **50 / 50 / 50** |
| soak in flight (1000 runs) | min **54**, max **60**, first 54, last 54 |
| soak tail trend | flat (no monotonic climb) |

The count is **invariant in the number of runs**: the same value after 1, 10 and 200 runs proves
there is no per-run retention (one leaked handle per run would show as +199 between N=1 and
N=200). The constant +6 over the idle baseline is established once, on the first child spawn, and
never grows; the soak's 54–60 band is that baseline plus the ~4–6 handles of the child in flight
(pipes + process object). Absolute baselines drift by ~6 between sampling sessions, so the
evidence is the invariance, not the absolute number.

`ensure_stdio_capture` (`src/backend/emit/runtime.rs`) commits the 24 MiB capture region with
`VirtualAlloc`, which creates no handles; the capture region is therefore reused, not re-created,
across runs — consistent with the fresh-window behavior verified in §5.

## 7. Classification

**PASS — no defect.** Every observation matches the documented contract:

- the 4096-byte stdin limit is exact and inclusive (4096 delivered complete, 4097 → 4096);
- over-limit input is a correct silent prefix truncation, as documented;
- the cap does what its documentation says: no deadlock against a child that ignores stdin;
- repeated short-lived children keep capture state fresh, keep stdout and stderr separated, never
  contaminate one another, survive injected failures, and leave no handle or allocation growth.

No `A` (defect), `B` (missing capability) or `E` (probe/fixture) finding remains open.

**Observations recorded, not fixed (no defect, out of scope):**

1. *Documentation consistency (low).* `stdlib/process.mink`'s **module header** describes the
   stdout/stderr window behavior but does not mention the 4096-byte stdin cap; the cap is
   documented only on `process_run_input` itself. Both files (`stdlib/` and the mirrored
   `npm/mink/stdlib/`) would need the same edit, and the behavior is already documented where it
   matters; recorded for a future documentation pass, not changed here.
2. *Silent vs loud truncation (deliberate, low).* Over-long stdin and over-long captured output
   behave differently by design: stdin truncates silently at 4096 B, stdout/stderr capture fails
   loudly with `E-R02` at 8 MiB. Both are documented; noted only so the asymmetry is a recorded
   decision rather than an accident.
3. *Still open from M2 P1* (unchanged by this session): finding **A4** (if-expression in operand
   position crashes the compiler) and **A5** (`if` without `else` as a non-final statement in a
   closure body is a parse error). Both remain candidates for a later M2/M3 implementation pass.

## 8. Root cause and fix

No defect was proven, so **no root cause and no fix apply**. Per the brief, no source file and no
test was modified or added: there is no failing behavior to regress.

## 9. Corrections made during testing (probe-side, not product)

Recorded explicitly, in the same spirit as M2 P2's probe-reading correction. All four were faults
in the scratch harness; each was found by the probe failing and root-caused before any conclusion
was drawn.

1. **Children exited 106 (`E-R06` leak) on the first run.** `rt_stdin_read()` returns a heap
   string that the caller owns, and the standalone build enables the leak checker, so a child that
   never frees it exits 106. The fix is in the probe: `rt_str_free(data)` before `rt_exit`.
   Verified directly (`printf 'AB' | child_stdin.exe` → `2/131/197/65/66`, exit 0). The runtime
   behavior is correct — the probe was wrong.
2. **False "cross-run contamination" in the stress verifier.** The first workload made every
   tenth run oversized (5000 B); since delivery is capped at 4096, runs 7/37, 17/47 and 27/57
   delivered the *same* 4096-byte prefix (same rotating unit), so the verifier's uniqueness check
   flagged three collisions that were an artifact of the probe design. Changed the injection to
   every twentieth run, so each oversized run keeps a distinct unit. A reminder that above the
   cap the only observable payload differences are the unit and the reported size.
3. **False checksum mismatch on the final run.** The verifier's first version expected the loop's
   unit (`A`) for the final payload; the probe actually feeds `Z`. The observed sum (69 953 =
   777·90 + 23) was correct — the verifier was wrong. Fixed in the verifier.
4. **False "handle leak" in the first soak sample set.** Samples taken after the parent had
   already exited read a handle count of 0, which made the peak-to-trough spread look like 66.
   Fixed by discarding post-exit zeros and the first (pre-initialization) sample; the steady state
   is a flat 54–60.
5. **Compiler invocation from the wrong directory.** Building the probes from inside `tmp/m2_p3/`
   fails with `E-S01 module file 'process.mink' not found` — the CLI resolves `mod` against the
   repository stdlib, so probes must be built from the repository root with a path (the M2 P1
   practice). Harness detail, not a product defect.

## 10. Changes made

- **Product source: none.** No file under `src/` was touched; `stdlib/`, `npm/`, `tests/` and
  `Cargo.toml` are unchanged; Linux sources untouched; no version change, tag, release or publish.
- **This document** is the only tracked addition.
- All probes and their outputs live in the git-ignored `tmp/m2_p3/`.

## 11. Tests run

- `cargo test --test process_lib` → **86 passed, 0 failed, 0 ignored** (baseline and re-confirmed
  after all validation; no test added, deleted, weakened or skipped).
- `python verify_boundary.py` → 8/8 cases match the documented contract.
- `python verify_deadlock.py` → no hang; 63 000 bytes captured.
- `python verify_stress.py` → 60/60 normal runs correct and uncontaminated; 2 injected failures
  correct; final run correct; parent exit 0.
- `python soak_handles.py` → 1000/1000 children, exit 0, handle count flat (54–60).
- `python handle_settled.py` → settled handles constant at 50 for 1/10/200 runs.

## 12. Remaining limitations

- Child stdin stays capped at one pipe buffer (4096 bytes) by design; longer input is silently
  truncated (documented). There is no chunked or streaming stdin path.
- Child stdout/stderr capture stays bounded at 8 MiB per stream with a loud `E-R02` beyond
  (Session 130), and the `E-R02` wording for window overflow is generic (Session 130 §4).
- The measured handle behaviour describes this workload: one child in flight at a time, sequential
  `process_run` calls. Concurrent child processes were not exercised here (no `process` API
  exposes them beyond OS threads spawning children; that belongs to a concurrency-stress pass).
- M2 P1 findings A4 and A5 remain open (§7.3).

## 13. Repository state and audit

- `git status --short` is identical to the state at the start of the session: the same 12 modified
  files (Session 128/129 work: 10 `.md` files, `src/mir/lower.rs`, `tests/closures.rs`) and the
  same untracked pre-existing files (`nl1.exe`, `nl1.mink`, `stdlib/httpserver.mink`,
  `npm/mink/stdlib/httpserver.mink`, `tests/httpserver_lib.rs`,
  `freebuff-chat-2026-10-01T19-08-00.600Z.md`, the Session 128/129/130 documents,
  `docs/roadmap/archive/`). This session added only this document.
- Probe artifacts (`tmp/m2_p3/`) are ignored (`git check-ignore` → `.gitignore:14:tmp/`).
- `Cargo.toml` and `npm/mink/package.json` remain **1.0.4**; tags remain `v1.0.0`–`v1.0.4`; no
  npm/release/publish command was run; nothing was committed; no Linux file was touched.
- No pre-existing untracked file was created, modified, deleted or absorbed.

## 14. Next recommended M2 task

**M2 P4 — resource/scaling stress on Windows**, the largest untouched M2 area: files far above
4 MiB, growing strings, large JSON payloads, large HTTP bodies, fragmentation under repeated
alloc/free, and sustained execution over minutes. The process-automation area is now closed for
stdout, stderr, child stdin, repeated children, invalid commands and cleanup; only real pipelines
beyond the existing coverage would extend it. If a defect surfaces, follow the M2 rule: classify
it (**A**/**B**/**C**/**D**/**E**) and only genuine **A** defects become implementation
candidates — the two open **A**-class compiler findings (A4, A5) are the standing candidates from
P1. Do not begin Linux work.
