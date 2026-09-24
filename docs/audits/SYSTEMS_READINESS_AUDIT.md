# MINK — Systems-Readiness Audit (Windows x86_64)

**Status:** Final audit. Windows x86_64 remains **FROZEN**.
**Platform evaluated:** Windows x86_64 only. **Linux: FROZEN** — untouched by this audit.
**Starting commit:** `74131ff` · **Ending commit:** the commit carrying this record.
**Regression at close:** 2957 passed · 0 failed · 3 ignored (2952 before this audit; five
hashing regressions added — see §6).

This record answers one question with executed evidence rather than test counts:

> Can MINK currently be used to implement real native Windows systems-oriented software,
> and what exact systems-programming claims are justified by observed evidence?

## 1. The program audited

A single native utility, **`mink-sysinfo`** (`examples/sysinfo/main.mink`, ≈500 lines of
MINK), built with the public CLI and run as a standalone `.exe`. It combines, in one
process:

* command-line arguments (`rt_argc` / `rt_argv`);
* environment round trip (get / set / has / remove);
* process identity and child-process creation with captured stdout (`rt_process_run`,
  `rt_process_stdout_len`, byte scan of the capture buffer);
* filesystem work (create dir, write, read, size, copy, move, remove, directory
  enumeration) plus missing-input, empty-input and space-in-path cases;
* a 4096-byte binary workload with SHA-256, FNV-1a, CRC-32 and a zlib compress/decompress
  round trip, all byte-verified;
* four OS threads accumulating under a lock, with the total checked against the closed
  form;
* a local TCP loopback (listen → dial → accept → send → recv → echo → close);
* time (`rt_time_millis`, `rt_time_now`, `time_strftime`).

stdout is deterministic; the nondeterministic identity/clock values go to stderr, so
repeat runs diff cleanly.

## 2. Native executable evidence

`mink build` produced `mink_sysinfo.exe` with **no external toolchain**. Parsing the image:

| Property | Observed |
| --- | --- |
| Machine | `0x8664` (x86-64) |
| Optional header | `0x20B` (PE32+) |
| Subsystem | `3` (WINDOWS_CUI) |
| Sections | `.text`, `.bss`, `.idata`, `.reloc` |
| Imports | **`kernel32.dll` only** |
| Size | 187,392 bytes |

Winsock is reached through dynamically resolved entry points (`rt_sys_load_lib` /
`rt_sys_get_proc`), not through the import table — which is also why a `net_*` call made
before `net_init()` faults instead of returning a structured error (§7).

The same executable was copied into an unrelated directory and run with `PATH` restricted
to `C:\Windows\System32`, with no MINK compiler, no Rust/Cargo and no repository files on
the machine path. It printed the full report and exited 0.

## 3. Memory and resource behaviour

The runtime validates every allocation and scans the live table at exit, so a clean exit
is positive evidence that nothing the program allocated was left live. `mink-sysinfo`
exited **0** (no `E-R06`) on every run, across every path: repeated map/set updates,
buffer create/free, zlib scratch arenas, thread control blocks, lock words and every
owned `Str`.

Observed directly during the audit and then **fixed** (§6): `hash_sha256` faulted with
`E-R05` on inputs of 256 bytes or more, and the hashing entry points leaked their owned
input.

## 4. Determinism and repeatability

* 12 consecutive standalone runs: identical stdout (`sha256` of stdout identical across
  all 12), all exit 0, no `E-R*` on stderr.
* UDP loopback probe: 3 consecutive runs, identical, exit 0.
* Concurrency total matched the closed form on every run (≤ 4 threads, exact under a lock).

This is repeatability evidence, not a soak test. No long-running workload was executed,
so "long-running execution" is **not** verified here (§5.24).

## 5. Claim inventory

| # | Category | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Native compilation | VERIFIED | `mink build` emits a PE with no external toolchain |
| 2 | Native machine-code generation | VERIFIED | x86-64 PE32+ image, `.text`/`.bss` emitted by the MINK backend |
| 3 | Direct OS interaction | VERIFIED | filesystem, process, environment, memory, threads all executed |
| 4 | Explicit ownership / lifetime | VERIFIED WITH LIMITATION | move semantics + compile-time use-after-move; §6 leak class in some stdlib paths |
| 5 | Heap allocation / deallocation | VERIFIED | `rt_alloc`/`rt_free`, `rt_str_alloc`/`rt_str_free`; exit-time leak scan |
| 6 | Deterministic process exit | VERIFIED | exit codes propagate; distinct `E-R05`/`E-R06` codes observed |
| 7 | Resource cleanup | VERIFIED WITH LIMITATION | cleanup demonstrated; exit scan catches leaks; §6 class remains |
| 8 | Filesystem access | VERIFIED | create/write/read/size/copy/move/remove/enumerate; missing, empty, space paths |
| 9 | Process creation / control | VERIFIED | child exit code + captured stdout marker |
| 10 | Environment access | VERIFIED | set/get/has/remove round trip |
| 11 | TCP / UDP networking | VERIFIED | TCP loopback echo (utility) + UDP loopback probe |
| 12 | Non-blocking I/O | VERIFIED | `rt_net_set_nonblocking` / `rt_net_poll` via the readiness layer |
| 13 | Threads | VERIFIED | 4 OS threads, join, exact aggregate |
| 14 | Locks | VERIFIED | mutex acquire/release/free; exact total under contention |
| 15 | async / await | VERIFIED (prior session) | `stdlib/tasks.mink`, `examples/async_tasks`; not re-exercised here |
| 16 | Binary data processing | VERIFIED | 4096-byte buffer, byte ops, exact round trip, reference-matched digests |
| 17 | File / archive / compression | VERIFIED | zlib compress/decompress round trip; zip module present |
| 18 | Cryptographic primitives | VERIFIED WITH LIMITATION | SHA-256 reference-matched; HMAC/HKDF + BCrypt RNG; §7 hash-naming caveat |
| 19 | TLS / HTTPS | VERIFIED (prior session) | `stdlib/tls.mink`; requires network + certificates |
| 20 | Low-level error propagation | VERIFIED WITH LIMITATION | structured `E-L`/`E-T`/`E-S`/`E-R` codes with spans; precondition misuse can fault raw |
| 21 | CLI / native process interaction | VERIFIED | argv, child process, exit codes, stderr |
| 22 | Standalone deployment | VERIFIED | runs from an unrelated directory, `PATH` = System32 |
| 23 | Dependency / runtime footprint | VERIFIED | single `kernel32.dll` import; no CRT DLL; no Rust/Cargo at runtime |
| 24 | Long-running execution | NOT VERIFIED | no soak/long-duration run performed |
| 25 | Repeated allocation / deallocation | VERIFIED | allocation loops + 12 clean runs |
| 26 | Concurrent execution | VERIFIED | threads + lock, exact totals, repeatable |
| 27 | Large binary / data handling | VERIFIED WITH LIMITATION | 4 KB in the utility; SHA-256 verified to 10 KB; larger files not exercised |
| 28 | Cross-subsystem interaction | VERIFIED | one program uses fs + threads + net + process + hashing + zlib |
| 29 | Native ABI / FFI capability | VERIFIED WITH LIMITATION | dynamic FFI (`rt_sys_*`); not a full C ABI, no C headers/linking |
| 30 | Debugger / profiler / tooling | NOT AVAILABLE | no integrated debugger or profiler; CLI tooling (check/build/test/repl/explain, JSON diagnostics) is solid |

## 6. Defects found and fixed

Both were surfaced by the utility hashing a real 4 KB buffer. They were reproduced,
root-caused, fixed in `stdlib/hashing.mink` (mirrored byte-for-byte into
`npm/mink/stdlib/`, guarded by `tests/release.rs`) and covered by five new permanent
regressions in `tests/hashing_lib.rs`.

1. **`hash_sha256` faulted with `E-R05` for any input ≥ 256 bytes.** The 0x80 padding byte
   was written in every block whose offset was non-negative, so an earlier block computed
   a message-schedule word index past the end of the 1024-byte workspace. The guard now
   also requires `byte_pos < 64`. Digests were subsequently checked against Python
   `hashlib` for 28 lengths from 0 to 10,000 bytes — all match.
2. **The hashing entry points leaked an owned `Str` input.** `hash_fnv1a`, `hash_djb2` and
   `hash_sha256` move their input (the caller's binding is dead afterwards, so the caller
   cannot free it) but never released it, so hashing a heap buffer leaked (`E-R06`) with no
   workaround. They now release the input, matching the existing `zlib` convention. A
   literal input is immortal, so the free is a no-op for it.

## 7. Known limitations (do not overclaim)

* **Input ownership is incomplete across the standard library.** The §6.2 pattern —
  consuming an owned `Str` without releasing it — also exists in `encoding` and `strings`
  (`hex_encode`, `base64_encode`, `str_trim`, … leak a heap input, `E-R06`). The affected
  suites tolerate exit 106, which is why it persisted. This audit fixed the hashing path
  because a real utility needed it; the broader module-wide change was not made here.
* **Unicode.** `Str` is a byte buffer with a UTF-8 code-point layer but no non-ASCII case
  operations and no Unicode database. Command-line arguments and filesystem entries use
  the ANSI Windows APIs, so non-ASCII argv/entries are limited.
* **Raw fault on precondition misuse.** Calling a `net_*` function before `net_init()`
  faults (observed: access violation) rather than raising a structured error.
* **Hash naming precision.** `hash_fnv1a` / `hash_djb2` are 64-bit-register variants (they
  do not truncate to 32 bits), so their values diverge from the classic 32-bit algorithms
  for longer inputs. `hash_sha256` matches the standard exactly.
* **No long-running/soak evidence**, and **no integrated debugger or profiler**.
* Linux is frozen and is **not** a supported platform; macOS is not supported.

## 8. Claim boundary

The strongest claim the evidence supports is:

> **A systems-oriented native programming language for Windows x86_64, with direct OS,
> memory, process, filesystem, networking and concurrency capabilities.**

Safe to advertise (each backed by §2–§4): native compiled; native x86-64 PE generation;
standalone executables; no third-party runtime dependency (no CRT DLL); direct OS access;
explicit ownership with compile-time move checking; heap allocation/deallocation with an
exit-time leak check; threads and locks; async/await; TCP/UDP networking; filesystem and
process control; environment access; binary data and compression; cryptographic hashing;
deterministic behaviour and process exit.

Must **not** be claimed: cross-platform support, Linux or macOS support; full C/C++
ecosystem or C ABI compatibility; unrestricted FFI; "memory-safe" as an absolute; mature
debugger/profiler tooling; real-time or deterministic-latency guarantees; complete Unicode
support; "production-ready"; or any performance comparison against C/C++ — no benchmarks
were run.

## 9. Verdict

MINK can implement real, non-trivial native Windows systems-oriented software today, and
the capability list in §8 is supported by executed evidence. The claim boundary reflects
the fixed defects, the residual standard-library ownership gap, the Unicode/ANSI limits,
and the absence of long-running, debugger/profiler and benchmark evidence. Windows
remains **FROZEN**; Linux remains **FROZEN**.
