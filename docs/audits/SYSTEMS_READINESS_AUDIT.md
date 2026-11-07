# MINK: Systems-Readiness Audit (Windows x86_64)

**Status:** Final audit. Windows x86_64 remains FROZEN.
**Platform evaluated:** Windows x86_64 only. Linux: FROZEN, untouched by this audit.
**Starting commit:** `74131ff` · Ending commit: the commit carrying this record.
**Regression at close:** 2957 passed · 0 failed · 3 ignored (2952 before this audit; five
hashing regressions added, see §6).

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
| Imports | `kernel32.dll` only |
| Size | 187,392 bytes |

Winsock is reached through dynamically resolved entry points (`rt_sys_load_lib` /
`rt_sys_get_proc`), not through the import table. A `net_*` call made before
`net_init()` does **not** fault: every entry point is resolved on demand and the
unresolved ones return the failure convention (`-1`, or an empty string for
`net_resolve`/`net_hostname`), verified again after the 1.0.2 closure pass.
(An earlier revision of this audit recorded an access violation here; that claim
was stale.)

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

1. `hash_sha256` faulted with `E-R05` for any input ≥ 256 bytes. The 0x80 padding byte
   was written in every block whose offset was non-negative, so an earlier block computed
   a message-schedule word index past the end of the 1024-byte workspace. The guard now
   also requires `byte_pos < 64`. Digests were subsequently checked against Python
   `hashlib` for 28 lengths from 0 to 10,000 bytes. All match.
2. The hashing entry points leaked an owned `Str` input. `hash_fnv1a`, `hash_djb2` and
   `hash_sha256` move their input (the caller's binding is dead afterwards, so the caller
   cannot free it) but never released it, so hashing a heap buffer leaked (`E-R06`) with no
   workaround. They now release the input, matching the existing `zlib` convention. A
   literal input is immortal, so the free is a no-op for it.

## 7. Known limitations (do not overclaim)

* Input ownership is incomplete across the standard library. The §6.2 pattern
  consuming an owned `Str` without releasing it, also exists in `encoding` and `strings`
  (`hex_encode`, `base64_encode`, `str_trim`, … leak a heap input, `E-R06`). The affected
  suites tolerate exit 106, which is why it persisted. This audit fixed the hashing path
  because a real utility needed it; the broader module-wide change was not made here.
  *Dated correction, 2026-11-06 (Session 190):* the named functions no longer leak their
  input — `hex_encode`, `hex_encode_upper`, `base64_encode` and `base64_url_encode` each
  call `rt_str_free(s)`, and `str_trim` delegates to `str_sub`, which releases its input.
  The remaining defect was in the tests, which dropped owned return values. Six suites
  (`crypto`, `encoding`, `hashing`, `math`, `random`, `time`) now require exit 0, which
  exposed and fixed two base64 assertions that had never executed. See
  [`../benchmarks/M6/M6_SESSION_190_LEAK_ENFORCEMENT.md`](../benchmarks/M6/M6_SESSION_190_LEAK_ENFORCEMENT.md).
  *Dated correction, 2026-11-06 (Sessions 191–193):* **this limitation is closed.**
  All 96 exit-106 tolerances across nine suites are gone — 84 in Session 190, the
  collections and filesystem pair in Session 191, and the ten strings tolerances in
  Session 192. Three of those ten were real leaks: JSON accessors return heap copies,
  not arena views, and the tests had never freed them. Every suite that runs compiled
  programs now requires exit 0, and the negative tests that assert 106 are checking that
  the leak checker still fires. See
  [`../benchmarks/M6/M6_SESSION_193_OWNERSHIP_CLOSURE.md`](../benchmarks/M6/M6_SESSION_193_OWNERSHIP_CLOSURE.md).
* Unicode. `Str` is a byte buffer with a UTF-8 code-point layer but no non-ASCII case
  operations and no Unicode database. Command-line arguments and filesystem entries use
  the ANSI Windows APIs, so non-ASCII argv/entries are limited.
* Graceful precondition misuse in `network`. Calling a `net_*` function before
  `net_init()` returns the failure convention (`-1`; `net_resolve`/`net_hostname`
  return an empty string) rather than faulting, verified on 1.0.2. An earlier
  revision of this audit recorded an access violation, which no longer reproduces.
* Hash naming precision. `hash_fnv1a` / `hash_djb2` are 64-bit-register variants (they
  do not truncate to 32 bits), so their values diverge from the classic 32-bit algorithms
  for longer inputs. `hash_sha256` matches the standard exactly.
* No long-running/soak evidence, and no integrated debugger or profiler.
* Linux is frozen and is not a supported platform; macOS is not supported.

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
support; "production-ready"; or any performance comparison against C/C++. No benchmarks
were run.

## 9. Verdict

MINK can implement real, non-trivial native Windows systems-oriented software today, and
the capability list in §8 is supported by executed evidence. The claim boundary reflects
the fixed defects, the residual standard-library ownership gap, the Unicode/ANSI limits,
and the absence of long-running, debugger/profiler and benchmark evidence. Windows
remains **FROZEN**; Linux remains FROZEN.

## 10. Dated addendum — Sessions 194–196 (2026-11-06)

Windows x86_64 runtime reliability was re-audited with bounded stress and one
confirmed memory-safety defect was fixed and independently verified.

* **Fixed:** collection services read `length`/`elements` directly from a freed
  handle. `Vec`, `Map` and `Set` accessors now validate the handle against the
  liveness table and fault `E-R05` (exit 105); previously they returned
  deterministic stale values and exited 0. Regression: eleven tests in
  `tests/collections_lib.rs`.
* **Verified without changes:** double-free still `E-R04` (104), out-of-bounds
  still `E-R10` (110), leaks still `E-R06` (106), valid operations unchanged.
* **Resolved in Session 197:** a freed-then-reused handle no longer aliases
  the replacement. Handles carry the allocation generation, so a stale `Vec`,
  `Map` or `Set` handle faults `E-R05` (105) even after its block has been
  handed to a new allocation; a plain double free stays `E-R04` (104). The
  residual bound is a 16-bit generation (65,536 reallocations of one block).
  Regression: eleven `s197_*` tests in `tests/collections_lib.rs`.
* **Fresh evidence:** full regression 3,194 passed / 0 failed / 1 ignored / 78
  suites; release build 4,030,976 bytes; `invsync` all cases passed; 13
  use-after-free variants, with 10× repeats on three representative cases;
  bounded stress (20k alloc/free, 50k pushes, 5k collection cycles, 200
  filesystem cycles, 256 KiB I/O, 30 child processes, 5 compiles) clean.

*Dated addendum — Session 197 (2026-11-06).* The residual alias limitation is
closed. After the generation-safe handle change, the measured regression is
**3,185 passed / 0 failed / 0 ignored across 75 suites** (every target except
`tls_lib`; 3,174 + 11 new `s197_*` tests), with the release binary at 4,041,216
bytes. `tls_lib` could not run on the audit machine: `python3` is the Microsoft
Store alias stub, so its TLS test server exits before any MINK code runs (2
passed, 18 server-start failures, 1 ignored). That is an environment
limitation, not a compiler result. See
[`M6_SESSION_197_GENERATION_SAFE_HANDLES.md`](../benchmarks/M6/M6_SESSION_197_GENERATION_SAFE_HANDLES.md).

*Dated addendum — Session 198 (2026-11-06).* A resource-lifecycle audit added
an instrumented suite (`tests/resource_lifecycle.rs`) that samples the child
process's OS handle count while it runs, with a negative control that proves
the sampler sees a leak. Measured: 3,000 filesystem cycles (62 → 63 → 62),
3,000 failing filesystem calls (62 → 62 → 62), 40 process spawns (72 → 72 →
72), 3,000 thread spawn/join cycles (63 → 63 → 63), 200,000 allocation cycles
(exit 0, exact total); control 22 → 410 with exit 106. No confirmed defect in
those paths. One risk is **recorded, not patched**: `emit_fs_read` ignores
`ReadFile`'s BOOL and byte count and returns a string whose length is the
requested size, so a short or failed read would surface stale heap bytes; no
deterministic trigger exists in the current API, so it is documented rather
than changed. See
[`M6_SESSION_198_RESOURCE_LIFECYCLE.md`](../benchmarks/M6/M6_SESSION_198_RESOURCE_LIFECYCLE.md).

This addendum raises the reliability evidence for the existing claim boundary. It
does **not** establish long-running soak behaviour, complete memory safety, or M6
external validation. Precondition misuse can still fault raw on some paths (§5
item 20); M6 remains open, M7 remains gated, and Linux remains frozen. See
[`../benchmarks/M6/M6_SESSION_196_RELIABILITY_CLOSURE.md`](../benchmarks/M6/M6_SESSION_196_RELIABILITY_CLOSURE.md).

*Dated addendum — Session 199 (2026-11-07).* Bounded sustained workloads were
run in one process with a hard wall-clock budget and 10 ms sampling of the
child's OS handle count and peak working set (`tests/soak.rs`, 7 `--ignored`
tests): 6 compiler builds (5.2–13.9 ms each), 100 program launches (2.28 ms
each), 400,000 collection cycles (0.021 ms/iter), 3,000 filesystem cycles plus
five 1 MiB reads (1.065 ms/iter), 100 child processes (6.874 ms/iter), 20,000
error-path iterations (0.045 ms/iter) and three 200-iteration combined runs
(~7.1 ms/iter). Every run exited 0 with empty stderr, a zero internal failure
counter and flat handle counts (28 → 68/82 → 68/78; peak WS 3.6–31.7 MiB). The
suite passes 7/7 in 8.3 s; exit codes 104/105/110 reproduced 10/10 times on the
error paths. No confirmed defect. One over-large workload (65,540 B × 12,000)
reached the documented 1 GiB no-coalesce arena ceiling — `E-R02` (102) at
iteration 342, peak WS 1027.1 MiB — failing loudly and deterministically; the
bounded workload was resized inside the documented limit rather than changing
the allocator. The `emit_fs_read` short-read risk remains recorded, not
patched. This is bounded-soak evidence, not long-running stability, and M6
external validation remains outstanding. See
[`M6_SESSION_199_BOUNDED_SOAK.md`](../benchmarks/M6/M6_SESSION_199_BOUNDED_SOAK.md).

*Dated addendum — Session 200 (2026-11-07).* Compiler correctness and
diagnostics were audited with a new 24-case contract battery
(`tests/diagnostics.rs`): each malformed program must exit 1, carry the
expected `E-…` code and a `file:line` location, contain no panic marker, and
produce byte-identical stderr on a repeat compile. Two confirmed defects were
fixed. (1) Integer literals that do not fit in `Int` silently wrapped —
`9223372036854775808` printed `-9223372036854775808` and
`999999999999999999999999` printed `2003764205206896639`, both exit 0 —
because both literal decoders used wrapping arithmetic and nothing checked the
source text. Literals are now decoded with checked arithmetic at type-check
time and rejected as `E-T43`, in expressions and recursively in match patterns,
while `-9223372036854775808` remains legal as `Int`'s minimum. (2) Every
aggregate type diagnostic rendered `` `` is not a valid key type: … ``, the
`E-T42` prefix, so `E-T11` array-index errors were mislabelled; aggregate kinds
now print their own message. Verification: 24/24 debug, 24/24 release, and a
fresh full regression across 78 suites (all targets except `tls_lib`, whose TLS
server needs the absent `python3`) was run after the shared type-checker
change. Release builds clean with `CARGO_INCREMENTAL=0`; the default
incremental path crashed `rustc` with STATUS_ILLEGAL_INSTRUCTION once, an
environment/toolchain event, not a source defect. See
[`M6_SESSION_200_COMPILER_DIAGNOSTICS.md`](../benchmarks/M6/M6_SESSION_200_COMPILER_DIAGNOSTICS.md).

*Dated addendum — Session 201 (2026-11-07).* The standard-library surface and
CLI developer experience were reviewed. `stdlib/*.mink` declares 700
top-level declarations: 462 public and 238 `_`-private helpers. The reference
covered 397 of the 462 public functions (85.9%); the 65 missing are now listed
with source signatures in §26 of the reference (math 20, http 13, time 12, log
7, byte 7, rt 2, str 1, net 1, network shutdown constants 2), and a new
`tests/stdlib_docs.rs` guard scans the library and fails when a public function
is undocumented (462 checked, 0 missing). The CLI contract was measured directly: `mink help` and no arguments
exit 0; an unknown command and a missing file for `run`/`build`/`check` exit 1
with a clear message. No code defect was found. See
[`M6_SESSION_201_STDLIB_DX.md`](../benchmarks/M6/M6_SESSION_201_STDLIB_DX.md).

*Dated addendum — Session 202 (2026-11-07).* Release readiness for the
six-session programme was verified on the final tree: **3,218 passed / 0 failed
/ 7 explicitly ignored / 79 suites** (all targets except `tls_lib`), release
build clean at **4,043,776 bytes**, and release-mode diagnostics (24), bounded
soak (7) and resource lifecycle (7) suites green. The programme added 51 tests
and fixed three confirmed defects — generation-safe collection handles (197),
integer-literal wraparound as `E-T43` (200), and the mangled aggregate
diagnostic prefix (200). Residual limits stay recorded and unchanged: the
unpatched `rt_fs_read` short-read risk, the 1 GiB no-coalesce arena ceiling,
`MAX_LIVE_ALLOCS = 1024`, the 16-bit handle generation, bounded rather than
open-ended soak evidence, and the absent `python3` that keeps `tls_lib` from
running. M6 external developer validation remains outstanding, M7 remains
gated on demonstrated demand, and Linux remains frozen. See
[`M6_SESSION_202_RELEASE_READINESS.md`](../benchmarks/M6/M6_SESSION_202_RELEASE_READINESS.md).

*Dated addendum — Session 203 (2026-11-07).* The `rt_fs_read` risk recorded in
Session 198 was confirmed: the emitter ignored `ReadFile`'s BOOL and byte count
and returned a string sized by `GetFileSize` over uninitialised/stale memory.
The fix records the transferred count in a per-call stack slot, sets the
string length from it, and returns an owned empty string when the read fails.
A held `LockFile` byte range reproduces the failure deterministically: the
pre-fix binary reported 4096 bytes, the fixed binary reports 0, frees the
result, exits 0, and reads the full file after the lock is released. Six tests
in `tests/fs_read.rs`; debug 6/6 (4 runs), release 6/6, full regression **3,224
passed / 0 failed / 7 ignored / 80 suites**. Remaining: short reads share the
fixed path but have no deterministic public-API trigger; `GetFileSize` is a
32-bit API, so files ≥ 4 GiB stay outside the supported range; `tls_lib` still
cannot run without `python3`. See
[`M6_SESSION_203_FILESYSTEM_READ.md`](../benchmarks/M6/M6_SESSION_203_FILESYSTEM_READ.md).

*Dated addendum — Session 204 (2026-11-07).* All three capacity risks were
inspected. The arena ceiling (1 GiB, no coalescing) and the 1,024-slot live
allocation table are intentional and fail deterministically (`E-R02`/102 for a
request above the ceiling before any commit; `E-R03`/103 for the 1,025th live
allocation) with no partially linked metadata and no post-failure corruption
path. The 16-bit generation field was a confirmed ABA defect: after 65,536
reuses of one block the tag wrapped and a stale handle was accepted as live
(red check reached `E-R10`/110 instead of `E-R05`/105). Freed blocks now retire
at generation `0xFFFF`, generation 0 is never issued, and retired addresses are
never reused; the cost is one abandoned block per 65,536 reuses. Seven tests in
`tests/capacity.rs`, debug and release 7/7; full regression **3,231 passed / 0
failed / 7 ignored / 81 suites**. Remaining: the 1 GiB ceiling and 1,024-slot
table stay as documented limits, retirement consumes dead arena space, and
`tls_lib` still cannot run without `python3`. See
[`M6_SESSION_204_CAPACITY_AND_GENERATIONS.md`](../benchmarks/M6/M6_SESSION_204_CAPACITY_AND_GENERATIONS.md).

*Dated addendum — Session 205 (2026-11-07).* The `tls_lib` limitation above is
resolved. Root cause: `python3` is the Microsoft Store App Execution Alias,
which spawns successfully and then exits 49 with "Python was not found"; the
harness discarded stderr, so it could only report a readiness timeout, and it
did not reap the child. CPython 3.11.9 is installed as `python`. The harness
now probes `python3`, `python`, `py -3`, accepts only a successful `Python 3`
probe, captures and reports server stderr, kills and reaps a failed child, and
tests the prerequisite explicitly. The complete suite executes: **21 passed /
0 failed / 1 ignored**, repeated three times; the single ignore is the opt-in
public-internet system-store control. New environment blocker: every
release-mode `rustc` invocation in this session crashed (varying
`STATUS_ILLEGAL_INSTRUCTION`, `STATUS_HEAP_CORRUPTION`,
`STATUS_ACCESS_VIOLATION`) on known-good targets and reduced-codegen flags;
debug is unaffected. This is a toolchain/machine fault, recorded as not run
rather than passed. See
[`M6_SESSION_205_TLS_HARNESS.md`](../benchmarks/M6/M6_SESSION_205_TLS_HARNESS.md).

*Dated addendum — Session 206 (2026-11-07).* The 4 GiB `GetFileSize`
limitation recorded in Sessions 203/204 was a real defect, not just an
unsupported range: `fs_file_size` returned the low DWORD (4 GiB + 12,345 →
12,345; exactly 4 GiB → 0) and `fs_read` returned that many leading bytes,
while a 4 GiB−1 file aborted with `E-R02`. Both services now use
`GetFileSizeEx`; `fs_file_size` reports the exact 64-bit length and `fs_read`
refuses files larger than the arena with an owned empty string. The public
API remains whole-file, so offsets above 4 GiB are not reachable. Five sparse
tests in `tests/large_file.rs`; red 4 failed / 1 passed, green 5/5. Release
remains environment-blocked by the `rustc` crash. See
[`M6_SESSION_206_LARGE_FILES.md`](../benchmarks/M6/M6_SESSION_206_LARGE_FILES.md).

*Dated addendum — Session 207 (2026-11-07).* Extended soak evidence: a bounded
300-second run (cap 900 s) with one child at a time, a 30-second per-launch
budget, a 512-byte payload and artifact cleanup. **1,440/1,440 launches exited
0** across Vec/alloc/string/filesystem/process workloads; **72/72 compiler
builds** produced a PE; **216/216 double-free runs** exited 104; **2/2 TLS
suites** passed; peak working set **4.1 MiB**; handle count **73 → 79**; disk
growth 298 KB, entirely the TLS race test's per-run fixture root; zero
crashes, hangs or unexpected errors. This is a 5-minute run, not a multi-hour
soak, and release re-runs remain blocked by the `rustc` environment crash. See
[`M6_SESSION_207_EXTENDED_SOAK.md`](../benchmarks/M6/M6_SESSION_207_EXTENDED_SOAK.md).

*Dated addendum — Session 208 (2026-11-07).* Section 26 of the standard-library
reference is the appendix of 66 public functions omitted from the main
sections. All 66 signatures were checked against `stdlib/*.mink` and match
exactly; no function is missing and no API changed. The prior coverage gap was
mention-only checking in `tests/stdlib_docs.rs`; the new
`tests/stdlib_signatures.rs` extracts each §26 signature and compares it with
the source, failing on any drift (1/1 passing). Runtime behavior stays covered
by the module suites. See
[`M6_SESSION_208_SECTION26_SIGNATURES.md`](../benchmarks/M6/M6_SESSION_208_SECTION26_SIGNATURES.md).

*Dated addendum — Session 209 (2026-11-07).* Cross-subsystem audit: four new
tests combine filesystem reads with collections, generation retirement, failed
reads and live-table exhaustion. All pass; no new defect was found. The full
regression independently re-exercises the Session 203/204/206 fixes and is
clean at **85 suites, 3,262 passed, 0 failed, 9 ignored**, with zero non-zero
suite exits. Release-mode runs remain blocked by the environment `rustc` crash.
See
[`M6_SESSION_209_CROSS_SUBSYSTEM.md`](../benchmarks/M6/M6_SESSION_209_CROSS_SUBSYSTEM.md).

*Dated addendum — Session 210 (2026-11-07, final).* Final independent audit:
worktree preserved (four untracked user files), no merge markers or new
binaries, Cargo/npm versions both 1.0.5, npm package unpublished. Debug build
and full regression pass at **85 suites / 3,262 passed / 0 failed / 9 ignored /
0 bad exits**, and the 203/204/206 fixes plus the 208/209 guards were re-run
independently (6/6, 5/5, 7/7, 4/4, 1/1). **Release build is still blocked**:
`rustc` at opt-level 3 crashes with `STATUS_ILLEGAL_INSTRUCTION` /
`STATUS_HEAP_CORRUPTION` / `STATUS_ACCESS_VIOLATION`, including after a forced
1.97.1 toolchain reinstall; debug works and a trivial release crate builds.
Release-mode verification is recorded as not run. Final verdict: **NOT READY**
— an environment release blocker, not a confirmed MINK defect; M6 external
validation also remains outstanding. See
[`M6_SESSION_210_FINAL_ACCEPTANCE.md`](../benchmarks/M6/M6_SESSION_210_FINAL_ACCEPTANCE.md).
