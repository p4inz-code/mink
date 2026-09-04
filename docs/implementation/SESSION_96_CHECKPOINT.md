# Session 96 — Windows Real-World Apps + HTTP POST + Error-Path Hardening (CHECKPOINT)

**Status: PAUSED at a safe stopping point (mid-session).**
**Starting commit: `f95d5a6` (clean).** Working tree returned to `f95d5a6` state at pause.

## What was completed before the pause

1. **Baseline truth audit** — clean tree at `f95d5a6`; Session 95 report confirmed
   (WINDOWS RUNTIME = STABLE, MULTI-PURPOSE EXECUTION = EXECUTION VERIFIED, P0/P1 = 0 remaining).
2. **HTTP POST audit** — `stdlib/http.mink` has an end-to-end POST builder:
   `http_client_post_with` / `http_client_post` exist and are wired through the same
   send/receive path as GET. POST execution test was planned but NOT yet run.
3. **net_init audit** — `net_init()` is the WSAStartup intrinsic; other net intrinsics
   (`net_tcp_socket`, `net_send`, etc.) do not guard against uninitialized state; a
   program that skips `net_init()` reaches the runtime and can fault. Fix (clean error)
   was planned, NOT implemented.
4. **process_stderr audit (root cause found)** — MINK source has NO user-facing stderr
   write intrinsic (only an internal `WriteStderr` used for runtime errors), so a MINK
   child legitimately cannot produce stderr. Stderr CAPTURE itself was probed and is
   deterministic and byte-exact for python/cmd children (several probes). The Session 95
   "unreliable" observation most likely traced to a MINK child that wrote nothing to stderr.

## P1 DISCOVERED: process_run deadlocks on child output > pipe buffer (~4096 bytes)

Evidence (boundary probe against the Session 95 binary AND the current tree):

- 100 B child stdout: rc=0, len=100 ✅
- 4000 B: rc=0, len=4000 ✅
- 4096 B: rc=0, len=4088 (V1 capture cap 4088; 8 bytes discarded — by design) ✅
- **4097 B: process_run returns rc=143, len=0, then the process hangs at exit** (the
  baseline code hangs indefinitely at `WaitForSingleObject(INFINITE)` BEFORE reading the
  pipe — the child blocks writing the 4097th byte into the full pipe, parent never drains).

Root cause: `emit_process_run` does wait-then-read; a child that fills the pipe buffer
blocks forever and the parent never reads. This is a genuine P1 (reliable-use blocker for
process capture of realistic output).

## Attempted fix (REVERTED for safe stopping — do not lose the design)

A poll-and-drain rewrite was drafted in `src/backend/emit/runtime.rs`:

- `emit_process_drain_body(code, handle_disp, buf_bss, captured_disp, avail_disp, read_disp, scratch_disp)`:
  non-blocking drain of one pipe via `PeekNamedPipe` + `ReadFile(n = min(avail, 4088-captured))`;
  when the 4088-byte capture is full it discards overflow into a 256-byte stack scratch
  ([rbp-512..-256]) so the child never blocks; `PeekNamedPipe` returning FALSE (EOF/broken
  pipe) ends the drain.
- Poll loop: drain stdout, drain stderr, `WaitForSingleObject(hProcess, 20)`; if signaled
  → final drain of both pipes → `GetExitCodeProcess`.
- Frame slots: [-200] stdout captured, [-208] stderr captured, [-216] avail, [-224] read;
  PROCESS_INFORMATION at [-192], STARTUPINFOA at [-168..-64], SA at [-544], scratch [-512..-256].
  No slot overlaps.
- `IAT_PEEK_NAMED_PIPE` was added to `src/backend/emit/pe.rs` imports.

Result: builds clean; 100/4000/4096 cases correct; **4097 case still misbehaves**
(returns rc=143, len=0, then hangs at exit rather than at wait). The 143 + exit-time hang
was NOT root-caused (possible pipe-buffer race, possible final-drain spin, possible
`PeekNamedPipe` EOF semantics). Because the fix was not fully verified, the emitter
changes were reverted to keep the tree at the known-green `f95d5a6` state.

## Next session (Session 97) must

1. Re-apply the poll-and-drain design above (comments in git history / this doc) and
   root-cause the 4097-byte anomaly (probe PeekNamedPipe return on EOF with buffered data;
   verify the drain loop cannot spin after process exit; consider draining BEFORE first wait).
2. Run the HTTP POST execution test (builder exists end-to-end).
3. Add the net_init-without-init clean-error guard + regression test.
4. Decide stderr: likely document that MINK children have no stderr-write intrinsic (P3).
5. Finish Phases 6–13 of the Session 96 brief (large-data stress, CLI/npm recheck, quality
   gates, docs, severity audit) and commit.

## Known state at pause

- Working tree: clean at `f95d5a6` (verified: `cargo build` clean, `process_lib` 71/71).
- No commits made this session; nothing to commit.
- Linux untouched (frozen), as required.
- Severity: P1 = process_run >4 KB output deadlock (pre-existing, discovered this session,
  documented above, NOT fixed). Everything else from Session 95 stands.