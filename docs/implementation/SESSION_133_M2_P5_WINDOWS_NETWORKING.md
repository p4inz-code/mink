# Session 133 — M2 P5: Windows networking reliability

**Scope:** validate the existing Windows x86_64 networking stack under local
workloads: hostname/IP TCP, plain HTTP by hostname, HTTPS regression, failure and
timeout behavior, repeated connections, and concurrency. Validation-first; no
new networking API. Linux was not touched; the version stays 1.0.4.

**Outcome:** the previously reported "plain HTTP by hostname" failure is **not
reproduced** — hostname TCP and plain HTTP work end to end. The session did find
two genuine concurrency defects in the Windows runtime: concurrent first-time
`net_init` could corrupt the Winsock function table and segfault, and
`net_recv` used one shared BSS buffer, so concurrent receives returned each
other's bytes (and a 4096-byte receive overran that buffer by 8 bytes). Both
were fixed in the Windows emitter with a permanent regression test. One
additional library contract defect (`net_dial` loses `net_last_error`) was
confirmed and is recorded, not fixed, because the minimal repair needs a new
ABI setter.

## 1. Baseline

Read before probing:

- `stdlib/network.mink`: `net_connect` documents hostname support through
  `getaddrinfo`; `net_dial` calls `net_init` + `net_tcp_socket` + `net_connect`
  and documents that on failure "net_last_error() holds the reason";
  `net_recv` returns an owned `Str`; `net_wait_readable(sock, timeout_ms)` exists
  for readiness timeouts; there is no connect or read timeout parameter.
- `src/backend/emit/runtime.rs`: `emit_net_connect` resolves both literals and
  names with `getaddrinfo` and publishes resolver failures through
  `WSASetLastError`; `emit_net_wsa_startup` stages ws2_32 entry-point names and
  WSADATA in the shared `recv_buf`; `emit_net_recv` received into
  `recv_buf + 8` and then copied into an exact-length `Str`.
- `stdlib/http.mink`: `http_send` calls `net_init` itself (Session 127), so the
  HTTP client is a standalone entry point; `http_last_error_code` is a
  module-level global.
- `stdlib/tls.mink`: resolves the host with `rt_net_getaddrinfo` before the
  socket connect and matches the certificate against the original host name.

Existing focused suites (current tree):

| Suite | Result |
| --- | --- |
| `tests/network_lib.rs` | 36 passed before this session's test (37 after) |
| `tests/http_lib.rs` | 39 passed (includes Session 132's 4 MiB body test) |
| `tests/tls_lib.rs` | 19 passed, 1 ignored |

The TLS suite initially failed in this environment because the harness spawns
`python3`, which here is the Microsoft Store alias stub, not Python. With a
`python3.exe` launcher shim in `tmp/m2_p5/bin` (a copy of the working
`python.exe`, prepended to `PATH` for the run) all 19 tests pass. That is a test
harness/environment assumption, not a MINK defect; the source and harness are
unchanged.

Prior M1/M2 finding checked here: "Plain HTTP by hostname broken" (the reason
this session singles out hostname behavior). Session 129 already recorded
`net_resolve("localhost", 80) → 127.0.0.1` and plain HTTP green; this session
re-verified it end to end against a deterministic loopback server.

## 2. Hostname / TCP behavior

Deterministic loopback fixtures: a Python HTTP server and a TCP echo server on
`127.0.0.1` (ports recorded in `tmp/m2_p5/ports.txt`). Probe transcript:

| Case | Result | Time |
| --- | --- | --- |
| `net_dial("127.0.0.1", http_port)` | socket 228 | — |
| `net_dial("localhost", http_port)` | socket 228 | — |
| `net_resolve("localhost", 80)` | `127.0.0.1` (9 bytes) | — |
| `net_resolve("LOCALHOST", 80)` | `127.0.0.1` | — |
| `net_resolve("127.0.0.1", 80)` | literal returned unchanged | — |
| `net_resolve("no-such-host.invalid", 80)` | empty string | — |
| Echo through `net_dial("localhost", echo_port)` | sent 9, received 9, bytes equal | — |
| `net_connect` invalid name | `-1`, `net_last_error() == 11001` | ~47 ms |
| `net_connect` refused loopback port | `-1`, `net_last_error() == 10061` | ~2.03 s |
| `net_connect` `192.0.2.1:81` | `-1` after OS SYN retries | ~21.0 s |
| `net_hostname()` | `MYSTERIOUSPUNK` (14 bytes) | — |

The ~2 s refused-connect latency is the operating system, not MINK: a bare
Python `socket.connect` to closed loopback ports on this machine takes
2022-2037 ms for every port tested. The 21 s unreachable case is the default
Windows TCP SYN retry schedule; the API exposes no connect timeout. Neither
hangs and neither reports success.

**Confirmed contract defect (class B):** `net_dial` returns `-1` but
`net_last_error()` is then `0`, while a direct `net_connect` reports
`11001`/`10061` correctly. Root cause: `net_dial` closes the socket on the
failure path, and `closesocket` clears the thread's Winsock last-error. The
library documents that `net_last_error()` holds the reason after `net_dial`, so
the contract is wrong. A minimal fix needs either a `WSASetLastError` intrinsic
or an API restructure; that is compiler/ABI work outside this validation
session's fix boundary, so it is recorded for M3. Workaround: use `net_connect`
on an owned socket when the error code matters.

## 3. Plain HTTP (hostname vs IPv4)

Deterministic endpoint: `/small` returns `MINK-M2P5-HTTP-OK` (17 bytes),
`/pattern/4096` returns the byte pattern `b[i] = (i*131+17) & 0xFF`.

| Request | Status | `http_last_error()` | Body | Content |
| --- | --- | --- | --- | --- |
| `http://localhost:<port>/small` | 200 | 0 | 17 B | exact, SHA-256 `c2629c0e…` |
| `http://127.0.0.1:<port>/small` | 200 | 0 | 17 B | exact, same SHA |
| `http://localhost:<port>/pattern/4096` | 200 | 0 | 4096 B | exact, first 17 / last 142 |

The old audit's "hostname HTTP silently returns an empty response with exit 0"
is **not reproducible** (class E). `net_connect` resolves names, and
`http_send` passes the host name through unchanged; the response is complete and
byte-identical to the IPv4 request.

## 4. HTTPS regression

The full `tests/tls_lib.rs` suite passes (19/19 runnable; the public-internet
case remains `#[ignore]`), including the pinned-CA, SAN, wildcard, expired,
wrong-name, wrong-usage, untrusted-root and large-body cases. A direct probe
against `tests/tls/server.py` with the committed fixtures additionally showed:

| Case | Result |
| --- | --- |
| `https_client_get_with("localhost", good_port, …)` with `ca.pem` | 200, body 220 B, byte-identical to `tlsroot/index.html` |
| wrong-name certificate | rejected, `tls_err = 1005`, status `CERT_E_CN_NO_MATCH` 2148204815 |
| expired certificate | rejected, `tls_err = 1005`, status `CERT_E_EXPIRED` 2148204801 |
| untrusted root | rejected, `tls_err = 1005`, status `CERT_E_UNTRUSTEDROOT` 2148204809 |

No new runtime error appeared. The TLS path still resolves the host itself and
matches the certificate against the host name, independent of the socket
address, and the `net_recv` change in §8 keeps the same owned-`Str` contract.

## 5. Failure and timeout behavior

| Case | Behavior |
| --- | --- |
| Invalid host name | explicit `-1`, `11001`, ~47 ms, no hang |
| Refused loopback port | explicit `-1`, `10061`, ~2.03 s (OS), no hang |
| Blackholed `192.0.2.1:81` | explicit `-1` after ~21 s (OS SYN retries), no hang, no false success |
| Silent peer: `net_wait_readable(sock, 500)` | returns 0 after 500-515 ms |
| Silent peer: `net_wait_readable(sock, 1500)` | returns 0 after 1500-1516 ms |

There is no connect-timeout or receive-timeout option in the socket API:
`net_connect`/`net_recv` block on the OS defaults, and callers must use
non-blocking mode plus `net_wait_readable` themselves. This is a capability gap
(class D), not a defect; the documented readiness timeout is honored exactly.

## 6. Repeated connections

| Workload | Result |
| --- | --- |
| 20,000 hostname HTTP GETs + 20,000 TCP echo round-trips (before the `net_recv` fix) | 0 failures; WS plateau ~96 MB; handles constant at 100 for the whole run (~51 s) |
| 5,000 + 5,000 (after both fixes) | 0 failures; WS plateau 46.6 MB; handles 100 → 101 and stable |
| 100 refused connections, then one successful echo connect | all 100 explicit failures, final connect succeeds; 0 accounting failures |
| 300 HTTP + 300 echo + 100 refused (single process) | 0 failures; the 100 refusals dominate runtime at ~2 s each (3 m 23 s total) |

No socket-handle growth, no stale state, no progressive memory growth after the
first plateau, and repeated failures do not poison later successful connects.

## 7. Concurrency and the two runtime defects

TCP echo concurrency (8 threads, 50 connections each, five processes) was
clean: 2,000 connections, 0 failures.

Hostname HTTP concurrency (8 threads, 10 requests each) uncovered two separate
defects.

**A1 — concurrent first-time `net_init` could segfault (4/20 processes, exit
139).** `http_send` and `net_dial` initialize Winsock implicitly. The init
sequence stages the 20 ws2_32 entry-point names and the 408-byte `WSADATA`
block in the **shared** BSS `recv_buf`, then calls `GetProcAddress`. Two threads
entering together overwrite each other's names between resolutions; a later
`GetProcAddress` reads garbage and stores a null pointer in the function table,
and the next call through it faults. Calling `net_init()` once before spawning
the threads made 10/10 runs clean, confirming the root cause.

**A2 — concurrent `net_recv` used one shared destination buffer.** `net_recv`
received into `recv_buf + 8` and only then copied the bytes into a fresh `Str`.
Two threads receiving at once could overwrite each other between the receive and
the copy; with the init fix alone the new regression test failed intermittently
with wrong bodies (exit 2). The same code also wrote up to `maxlen` bytes at
`recv_buf + 8`, so the HTTP client's `net_recv(sock, 4096)` overran the
4096-byte buffer by 8 bytes on **every** response, into the neighbouring
`bcrypt_dll_handle` BSS slot.

After both fixes: 40/40 and then 20/20 processes clean, all eight workers
reporting 0 failures, and the permanent 25-process regression test passes.

## 8. Fixes

All in `src/backend/emit/runtime.rs` (Windows emitter; no stdlib or Linux
change):

1. `emit_net_wsa_startup`: acquire the runtime spin lock before the
   `NET_INIT_FLAG` check and release it on every return path (`already`,
   `wsa_fail`, `dll_fail`). Initialization is now process-wide one-time and
   atomic; the second thread sees the flag and skips the scratch sequence.
   Termination and failure behavior are unchanged.
2. `emit_net_recv`: allocate the destination `Str` first (`StrAlloc(maxlen)`),
   `recv` directly into its data area, and write the actual byte count into the
   length prefix (0 on close/error; a non-positive `maxlen` returns an empty
   owned `Str`). This removes the shared buffer, the second copy, the
   concurrent-receive race, and the 8-byte overrun. Callers' contract is
   unchanged: an owned `Str` whose length is the number of bytes received.

`tests/network_lib.rs` gained
`n100_concurrent_implicit_net_init_is_serialized` (Windows-gated): 8 threads,
10 hostname HTTP requests each, no pre-init, 25 fresh processes against a Rust
loopback server, asserting exact 17-byte bodies and exit 0. It fails against the
pre-fix emitter (wrong bodies/crash) and passes now.

## 9. Classification

| Finding | Class | Action |
| --- | --- | --- |
| Plain HTTP by hostname | **E** — not reproduced; works end to end | none |
| Concurrent implicit `net_init` segfault | **A** — correctness defect | fixed + regression test |
| Concurrent `net_recv` wrong payloads / 8-byte BSS overrun | **A** — correctness defect | fixed |
| `net_dial` clears `net_last_error` on failure | **B** — reliability/contract defect | recorded; needs an ABI setter, not fixed |
| No connect/receive timeout option | **D** — capability gap for M3 | recorded |
| `http_last_error_code` is a process-global, not per-thread | **B** — concurrent callers can observe another request's error | recorded |
| Refused connect ~2 s and unreachable ~21 s | **C** — OS/Windows behavior, explicit errors | recorded |
| TLS suite needs `python3` on `PATH` | **E** — test-environment issue | documented, harness unchanged |

## 10. Tests run

- `tests/network_lib.rs`: **37 passed** (36 existing + the new `n100`).
- `tests/http_lib.rs`: **39 passed**.
- `tests/tls_lib.rs`: **19 passed, 1 ignored** (with the `python3` shim).
- Pre-fix evidence: `n100` failed (`Some(2)`, wrong bodies); 4/20 concurrency
  processes crashed with exit 139.
- Post-fix evidence: `n100` passes; 40/40 then 20/20 concurrency processes
  clean; 5,000+5,000 repeated requests clean.

## 11. Remaining limitations and gaps

- `net_dial` still loses the socket error on failure; use `net_connect` when
  the code matters. Fixing it needs `WSASetLastError` exposed to the stdlib (or
  a restructured failure return).
- No connect or receive timeout API; `net_wait_readable` is the only bounded
  wait primitive. Unreachable addresses block for the OS default (~21 s).
- `net_recvfrom` still uses the shared BSS buffer and was not exercised
  concurrently; UDP concurrency may have the same data race.
- `http_last_error_code` is global; per-thread error state is not available.
- `emit_net_wsa_startup` now serializes on the runtime spin lock; a caller that
  invoked `net_init` while already holding that lock would deadlock (no current
  call path does).
- HTTPS was exercised over loopback with the committed CA fixtures, not the
  public internet; the public case remains ignored by design.
- Concurrency was 8 threads on loopback with small bodies; large concurrent
  bodies and many hundreds of threads were not measured.

## 12. Environment note (build toolchain)

`cargo build --release` on this machine currently aborts inside `rustc` with
`STATUS_ILLEGAL_INSTRUCTION` (0xc000001d) or `STATUS_ACCESS_VIOLATION`
(0xc0000005). It does so for the **unmodified** tree too (verified by restoring
`src/backend/emit/runtime.rs` from `HEAD`), so it is not caused by this
session's changes; the debug profile builds and runs correctly. All suites and
probes above were therefore run with the debug compiler (`target/debug/mink.exe`,
built from the final sources). Release verification remains pending on a
healthy toolchain/environment.

## 13. Repository state and audit

- Intended changes: `src/backend/emit/runtime.rs`, `tests/network_lib.rs`, and
  this document. No stdlib, no Linux file, no version/tag change.
- Probes and the Python `python3` shim live under `tmp/m2_p5/` and are ignored
  (`git check-ignore` → `.gitignore:14:tmp/`).
- Pre-existing modified and untracked files (Session 128-132 work, the roadmap
  archive, `nl1.*`, the untracked `httpserver` files, the pasted transcript)
  are untouched.
- `Cargo.toml` and `npm/mink/package.json` remain **1.0.4**; tags remain
  `v1.0.0`–`v1.0.4`; no npm/release/publish/tag command was run; nothing was
  committed.

## 14. Recommendation for M2 P6

Networking is now practically usable for the validated local cases. The next
M2 area is server workloads or integrated applications (the roadmap's next
uncovered items), not more one-shot network capacity. Carry these M3
candidates forward from this session:

1. Preserve the socket error across `net_dial` (add a `WSASetLastError`
   intrinsic or return the error), so the documented contract holds.
2. Add explicit connect/receive timeout options, or a documented
   `net_wait_*`-based recipe, so blackholed peers cannot block for 21 s.
3. Make the HTTP last-error state thread-local (or make the concurrency
   contract explicit), and give `net_recvfrom` the same per-call buffer
   treatment as `net_recv`.
