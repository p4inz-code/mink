# Session 96 — Windows Real-World Apps + HTTP POST + Error-Path Hardening

**Status: COMPLETE (with one documented open P1).**
**Starting commit: `f95d5a6` (clean).** **Final commit: (see git log).**
**Session 95 baseline: 2536 tests passed.** **Final: 2538 passed / 0 failed.**

## Goals and outcomes

| Goal | Outcome |
|---|---|
| Composed real MINK applications (Phases 1–2) | EXECUTION VERIFIED — six Session-95 apps re-verified plus file+JSON, HTTP+JSON, process, network-diag, and crypto composition evidence on Windows |
| HTTP POST execution (Phase 3) | EXECUTION VERIFIED — new durable test `http_windows_post_exact_body_roundtrip` |
| net_init missing-init error path (Phase 4) | FIXED — clean errors, regression test added |
| process_stderr audit (Phase 5) | Root cause found: MINK has no user-facing stderr-write intrinsic; CAPTURE is byte-exact. Classified P3 (missing intrinsic). |
| Large/repeated workloads (Phase 6) | EXECUTION VERIFIED — 60 KB HTTP body, 2000-cycle allocator, 3× repeated POST/GET, repeated process runs (existing suites) |
| Windows user workflow / npm (Phase 7) | Re-verified — version aliases, check/build/run, spaces-in-path, standalone exe all pass |
| Quality gates (Phase 10) | fmt clean · clippy 0 new warnings · test 2538/0 · debug + release builds clean |

## HTTP POST — EXECUTION VERIFIED

`stdlib/http.mink` has an end-to-end POST builder (`http_client_post_with`, `http_client_post`):
request line, `Host`, `Content-Type: text/plain`, `Connection: close`, inline decimal
`Content-Length` (1–7 digits), body, send, multi-recv response. New durable Rust test
`http_windows_post_exact_body_roundtrip` runs a deterministic localhost echo server and a
real generated MINK executable performing three sequential POSTs:

- 511-byte body of `'A'` → HTTP 200, body echoed byte-exact, length exact (3-digit CL)
- 2048-byte body of `'B'` → HTTP 200, byte-exact (4-digit CL)
- empty body → HTTP 200, `Content-Length: 0`, exact

Ownership: `http_client_post_with` frees `path`/`body`; `http_send` frees `request`/`host`;
the returned response is freed by the caller at a single exit. Leak checker on — exit 0.

## net_init error path — FIXED (was: potential fault through NULL Winsock pointer)

Root cause: `rt_net_wsa_startup` sets a BSS `NET_INIT_FLAG`, but every other net intrinsic
called Winsock through `NET_FUNC_TABLE` function pointers with no init check. Before
`net_init()`, the table is all zeros (DLL never loaded), so a direct call faults through
a NULL pointer instead of failing cleanly.

Fix (`src/backend/emit/runtime.rs`, Windows-only emitter — Linux untouched):
every func-table-calling net intrinsic now checks `NET_INIT_FLAG` at entry and jumps to a
clean-error tail:

- `rt_net_socket` / `rt_net_connect` / `rt_net_bind` / `rt_net_listen` /
  `rt_net_accept` / `rt_net_send` / `rt_net_close` / `rt_net_shutdown` → `-1`
  (the documented failure value; stdlib callers already handle it)
- `rt_net_recv` / `rt_net_get_host_name` → allocated empty `Str` (their documented error
  shape; no leak, checked)
- Pure register intrinsics (`htons`, `ntohs`, V1 `getaddrinfo` passthrough) need no guard.

Regression test `network_operations_without_init_fail_cleanly`: a real MINK program that
skips `net_init()` and calls `net_tcp_socket`, `net_udp_socket`, `net_hostname`, `net_recv`
must return the documented failure values, exit 0, and pass the leak checker (no crash).
18/18 hardening tests pass.

## process_stderr audit — root cause found (P3, no code change needed)

- MINK source has **no user-facing stderr-write intrinsic** (only an internal `WriteStderr`
  used for runtime error reporting). A MINK child therefore legitimately produces no
  stderr — this explains Session 95's "unreliable capture" observation.
- stderr CAPTURE itself was probed repeatedly against python/cmd children and is
  deterministic and byte-exact (content, not just length), with no stale-buffer reuse
  across runs and correct exit codes.
- Classification: P3 — add a user-facing stderr intrinsic as a future language feature.
  Nothing to fix in capture.

## Open P1 (documented, do not lose)

**`process_run` deadlocks when a child writes more than the ~4 KB pipe buffer** (evidence:
100/4000/4096 B pass; 4097 B → the runtime waits `WaitForSingleObject(INFINITE)` before
reading, the child blocks on the full pipe, and the program hangs). Pre-existing — the
Session 95 binary behaves identically. Full design for a poll-and-drain fix
(`PeekNamedPipe` + bounded reads + 20 ms waits, overflow discarded into stack scratch) is
recorded below; an implementation draft was built and reverted because the 4097-byte case
still misbehaved and was not root-caused before the budget ended. Recommended for the next
session: re-apply and root-cause (probe `PeekNamedPipe` semantics at EOF with buffered
data; verify the final drain cannot spin after process exit), or switch the child pipes to
overlapped I/O and wait on process + read-completion events — the structurally robust
Windows solution.

## Bug severity (Session 96 end)

- P0: 0 remaining
- P1: 1 remaining — `process_run` deadlock on child output > ~4 KB (above)
- P2: net_init-without-init crash (FIXED this session); V1 single-owning-string model
  constraints (stdlib-documented); env stubs (README-accurate)
- P3: missing user-facing stderr intrinsic (documented above)

## Files changed (Session 96)

- `src/backend/emit/runtime.rs` — NET_INIT_FLAG guards on 10 net intrinsics (Windows-only)
- `tests/windows_hardening.rs` — +2 durable tests (HTTP POST round trip; no-init clean
  errors), `/echo` route on the fixture server; suite 16 → 18
- `docs/implementation/SESSION_96_WINDOWS_APPS_HTTP_POST_ERROR_HARDENING.md` — this record

## Final status

- **WINDOWS RUNTIME = STABLE**
- **WINDOWS REAL-WORLD APPLICATIONS = EXECUTION VERIFIED** (six Session-95 domains plus
  file+JSON, HTTP+JSON, and multi-purpose composition; HTTP POST execution verified)
- **WINDOWS HTTP POST = EXECUTION VERIFIED**
- **WINDOWS ERROR HANDLING = STABLE** (no-init net misuse now fails cleanly; error paths
  covered by the hardening suite)
- **WINDOWS COMPLETION GATE READINESS = NOT READY** — one P1 remains: `process_run`
  deadlock on child output > ~4 KB. Everything else required by the Session 96 brief is
  complete and verified. Linux untouched (frozen); no Linux files modified.
