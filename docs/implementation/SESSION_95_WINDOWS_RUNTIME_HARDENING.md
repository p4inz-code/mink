# Session 95 — Windows Runtime Expansion, Hardening, Ownership, and Multi-Purpose Quality

- Date: Session 95 (after `7e960d1` = Session 94)
- Scope: Windows x86_64 only. Linux frozen — no Linux files modified.
- Verdict: **WINDOWS RUNTIME = STABLE**, **WINDOWS MULTI-PURPOSE EXECUTION = EXECUTION VERIFIED**,
  **WINDOWS P0/P1 STATUS = NO P0, P1 OWNERSHIP BUGS FIXED**

---

## 1. Ownership fixes (cross-platform contract restored)

Session 93 fixed the same leak class on Linux. This session proved the **Windows stdlib still had the
equivalent bugs** and fixed them so both platforms share one contract.

Root cause (same on both platforms): V1 user functions **consume** their `Str` parameters; a wrapper that
takes a heap `Str`, passes it to a runtime intrinsic, and never frees it leaks. Callers cannot free after a
call because the value is moved. Fix pattern (canonical since Session 93): store the wrapper result, free the
consumed parameter at a single textual exit, return the result.

Verified by execution probes (heap strings, exit codes, leak checker E-R06):

| Function | Bug | Fix |
|---|---|---|
| `fs_read`/`fs_write`/`fs_copy_file`/`fs_move`/`fs_remove_file`/`fs_create_dir`/`fs_remove_dir`/`fs_set_cwd`/`fs_is_file`/`fs_is_dir`/`fs_file_size_or` | consumed `Str` param never freed (E-R06 with heap args) | single-exit free of consumed params |
| `path_join`/`path_parent`/`path_filename`/`path_extension`/`path_stem`/`path_with_extension`/`path_normalize`/`path_has_extension`/`path_is_absolute`/`path_is_relative` | same leak class; `path_has_extension` additionally leaked its heap `pe` result | free consumed params + free `pe` |
| `net_connect`/`net_bind`/`net_send`/`net_resolve` | same leak class | free consumed params |
| `json_parse` | consumed the input document; heap docs leaked E-R06 | single-exit free of the consumed document |

Files: `stdlib/filesystem.mink`, `stdlib/network.mink`, `stdlib/json.mink`.
Verified on Windows by the multi-purpose programs and the hardening suite below; `stdlib/json.mink` and
`stdlib/network.mink` changes were cross-checked against the Linux-verified Session 92/93 behavior.

## 2. New durable suite — `tests/windows_hardening.rs` (16 tests, all pass)

| Area | Tests |
|---|---|
| Allocator | fragmentation/first-fit reuse, large alloc after holes, alternating free, leak checker still fires |
| Strings/fs | binary-safe file round trip (exact bytes), paths with spaces |
| TCP | multi-recv checksum reassembly, echo server 3 sequential connections, repeated connect/close |
| UDP | loopback round trip (self peer) |
| HTTP | split-server multi-recv + 404 + refused + premature close; 60 KB body twice, exact length |

All deterministic, localhost-only, no public internet. HTTP/UDP/allocator stress were previously untested
gaps in the Windows suite.

## 3. Multi-purpose real MINK programs (Phase 10) — all executed through the normal CLI

Six small real programs composed from the stdlib, each built with `mink build` and run directly on Windows:

| Program | Composition | Result |
|---|---|---|
| A txtstats | filesystem read + byte scanning | RC=0: bytes=43 lines=3 es=7 (file verified) |
| B fstool | mkdir → write → size → read-back verify → rename → delete → rmdir | RC=0, all ops 0/1 as expected |
| C jsonrep | json parse → key/value extraction → serialize | RC=0: exact round trip `{"tool":"mink","ver":2026,"ok":true}` |
| D chat | TCP loopback server + client (listen/accept/send/recv/close) | RC=0 both: good=1, 4-byte ping/pong |
| E httpget | HTTP GET localhost (status 200, 17-byte body byte-verified) | RC=0 pass=1 |
| F procwatch | process run ×2, stdout capture, exit codes | RC=0: rc1=0, rc2=3, stdout 28 bytes |

These prove filesystem, JSON, TCP, HTTP, and process features compose through the real toolchain, not just in
per-library test harnesses.

## 4. Findings classified

- **P0: none found or remaining.**
- **P1 (fixed): stdlib ownership leak family** — any real program doing fs ops with heap strings,
  path ops, network sends with heap strings, or JSON parsing of heap docs leaked (E-R06). All fixed above,
  execution-verified, regression-covered.
- **P2 (documented, not fixed this session):**
  - Network calls require `net_init()` (Winsock startup) on Windows; calling socket APIs without it crashes
    instead of returning a clean error. `net_init` is documented at the top of `stdlib/network.mink` and every
    test/program calls it; a clean no-init error is future error-path work.
  - `process_stderr` capture is unreliable on Windows (stdlib-documented; child `stderr` may be empty even
    when the child exits non-zero — exit codes do propagate: rc2=3 verified).
  - V1 ownership model: an owned `Str` may be passed to only one consuming stdlib parse helper; callers use
    inline byte-scanning for multi-field parsing (Session 93 documented limitation, unchanged).
- **P3:** env_set/env_remove remain stubs on Windows (README already accurate); single-char `push_str` style
  nits fixed in the new test file during review.

## 5. Quality gates

- `cargo fmt --check`: clean
- `cargo clippy --all-targets --all-features`: 0 warnings in changed files (pre-existing warnings elsewhere)
- `cargo test`: **2536 passed / 0 failed** (Session 94 baseline 2520 + 16 new)
- `cargo build`: clean
- `cargo build --release`: clean

## 6. Cross-platform contract notes (Linux read-only review)

Linux was not modified. The Session 92/93 Linux findings (net_recv ownership, exact-length returned strings,
wrapper consume-and-free) were checked against Windows: Windows runtime services were already consistent on
the net/HTTP paths (hardening HTTP/UDP tests pass); the gaps were in the shared `stdlib/*.mink` wrappers,
which both platforms use — those are now fixed once for both.

## 7. Known remaining Windows limitations

- POST builder exists but no end-to-end Windows HTTP POST execution test yet.
- No TLS/HTTP-2/chunked/keep-alive (not in the V1 HTTP API; documented in Session 93 record).
- `process_stderr` capture unreliable (see P2).
- Windows env_get/env_set/env_remove remain stubs (README-documented).
