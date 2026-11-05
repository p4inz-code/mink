# Session 146 — M3 P6: UDP buffer parity

**Scope:** the documented "UDP buffer parity" item only. Windows x86_64 is the
target; Linux stays frozen. Version stayed **1.0.4**. No push, tag, publish,
merge, rebase, reset, clean, stash or amend.

## 1. Baseline

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `6e88a7bcd62b0e853967d3625e125657f2ec4f2e` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Version | `1.0.4` |
| Working tree | 39 modified tracked, 22 untracked (Sessions 128–145) |
| Full suite before | **3,122 passed / 0 failed / 1 ignored** |
| `tests/network_lib` before | **54 passed / 0 failed** |
| `tests/http_lib` / `tests/threads_lib` / `tests/httpserver_lib` | 49 / 22 / 10, all passing |

Session 145 (bounded connect/read timeouts) was the immediately preceding task
and is unchanged by this session. See
`docs/implementation/SESSION_145_M3_SOCKET_TIMEOUTS.md`.

## 2. Current UDP architecture

```
MINK:        net_recvfrom(sock, maxlen) -> Str
stdlib:      stdlib/network.mink  (published copy npm/mink/stdlib/network.mink)
intrinsic:   rt_net_recvfrom       (src/runtime/intrinsics.rs)
emitter:     emit_net_recvfrom     (src/backend/emit/runtime.rs)
Winsock:     recvfrom              (ws2_32 table slot NET_FUNC_TABLE2 + 24)
```

`rt_net_recvfrom` is emitted directly as machine code by the Windows x86_64
backend; there is no MIR/IR node or lowering step of its own, and no new
intrinsic was needed. `net_sendto` is the analogous send path. The sender
accessors (`net_sender_ip` / `net_sender_port`) read the 16-byte
`NET_SENDER_SOCKADDR` cell written by the receive.

## 3. The exact parity gap

The receive destination was the **shared BSS scratch buffer** `recv_buf`
(`NET_RECV_BUF`, 4,088 data bytes), and `maxlen` was clamped to that constant:

- any IPv4 datagram larger than 4,088 bytes failed with **WSAEMSGSIZE (10040)**
  and returned an empty `Str`, even when the caller asked for 8,192 bytes or
  more;
- the result was then copied out of the shared buffer, so two sockets (or two
  threads) receiving concurrently could overwrite each other's datagram
  between `recvfrom` and the copy;
- `net_recvfrom(sock, 0)` clamped **up** to 4,088 and silently consumed a
  datagram, unlike `net_recv`, where a non-positive request receives nothing.

This was not a protocol limit: IPv4 UDP allows 65,507 data bytes, and the
backend already declared `NET_DATAGRAM_MAX = 65507`. It was an internal scratch
buffer standing in for a caller-sized destination.

The old `tests/network_lib::n99` asserted the clamp as if it were the contract
("a nonsensical request is clamped up to the buffer"), which is why the gap had
stayed invisible to the suite.

## 4. Reproduction (pre-fix)

A single debug binary, one bound socket on `127.0.0.1`, no timing dependence:

```
send 4096 bytes -> net_recvfrom(sock, 8192) -> len 0,   net_last_error 10040
send 65507 bytes -> net_recvfrom(sock, 65507) -> len 0,   net_last_error 10040
send 3 bytes     -> net_recvfrom(sock, 0)     -> len 3,   net_last_error 0
```

Ephemeral binding (port 0) works, so permanent tests need no port coordination
and cannot collide across parallel test binaries.

## 5. Root cause

`emit_net_recvfrom` used `NET_RECV_BUF` as the receive destination and as the
copy source, capping `maxlen` at its size. The allocation/copy geometry — not
the Winsock call, not the length type, not truncation semantics — was the
defect.

## 6. Fix

`src/backend/emit/runtime.rs`, `emit_net_recvfrom` only:

1. `maxlen <= 0` returns an owned empty `Str` **without receiving**, matching
   `rt_net_recv`; the datagram stays queued and the error is untouched.
2. A positive `maxlen` is capped at `NET_DATAGRAM_MAX` (65,507) and that value
   is passed to `StrAlloc`.
3. `recvfrom` writes **directly into the allocated Str's data area**
   (`str + 8`), so there is no shared scratch buffer and no copy loop.
4. The Str's length prefix is set to the actual received length (0 on error or
   for a zero-length datagram), and the Str is returned.

`stdlib/network.mink` and `npm/mink/stdlib/network.mink` document the contract;
the two files are byte-identical (`cmp` clean). Nothing else changed: no
intrinsic, IR, lowering, import-table, BSS-layout or ABI change.

## 7. Contract

| Question | Answer |
| --- | --- |
| Unit of `maxlen` | bytes, the receive buffer — same meaning as `net_recv`'s |
| Maximum destination | 65,507 bytes (largest IPv4 UDP payload); larger requests cap here |
| Datagram larger than `maxlen` | empty `Str`, `net_last_error() == 10040` (WSAEMSGSIZE). Winsock does not partially deliver a datagram; no truncated bytes are returned |
| Zero-length datagram | owned empty `Str`, `net_last_error() == 0` |
| `maxlen == 0` / negative | owned empty `Str`, no receive, error untouched (datagram stays queued) |
| Embedded NUL / binary | preserved exactly; the length prefix is authoritative, no terminator is added |
| Repeated receives | each call allocates a fresh exact-size `Str`; no stale bytes from a previous datagram |
| Concurrency | no shared receive buffer; sockets and threads are isolated |
| Sender helpers | unchanged: `net_sender_ip` / `net_sender_port` describe the last datagram |
| Default behaviour | blocking, as before; no timeout was added or changed here |

## 8. Tests

`tests/network_lib.rs`:

- `n99` renamed to `n99_udp_recvfrom_maxlen_is_the_receive_buffer` and updated
  to the real contract (short read still `10040`; request past the old buffer
  reads whole; `maxlen 0` receives nothing with error 0).
- New Section U (11 tests):
  - `u01_udp_small_datagram_round_trip_is_byte_exact`
  - `u02_udp_large_datagram_above_the_old_buffer_is_received_whole` (4,096 B)
  - `u03_udp_boundary_at_the_old_4088_limit` (4,087 / 4,088 / 4,089)
  - `u04_udp_embedded_nul_and_binary_bytes_are_preserved`
  - `u05_udp_repeated_datagrams_of_different_sizes`
  - `u06_udp_zero_length_datagram`
  - `u07_udp_two_independent_sockets_keep_their_own_datagrams`
  - `u08_udp_send_and_receive_error_paths`
  - `u09_udp_cleanup_after_successful_receive`
  - `u10_udp_cleanup_after_truncation_failure`
  - `u11_udp_max_ipv4_payload_round_trip` (65,507 B)

Each U test sends real datagrams from a loopback peer thread and compares the
**actual bytes and returned lengths**, not just exit codes. A UDP peer helper
(`udp_peer` / `udp_round_src` / `assert_udp_round`) keeps the cases
deterministic; there is no sleep-based synchronization.

### Post-fix probe (debug and release)

```
send 4096  -> recv maxlen 8192  -> len 4096, err 0
send 65507 -> recv maxlen 65507 -> len 65507, err 0
send 3     -> recv maxlen 0     -> len 0,    err 0   (datagram stays queued)
```

## 9. Verification

| Check | Result |
| --- | --- |
| `cargo test --test network_lib` | **65 passed / 0 failed** (54 + 11) |
| `cargo test --test http_lib` | **49 passed / 0 failed** |
| `cargo test --test threads_lib` | **22 passed / 0 failed** |
| `cargo test --test httpserver_lib` | **10 passed / 0 failed** |
| `cargo test --no-fail-fast` (python3 shim) | **3,133 passed / 0 failed / 1 ignored**, exit 0 |
| `cargo build --release` | exit 0, finished in 19.03 s, **59 warnings** (existing baseline) |
| Release-compiled UDP reproducer | same post-fix output as debug, exit 0 |
| `git diff --check` | clean, exit 0 |
| Published stdlib copy | `cmp` clean against `stdlib/network.mink` |

The first release attempt failed inside the host toolchain (`rustc.exe` exited
`0xc000001d`, `STATUS_ILLEGAL_INSTRUCTION`) before code generation — the same
intermittent host crash seen on test-crate compilation. The retry completed
normally; no source change was involved.

Suite math: 3,122 + 11 new UDP regressions = **3,133**; the single ignored test
is unchanged.

## 10. Audit

| Area | Finding |
| --- | --- |
| Buffer allocation | exactly `min(maxlen, 65507)`; no fixed-size destination remains in `recvfrom` |
| Copy length | no copy at all; the destination is the returned `Str` |
| Length prefix | written from `recvfrom`'s return value, 0 for error/zero-length |
| Capacity check | `maxlen <= 0` rejected before allocation; unsigned cap comparison at 65,507 |
| Signedness | `recvfrom`'s `int` is sign-extended with `cdqe` before the `SOCKET_ERROR` test; lengths are non-negative afterwards |
| NUL / terminator | no terminator is written; total Str allocation is the only bound and embedded NULs are data |
| sockaddr sizes | `NET_SENDER_SOCKADDR` 16 bytes and `fromlen = 16` unchanged |
| Lifetime | one owned Str per call, returned to the caller; nothing points into BSS |
| Repeated reuse | no scratch buffer to reuse or clear |
| Concurrency | no shared mutable state on the receive path |
| Cleanup | existing `net_close` / `net_cleanup` path unchanged; U9/U10 exercise success and truncation followed by close, including the exit-time leak scan |
| ABI / imports | no new import, service, BSS cell or offset change |
| Windows only | only the Windows emitter changed in this session; no Linux UDP capability was added or expanded. `src/backend/emit/linux_runtime.rs` shows as modified in Git but its working-tree change is pre-existing (mtime 21:59, Sessions 128–145) and was not edited here |
| Out of scope | no UDP timeout, multicast, IPv6, async, HTTP chunked, parser, print, branding or version work |

## 11. Known limitations

- A datagram larger than the caller's `maxlen` is still reported as
  `WSAEMSGSIZE (10040)` with no partial bytes. That is Winsock's contract and
  is now documented rather than silently triggered at 4,088 bytes.
- `net_recvfrom` remains blocking and has no timeout argument. Session 145
  bounded TCP connect/read; extending that budget to UDP is a separate item
  and was not attempted here.
- `maxlen == 0` now receives nothing (previously it silently consumed a
  datagram). This aligns UDP with `net_recv` and is covered by `n99` and U6.

## 12. Git state and commit decision

| Item | Value |
| --- | --- |
| HEAD | `6e88a7bcd62b0e853967d3625e125657f2ec4f2e` (unchanged) |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Staged | none |
| Modified tracked | 39 |
| Untracked | 23 (22 pre-existing + this record) |
| Version | `1.0.4` (unchanged) |

**No commit.** The UDP change cannot be isolated from the uncommitted tree:
`src/backend/emit/runtime.rs` and `tests/network_lib.rs` also carry Sessions
142/144/145 work, and `stdlib/network.mink` + the published copy carry the
Session 145 timeout helpers. Any file-level staging would absorb unrelated
work, and a hunk-level split would produce a compiler tree that references
helpers absent from HEAD. The verified work therefore remains in the working
tree, exactly as Session 145 left its work.

## 13. Remaining M3

1. HTTP chunked decoding
2. `parse_block_expr` trailing-token recovery
3. `rt_print_str` single-write line atomicity
