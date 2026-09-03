# Session 92 — Linux Crypto Runtime + Network/Env Ownership Hardening

Date: 2026-09-03. Branch `main`, starting commit `1c972f3`. MINK v1.0.1.

## Scope

No package management, REPL, formatter, or Git workflow automation work. This
session hardens the existing Linux x86_64 target: crypto runtime, UDP and
network/env ownership verification, and durable regression coverage.

## Linux subsystem status matrix

| Subsystem              | Previous (S91)            | Current (S92)                 | Verification level |
|------------------------|---------------------------|-------------------------------|--------------------|
| ELF backend            | execution verified        | unchanged                     | STABLE             |
| Core runtime           | execution verified        | unchanged                     | STABLE             |
| Filesystem             | execution verified        | unchanged                     | STABLE             |
| Process                | execution verified        | unchanged                     | STABLE             |
| Time                   | execution verified        | unchanged                     | STABLE             |
| Random                 | execution verified        | unchanged                     | STABLE             |
| Env get (env_get)      | "implemented" but crashed | segfault fixed (rdx clobber)  | EXECUTION VERIFIED |
| Env has (env_has)      | partial (never true)      | true-return fixed (rax clobber)| EXECUTION VERIFIED |
| Env set / remove       | deferred stubs (-1)       | unchanged (still stubs)       | STUB               |
| TCP networking         | execution verified        | unchanged                     | STABLE             |
| net_recv ownership     | avoided in tests          | owned heap Str, free verified | EXECUTION VERIFIED |
| UDP networking         | exposed, untested         | loopback round-trip verified  | EXECUTION VERIFIED |
| Crypto                 | 4 stubs                   | getrandom(2) implementation   | EXECUTION VERIFIED |
| HTTP                   | blocked on runtime        | http_recv_all chunk leak fixed| PARTIAL (parsing tests pass; no live-HTTP Linux test) |

## Key finding: the "broken Linux exit/leak checker" was a measurement artifact

S92-part-1 probes reported `rt_exit(7)` exiting 0 and silent leak detection on
Linux. Re-investigation proved the Linux exit code and leak scan were always
correct: `strace` showed `exit(7)`, Python subprocess reported returncode 7,
and an isolated WSL script reported `LEAK_EXIT=105`. The earlier `echo $?`
measurements were polluted because the outer (Windows) shell expanded `$?`
before WSL executed — every nested `bash -c "...echo $?..."` read the outer
shell's status (0), not the MINK process's. Correct methodology: measure exit
codes through Rust `Command` (as the test harness does) or via script files
run with `wsl -- bash file.sh`, never via `echo $?` inside `wsl bash -c`.

One genuine inconsistency remained: the Linux leak path exited **105** (it
used the `RuntimeErrorKind::Leak` enum discriminant, 5) while Windows and the
catalogued E-R06 use error number 6 → exit **106**. Fixed by calling
`RuntimeErrorKind::Leak.number()`.

## net_recv / env_get ownership

Root cause of the reported "net_recv ownership problem": **no language bug
existed**. Both platforms' `rt_net_recv` allocate a fresh heap Str (length
prefix + bytes) on every path — success, close, and error (`StrAlloc(0)`).
Session 91's tests merely *avoided* net_recv ("no recv to avoid leak
checker"), hiding that the correct usage is `recv` then `rt_str_free`.

Chosen model (smallest, architecturally consistent):
- `net_recv`, `net_resolve`, `net_hostname`, and `env_get` return **owned
  heap Strs**; the caller must `rt_str_free` them. Intrinsics never consume.
- Evidence: recv + free exits 0 with correct content; recv without free exits
  **106 (E-R06)**; same for env_get (free → 0, unfreed → 106). Windows
  behavior is unchanged (its E-R06 leak tests still pass).
- Updated the stale `stdlib/network.mink` docstring that claimed net_recv
  returns a pointer into a fixed BSS buffer that is overwritten each call.
- Fixed a real latent leak on **both** platforms: `http_recv_all` never freed
  its per-chunk `net_recv` results (up to 1024 chunks). Restructured with a
  single textual `rt_str_free(data)` per iteration (MINK V1 move semantics
  forbid freeing in an early-exit branch and then using the value again).

## env_get/env_has real bugs found and fixed (Linux)

- `env_get` found-path segfaulted (SIGSEGV): the value-start pointer was held
  in `rdx` across the `StrAlloc` service call, which clobbers caller-saved
  registers (rdx ended up 0x1c). The pointer is now saved to `[rbp-16]`
  before the call and restored after.
- `env_has` returned `false` for existing variables: it set `rax = 1` (true)
  and then called `free_cstr`, which clobbers rax. The free now happens
  before the result is set.

## Crypto capability matrix

| Operation               | Windows implementation          | Linux previous | Linux current                     |
|-------------------------|---------------------------------|----------------|------------------------------------|
| crypto_init             | LoadLibrary bcrypt.dll          | stub (returns 0) | returns 0 (no provider needed)   |
| crypto_random_bytes     | BCryptGenRandom into buf        | stub (returns 0, no bytes) | getrandom(2) loop, full fill; -1 on error |
| crypto_random_int       | BCryptGenRandom 8 bytes         | stub (returns 0) | getrandom(2) 8 bytes             |
| crypto_secure_zero      | byte-wise... 32-bit store loop  | stub (returns 0, no zeroing) | exact byte-wise zero loop        |

No OpenSSL or libc crypto: Linux uses the kernel CSPRNG via `getrandom(2)`
(SYS_GETRANDOM = 318, already proven in `emit_init`). HMAC-SHA256 / HKDF /
SHA-256 remain pure MINK (stdlib/hashing.mink + stdlib/crypto.mink), which
now compile and run on Linux. Note: the Windows `crypto_secure_zero` writes
32-bit zeros while advancing one byte, which can touch up to 3 bytes past the
requested region; the Linux version stores exactly `len` bytes.

## Execution evidence (generated MINK ELF binaries, WSL2 Ubuntu)

| Probe                                  | Exit | Meaning                                  |
|----------------------------------------|------|------------------------------------------|
| crypto (random bytes/int/hex)          | 0    | CSPRNG fills, lengths and content OK     |
| env_get owned + free + env_has both    | 0    | found/missing/free/true/false all correct|
| env_get result not freed               | 106  | E-R06 leak detected                      |
| UDP bind + connect-send + recv round-trip | 0  | datagram loopback, content verified     |
| UDP send on unconnected socket         | 0    | error path returns -1, no hang          |
| TCP echo, recv + free                  | 0    | owned receive, content verified         |
| TCP echo, recv without free            | 106  | E-R06 leak detected                     |

## Regression tests added (tests/process_lib.rs)

11 new Linux WSL tests (existing `linux_test!` / `linux_net_test!` pattern,
plus a new `linux_crypto_test!` builder that concatenates
stdlib/hashing.mink + stdlib/crypto.mink):
- `linux_n07_recv_owned_free`, `linux_n08_recv_leak_e_ro6`
- `linux_n09_udp_loopback`, `linux_n10_udp_unconnected_send_error`
- `linux_e07_env_get_owned_free`, `linux_e08_env_get_missing_empty`,
  `linux_e09_env_get_leak_e_ro6`, `linux_e10_env_has_existing_true`
- `linux_c01_crypto_random_bytes`, `linux_c02_random_bytes_differ`,
  `linux_c03_random_int_and_hex`

Deterministic: loopback only, no public internet. Fixed ports; Linux
`net_bind` now sets `SO_REUSEADDR` before bind so repeated test runs do not
hit EADDRINUSE from TIME_WAIT peers.

Windows test fix: `time_lib::t50_time_now_year_is_2026` hardcoded month = 8
and broke on the September date rollover (unrelated to this session). It now
derives the expected UTC year/month on the host with the same civil-date
algorithm the MINK code under test uses, so it stays deterministic.

## Quality gates (all pass)

- `cargo fmt --check` — clean
- `cargo clippy --all-targets --all-features` — no warnings in changed files
- `cargo test` — all 56 test binaries pass (Windows + Linux-in-WSL)
- `cargo build` / `cargo build --release` — clean

## Remaining Linux gaps (accurate)

- Vec* (8 services) and Env set/remove remain stubs.
- HTTP has no live end-to-end Linux test yet (http_recv_all leak now fixed;
  parsing-level http tests pass on Windows).
- Crypto service coverage: crypto_secure_zero has no direct stdlib consumer
  (secure zero is a MINK byte loop) — implementation exists for parity.
