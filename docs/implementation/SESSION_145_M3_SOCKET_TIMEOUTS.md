# Session 145 — M3 P5: bounded socket connect and read timeouts

**Scope:** the remaining M3 timeout item only — an explicit budget for a TCP
connect and for a blocking receive, so a peer that never answers cannot hold a
MINK process indefinitely. Windows x86_64 is the target; Linux stays frozen
(the new intrinsic answers -1 there, like every other readiness stub). Version
stayed **1.0.4**. No push, tag, publish, merge, rebase, reset, clean, stash or
amend. Every pre-existing dirty file was preserved.

**Outcome:** a MINK program can now perform a connect and a receive with a
caller-supplied millisecond budget. A silent peer that used to hold `net_recv`
until the process was killed is now bounded: 150 ms budget, returned in
157 ms with `net_last_error() == 10060` (WSAETIMEDOUT). Eleven permanent
regressions were added. Full suite: **3,122 passed, 0 failed, 1 ignored**,
exit 0.

Investigating the readiness layer the timeouts wait on exposed three
pre-existing Session 113 defects, all repaired here because the timeout path
cannot be correct without them: the writable event mask was rejected by
`WSAPoll`, `SOCKET_ERROR` returns were not sign-extended, and
`net_set_nonblocking(sock, 0)` branched on stale flags so blocking mode was
never restored.

## 1. Baseline

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `6e88a7bcd62b0e853967d3625e125657f2ec4f2e` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Divergence | ahead 6, behind 1 |
| Staged | none |
| Modified tracked | 39 |
| Untracked | 21 |
| `git diff --check` | clean |
| Version / warnings | **1.0.4** / 59 |
| Prior full suite | **3,111 passed, 0 failed, 1 ignored** (Session 144, shim on `PATH`) |
| Prior `tests/network_lib` | **43 passed, 0 failed** |

The working tree carried the uncommitted Sessions 128–144 work. `git status`,
`git branch -vv`, the hunk layout of every target file and the pre-existing
diffs were inspected before any edit. The target files were already dirty;
their earlier hunks were preserved byte for byte.

## 2. Current API and where blocking occurs

```
MINK source
  -> stdlib/network.mink   net_connect, net_recv (no timeout parameter)
  -> intrinsic              rt_net_connect, rt_net_recv
  -> IR service             RuntimeService::NetConnect / NetRecv
  -> Windows emitter        src/backend/emit/runtime.rs
  -> Winsock                connect(...), recv(...)
```

| Path | Runtime call | Blocking behavior |
| --- | --- | --- |
| `net_connect` | `connect` in `NET_FUNC_TABLE + 32` | waits for the TCP handshake; `getaddrinfo` runs first and is unbounded |
| `net_recv` | `recv` in `NET_FUNC_TABLE + 72` | waits until data, peer close or error; no timeout |
| `net_accept` | `accept` in `NET_FUNC_TABLE + 56` | waits for a connection |

Existing capabilities found and inspected, none of which bounded these calls:

- `rt_net_set_nonblocking` (`ioctlsocket(FIONBIO)`), `rt_net_poll` (`WSAPoll`)
  and the stdlib helpers `net_wait_readable` / `net_wait_writable` /
  `net_can_read` / `net_can_write` (Session 113).
- `rt_net_wsa_set_last_error` / `net_set_last_error` (Session 141, uncommitted):
  the error channel a helper needs when it must preserve a code across
  `closesocket`.
- `getsockopt` was already resolved from `ws2_32.dll` into the extended table
  (`NET_FUNC_TABLE2 + 8`, Session 125) but had no MINK-visible intrinsic —
  without it a non-blocking connect's outcome cannot be confirmed.
- No `SO_RCVTIMEO`/`SO_SNDTIMEO` path exists, and no `setsockopt` resolution.

## 3. Reproduction (pre-fix behavior)

The reproducer needs no external host: a listening socket's backlog completes
the handshake, and the accepted side is never read from, so the client's read
can never finish.

```
fn main() -> Int {
    net_init();
    let listener = net_listen_on("127.0.0.1", 19999, 4);
    let sock = net_tcp_socket();
    if net_connect(sock, "127.0.0.1", 19999) != 0 { return 1; }
    rt_print_int(1);
    let data = net_recv(sock, 64);      // peer never sends
    ...
}
```

```
$ timeout 3 tmp/s145/unbounded.exe
1
UNBOUNDED_EXIT=124        # killed by the watchdog: no bound exists
```

The same program with the new call:

```
$ timeout 20 tmp/s145/bounded.exe
0
10060
157                        # empty Str, WSAETIMEDOUT, elapsed ms
BOUNDED_EXIT=0
```

Before this session the second program could not be written: no
`net_recv_timeout` existed and no combination of the existing API bounded the
read (`net_wait_readable` itself was broken, see §5). Unbounded blocking was
not a missing timeout constant; it was the only behavior.

Connect timeout: a deterministic local "SYN is never answered" peer does not
exist on Windows loopback without privileged network reconfiguration (a
listener completes the handshake from the kernel backlog; a closed port
refuses immediately). No fabrication was attempted. The connect path is
instead tested end to end for the two locally reproducible outcomes (success,
refused) and its waiting step is the same `WSAPoll` code the read-timeout
tests bound (T10 in §8).

## 4. Chosen design

The smallest architecture-consistent capability, not a networking redesign:

1. **Bounded read — stdlib only.** `net_recv_timeout` waits for readability
   with the existing `net_wait_readable` (`WSAPoll`) for the caller's budget
   and calls the existing `net_recv` only when the socket is ready. No new
   runtime service, no per-socket state, no `SO_RCVTIMEO` (which would be
   stateful and would not help connect).
2. **Bounded connect — stdlib plus one intrinsic.** `net_connect_timeout`
   switches the socket to non-blocking, calls the existing `net_connect`
   (immediate `WSAEWOULDBLOCK`), waits for writability with the existing
   `net_wait_writable`, then confirms the handshake with the socket's
   `SO_ERROR`. That confirmation is the only new runtime primitive:
   `rt_net_so_error` → `getsockopt(SOL_SOCKET, SO_ERROR)`, reusing the
   `getsockopt` pointer that has been in the extended table since Session 125.
   `connect` cannot be bounded with `SO_RCVTIMEO`, and `WSAPoll` writability
   alone cannot tell success from failure, which is why this is a separate
   operation rather than one shared "timeout" switch.
3. **No timeout state on the socket.** The budget belongs to the call. Two
   sockets, or two threads, cannot alter each other's budget; there is no
   process-global mutable timeout state, no background thread and no async
   runtime.

Alternatives rejected as larger or incorrect: `setsockopt(SO_RCVTIMEO)`
(new import, stateful per socket, does not bound connect, changes all later
reads); a combined "set socket timeout then connect/read" API (two overlapping
mechanisms); a general event loop (out of scope).

## 5. Root cause of the unbounded behavior and the readiness defects

The read/connect calls themselves are correctly implemented blocking calls;
the gap was the missing bounded operation. While wiring it, the readiness layer
it must wait on turned out not to work at all, which is why the first
implementation returned success for a refused port and left sockets
non-blocking. Three Session 113 defects were reproduced with native probes and
fixed:

1. **`POLLWRBAND` is rejected by `WSAPoll`.** `net_pollout()` returned
   `0x0030` (`POLLWRNORM | POLLWRBAND`). `WSAPoll` answers `SOCKET_ERROR` with
   WSAEINVAL (10022) for that mask, so every writable wait failed.

   ```
   events 0x0030 -> 4294967295, net_last_error 10022
   events 0x0010 -> 1,          net_last_error 0
   events 0x0300 -> 0 (no data yet), net_last_error 0
   ```

   Fixed to `0x0010` (`POLLWRNORM`, i.e. `POLLOUT`).
2. **`SOCKET_ERROR` was not sign-extended.** `WSAPoll` and `ioctlsocket`
   return `int`; only `EAX` is defined. `-1` reached MINK as
   `0x00000000FFFFFFFF`, which is `> 0`, so a failed poll read as "ready"
   (`net_can_write` returned true even while the poll was failing). Added
   `cdqe` to both emitters, the pattern `recv` already used.
3. **`net_set_nonblocking(sock, 0)` never restored blocking mode.** The
   emitter loaded `on` with `mov` (which does not set flags) and branched on
   the caller's stale flags before storing the mode. With `on != 0` the
   not-taken branch still stored 1, so non-blocking mode appeared to work;
   `on == 0` was unreliable. Probe: `set_nonblocking(sock, 0)` returned 0,
   yet the next `recv` returned 0 bytes with 10035. Added an explicit
   `test_rr` before the branch.

These are fixes to the readiness layer the timeout path is built on, not a
change of unrelated networking behavior: before them the writable helpers
never worked and blocked-mode restore was broken. `net_recv`, `net_connect`,
`net_send`, `net_accept`, UDP and HTTP are unchanged and still blocking.

## 6. API contract

All additions are in `stdlib/network.mink` (and the byte-identical npm
mirror).

| Function | Result |
| --- | --- |
| `net_timeout_code() -> Int` | 10060, `WSAETIMEDOUT` |
| `net_timeout_ms(timeout_ms) -> Int` | normalised budget for `WSAPoll` |
| `net_socket_error(sock) -> Int` | pending `SO_ERROR`; 0 = none, -1 = query failed |
| `net_recv_timeout(sock, maxlen, timeout_ms) -> Str` | OWNED Str, as `net_recv` |
| `net_connect_timeout(sock, addr, port, timeout_ms) -> Int` | 0 success, -1 failure |

**Time unit:** milliseconds, the existing `net_poll`/`net_wait_readable`
convention.

**Scope:** each call bounds the operation it names. The read budget bounds the
read only; the connect budget bounds the TCP handshake only (name resolution
runs before it and is not bounded).

**Timeout result:** `net_last_error()` is `10060` (`WSAETIMEDOUT`).
`net_recv_timeout` returns an owned empty Str; `net_connect_timeout` returns
-1. A read error keeps the read's own Winsock code, a peer close leaves 0, and
a refused connect keeps `10061` — a timeout is never conflated with them.

**Ownership:** per call, never per socket and never process-global. Setting a
budget cannot affect another call, another socket or another thread. Nothing
is stored anywhere.

**Default behavior:** unchanged. `net_recv`, `net_connect` and `net_accept`
still block indefinitely; only the new calls bound anything. A successful
`net_connect_timeout` restores blocking mode and leaves `net_last_error()` at
0, so ordinary blocking calls work on the returned socket.

**Zero:** one non-blocking poll (`WSAPoll` timeout 0). A silent socket
therefore returns the timeout result immediately; data that is already
available is returned normally.

**Negative:** waits indefinitely (the `WSAPoll`/`net_poll` convention); every
negative value is normalised to -1, so the contract is "negative = no limit".

**Maximum:** `2147483647` ms (~24.8 days), the largest signed 32-bit value
`WSAPoll` accepts. Larger values are capped to it rather than truncated (a
truncated value could become a short or negative wait). Zero-length requests
(`maxlen <= 0`) return an owned empty Str without waiting, mirroring
`net_recv`.

**Failure state of a timed-out connect:** the socket is left in an
indeterminate state and the caller must close it; the helper never closes a
caller's socket and `net_connect_timeout` consumes `addr` like `net_connect`.
A timed-out read leaves the socket usable (T4).

Both calls clear the last-error at entry — the same rule `http_send` follows —
so the code observed afterwards describes that call rather than a stale
earlier failure.

## 7. Implementation

| Layer | Change |
| --- | --- |
| `src/runtime/intrinsics.rs` | `rt_net_so_error(sock: Int) -> Int` |
| `src/backend/ir.rs` | `RuntimeService::NetSoError`, arity 1, callable |
| `src/backend/lower.rs` | intrinsic-name mapping |
| `src/backend/emit/runtime.rs` | `emit_net_so_error` (`getsockopt`, `NET_FUNC_TABLE2 + 8`, guarded by `NET_INIT_FLAG`); service registration; `cdqe` for `WSAPoll` and `ioctlsocket`; `test_rr` before the non-blocking mode branch |
| `src/backend/emit/linux_runtime.rs` | -1 stub and registration (frozen Linux) |
| `stdlib/network.mink`, npm mirror | the five functions above |
| `tests/network_lib.rs` | bounded runner, local fixtures, n109–n119 |

`emit_net_so_error` builds a 32-byte frame: `[rbp-8]` zero-initialised optval
(so the 8-byte read cannot see stale bytes), `[rbp-16]` optlen = 4, and
`[rbp-24]` the call result. It calls
`getsockopt(sock, SOL_SOCKET (0xFFFF), SO_ERROR (0x1007), &optval, &optlen)`
with the fifth argument at `[rsp+32]`. It returns optval, or -1 when the call
returns `SOCKET_ERROR` or networking is not initialized.

No ABI offset changed (no BSS cell added), no import was added and no IAT index
moved: `getsockopt` was already resolved into the extended table in Session
125. The PE import table and `.bss` layout are byte-identical in shape to the
baseline.

## 8. Regressions

All are in `tests/network_lib.rs` §14. Fixtures are Rust `TcpListener`s on
`127.0.0.1:0` (the OS picks the port), and every program runs under a
wall-clock watchdog that kills the child, so a regression fails instead of
hanging the suite. Bounds are deliberately wide (lower bound well under the
requested wait, upper bound seconds above it).

| Test | T | Asserts |
| --- | --- | --- |
| `n109_connect_timeout_success_and_read_timeout_round_trip` | T1 | connect 0, echo round trip 4 bytes, last error 0 |
| `n110_read_timeout_returns_at_the_budget_with_wsaetimedout` | T2 | empty read, 10060, elapsed >= 100 ms and < 5000 ms |
| `n111_data_before_the_timeout_is_received_normally` | T3 | data sent at 300 ms arrives, last error 0 |
| `n112_repeated_timeouts_on_one_socket_leave_it_usable` | T4 | 3 x 200 ms timeouts all 10060; later read gets "late"; elapsed >= 400 ms |
| `n113_the_timeout_can_be_changed_between_reads` | T5 | 100 ms expires, 8000 ms on the same socket receives |
| `n114_successful_connect_timeout_restores_blocking_mode` | T6 | plain blocking `net_recv` receives data sent 300 ms later, error 0 |
| `n115_zero_negative_and_oversized_timeouts_follow_the_contract` | T7 | 0 polls once (10060, fast), -1 waits for data, `INT64_MAX` is capped and receives |
| `n116_close_after_a_timeout_is_clean` | T8 | timeout then `net_close` 0 and `net_cleanup` 0 |
| `n117_one_sockets_timeout_does_not_affect_another_socket` | T9 | silent socket times out, second socket's 400 ms data still arrives |
| `n118_connect_timeout_reports_the_socket_error_of_a_refused_port` | T10 | -1 with 10061 from `SO_ERROR` |
| `n119_writable_readiness_uses_a_valid_event_mask` | T10b | `net_wait_writable` returns 1 (not -1, not 0xFFFFFFFF), error 0, `net_can_write` true |

Evidence captured during the session (actual runs, not descriptions):

```
first fixed build, focused run:   8 passed, 2 failed
  n114 exit 0  (blocking not restored: recv 0 bytes, 10035)
  n118 exit 0  (refused connect returned success: poll failed, SO_ERROR unread)
probe: net_wait_writable -> 4294967295, net_last_error 10022
probe: net_set_nonblocking(sock,0) -> 0, then recv -> 0 bytes, 10035
final focused run:                11 passed, 0 failed
```

No existing test was changed, skipped, weakened or deleted. The three
readiness defects above are pinned by n114 (restore), n118/n119 (poll and
sign-extension) and by the existing readiness users (httpserver 10/10).

## 9. Verification

| Check | Result | Classification |
| --- | --- | --- |
| `cargo build` | success, 59 warnings (unchanged) | PASS |
| `cargo test --test network_lib` | **54 passed, 0 failed** (43 + 11) | PASS |
| `cargo test --test http_lib` | **49 passed, 0 failed** | PASS |
| `cargo test --test threads_lib` | **22 passed, 0 failed** | PASS |
| `cargo test --test httpserver_lib` | **10 passed, 0 failed** | PASS |
| Unbounded reproducer | prints `1`, killed at 3 s, exit 124 | PASS (evidence) |
| Bounded reproducer (debug) | `0` bytes, `10060`, 157 ms, exit 0 | PASS |
| Bounded reproducer (release compiler) | `0` bytes, `10060`, 156 ms, exit 0 | PASS |
| Full suite `cargo test --no-fail-fast`, shim on `PATH` | **3,122 passed, 0 failed, 1 ignored**, exit 0 | PASS |
| `cargo build --release` | success, 59 warnings, 21.20 s | PASS |
| `git diff --check` | clean (exit 0) | PASS |
| Orphan processes after the probes | none; fixtures are joined or killed | PASS |

The full-suite total is Session 144's **3,111** plus these eleven tests. The
one ignored test is the pre-existing one. The shim (`tmp/m3_print/bin/python3.exe`
first on `PATH`) is the Session 142–144 environment condition; without it the
TLS fixture cannot start.

## 10. Error semantics audit

| Concern | Result |
| --- | --- |
| `net_last_error()` | 10060 means timeout; 10061 refused; 0 no error/peer close; other Winsock codes pass through unchanged. Both new calls clear at entry and only publish a code that belongs to this call. |
| `http_last_error()` | Untouched. `stdlib/http.mink` still uses `net_recv` (blocking) and its own per-thread codes; no HTTP path calls the new functions, so no HTTP code can be conflated with a socket code. |
| Success paths | A successful read leaves 0; a successful connect restores blocking mode and leaves 0. |
| Cleanup paths | `net_close` after a timeout is clean (n116). `net_connect_timeout` never closes the caller's socket; `net_dial` is unchanged and still republishes its own error. |
| Before `net_init` | `net_set_last_error` is a guarded no-op, `net_wait_readable`/`net_socket_error` answer -1, so the new calls fail closed (-1 / empty Str) without faulting. Existing pre-init behavior is unchanged. |

## 11. Audit

| Area | Result |
| --- | --- |
| Architecture | Read timeout is stdlib-only over the existing readiness layer; connect adds one minimal `SO_ERROR` accessor. No second timeout mechanism, no `setsockopt`/`SO_RCVTIMEO`, no async runtime, no new imports, no BSS growth. |
| Thread safety | No shared mutable timeout state. The budget is an argument; `SO_ERROR` and `WSAPoll` are per-socket/per-thread Winsock operations. Two sockets with different budgets are proven independent (n117). |
| Resource safety | The poll array is one `rt_alloc`/`rt_free` pair inside the existing helper; `SO_ERROR` uses stack space only; no handles, threads or allocations are created by the new code; the caller owns and closes every socket. |
| Error safety | Timeout, refusal, peer close and read failure are distinguishable; success clears the entry error; existing error producers are untouched. |
| Windows / ABI | x86_64 Win64 calls only (`getsockopt` 5-argument layout, 32-byte shadow space). No IAT index, import or `.bss` offset changed. `cdqe` makes the documented `int` returns readable as signed values. |
| Regression safety | `net_recv`, `net_connect`, `net_send`, `net_accept`, UDP, HTTP and TLS tests are all green; the blocking calls still block (proven by the unbounded probe). |
| Scope | Only the connect/read timeout item and the readiness defects it depends on. No UDP buffer parity, chunked decoding, parser, print, branding, version or Linux work. |

## 12. Known limitations

1. The connect budget bounds the handshake, not `getaddrinfo` name resolution.
2. A deterministic local "SYN never answered" peer is not reproducible on
   Windows loopback without privileged network configuration, so the connect
   timeout's expiry branch is not covered end to end; the same `WSAPoll` wait
   is covered by the read tests, and the connect result paths (success and
   refusal via `SO_ERROR`) are covered.
3. A failed or timed-out connect leaves the socket indeterminate; the caller
   must close it (documented, not tested with a dedicated fixture because the
   local outcomes are refused/success).
4. `net_accept` has no dedicated timeout helper. `net_wait_readable` already
   bounds accept readiness; that path is unchanged and untested here.
5. A poll-then-recv sequence assumes a socket has one owner; concurrent reads
   on the same socket by several threads can still race (pre-existing socket
   ownership model, unchanged).
6. Timer granularity is the Windows scheduler tick; the tests use budgets of
   100 ms or more and wide bounds.
7. Linux keeps -1 stubs (frozen platform).
8. The readiness repairs change Session 113 behavior: `net_wait_writable`
   and `net_can_write` now actually work instead of failing with WSAEINVAL
   while reporting ready. This is a defect fix required by the timeout path.
9. The full suite needs the Python shim on this machine.

## 13. Files changed

- `src/runtime/intrinsics.rs` — `rt_net_so_error`
- `src/backend/ir.rs` — `NetSoError` service
- `src/backend/lower.rs` — mapping
- `src/backend/emit/runtime.rs` — SO_ERROR emitter and registration, poll
  sign-extension, non-blocking mode branch fix, ioctlsocket sign-extension
- `src/backend/emit/linux_runtime.rs` — stub and registration
- `stdlib/network.mink`, `npm/mink/stdlib/network.mink` — contract and helpers
- `tests/network_lib.rs` — bounded runner, fixtures, n109–n119
- `docs/architecture/CROSS_PLATFORM_RUNTIME_ARCHITECTURE.md` — intrinsic table
- `docs/implementation/RUNTIME_IMPLEMENTATION.md` — bounded socket section
- this record

## 14. Git

**No commit was created.** The change cannot be isolated without absorbing
Sessions 128–144 work:

- `stdlib/network.mink` already carried uncommitted Session 141 hunks
  (`net_set_last_error`, `net_dial` error preservation). HEAD's copy has no
  `fn net_set_last_error`, and the new timeout helpers call it, so a commit of
  only this session's hunks would not compile. Separating them would require
  reconstructing Session 141's work into this commit or committing a
  half-converted library.
- `tests/network_lib.rs` carries 236 lines of uncommitted Session 142/144
  tests (n103–n108) before this session's 573 lines; `n114`–`n119` depend on
  the new stdlib, which depends on that Session 141 work.
- `src/backend/emit/runtime.rs` carries uncommitted Session 134/141/144 work
  (including the extended ws2_32 table's `getsockopt` resolution and the
  `WSASetLastError` service). The SO_ERROR emitter is not meaningful without
  the table; a HEAD-based commit would have to include those hunks.

The verified work remains in the working tree. No reset, rebase, stash,
checkout, clean, amend, merge, push, tag or publish was performed.

Final state:

- HEAD: `6e88a7bcd62b0e853967d3625e125657f2ec4f2e` (unchanged);
  `origin/main` `b860837a8a6039f304a503e3d85728a643ab470b`; ahead 6, behind 1.
- Staged: none. Modified tracked: **39** (all were already modified; this
  session changed ten of them: eight source/test files and two docs).
  Untracked: **22** (21 baseline + this record). `git diff --check`: clean.
  Version **1.0.4**.

## 15. Remaining M3 work

1. UDP buffer parity.
2. HTTP chunked decoding.
3. `parse_block_expr` trailing-token recovery (Session 140 §9.3).
4. `rt_print_str` single-write line atomicity (print-adjacent).
