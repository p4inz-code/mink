# Windows Practical-Parity Validation — Capability Matrix and Gap List

Milestone: Windows practical-parity validation (sessions 126-127).
Baseline: HEAD `dd556a692b8290416e14703471307cc311995d2f` (v1.0.4), no version
bump, Windows x86_64 only, Linux FROZEN.

Every row below is backed by a committed harness under `validation/` that runs
the MINK build and a Python reference on identical input and compares stdout,
stderr, the exit code (and any produced file) byte-for-byte. "Verified" means
the cross-check passed with the leak checker (`E-R06`) silent.

## 1. Capability matrix

| # | Capability | Evidence (app / harness) | Cases | Verdict |
|---|---|---|---|---|
| 1 | Filesystem: walk, create, copy, move, filter, report | `validation/a1_fileinspect`, `a2_treeops` | 20 + 18 | VERIFIED |
| 2 | Text/regex selection (`re` subset) | `validation/a3_textgrep` | 35 | VERIFIED (subset) |
| 3 | CSV parse / transform / write (RFC 4180) | `validation/b_csvdata` | 34 | VERIFIED |
| 4 | JSON parse / inspect / extract / serialize | `validation/c_json` | 43 | VERIFIED (arena-bounded) |
| 5 | Relational store: DDL, DML, queries, transactions | `validation/d_sqlite` | 10 | VERIFIED (in-memory only) |
| 6 | HTTP/1.1 client (GET, headers, body) | `validation/e_network` | 18 | VERIFIED (post-fix) |
| 7 | HTTPS client (TLS, chain + hostname validation) | `validation/e_network` | 18 | VERIFIED (explicit CA) |
| 8 | TCP client (connect, send, recv, close) | `validation/e_network` | 18 | VERIFIED |
| 9 | UDP client (datagram send/receive) | `validation/e_network` | 18 | VERIFIED |
| 10 | Subprocess execution + stdout/stderr capture | `validation/f_process` | 19 | VERIFIED (Windows-only) |
| 11 | Subprocess stdin (piped input) | `validation/f_process` | 19 | VERIFIED (≤ 4096 B) |
| 12 | argv handling (non-option arguments) | A3/B/C/D/E/F/G/H | 100+ | VERIFIED (ANSI, see gap 1) |
| 13 | Environment variables (get/set/has/remove) | `validation/f_process` | 19 | VERIFIED |
| 14 | UTC timestamp formatting | `validation/f_process` | 19 | VERIFIED |
| 15 | Threads + mutex (worker pool) | `validation/g_threads` | 18 | VERIFIED |
| 16 | Two-stage pipeline with join barrier | `validation/g_threads` | 18 | VERIFIED |
| 17 | Multi-subsystem integration (JSON+FS+hash+CSV) | `validation/h_manifest` | 13 | VERIFIED (ASCII paths) |
| 18 | SHA-256 hashing of file content | `validation/h_manifest` | 13 | VERIFIED |
| 19 | Memory leak checking (E-R06) across all apps | every harness | 228 | VERIFIED (0 leaks at exit) |
| 20 | Standalone PE (no interpreter/DLL beyond Windows) | every harness | 228 | VERIFIED |

Total validation cases: **228** = A1 20 + A2 18 + A3 35 + B 34 + C 43 +
D 10 + E 18 + F 19 + G 18 + H 13.

## 2. Not available / not exercised (documented holes)

| # | Capability | Status | Note |
|---|---|---|---|
| N1 | Unicode path names | **FAILS** | ANSI layer: bytes are code-page round-tripped or become `?` (gap 1) |
| N2 | Long paths (> `MAX_PATH`, 260) | **FAILS** | ANSI `CreateFileA`-style APIs, no `\\?\` support (gap 3) |
| N3 | Ctrl+C / console control handler | **ABSENT** | no signal or `SetConsoleCtrlHandler` API in the runtime or stdlib (gap 4) |
| N4 | Live public-internet HTTPS | manual only | `tests/tls_lib.rs` `s62_system_store_public_https` stays `#[ignore]`d; the suite is offline and reproducible |
| N5 | Streaming / interactive child I/O | **ABSENT** | capture windows are bounded and read once |
| N6 | HTTP keep-alive, chunked transfer, redirects | **ABSENT** | documented `http.mink` limits |
| N7 | On-disk database persistence | **ABSENT** | the sqlite engine's database is an in-memory `Str` blob |
| N8 | `ORDER BY` / `GROUP BY` / `JOIN` / `LIMIT` | **ABSENT** | engine is `SELECT *` / `SELECT COUNT(*)` + `WHERE` |

## 3. Prioritized gap list

**P0 — blocks real Windows file work**

1. **ANSI filesystem, no Unicode paths.** Evidence (H first run): reading
   `sub dir/naïve café.txt` succeeded but the path was *reported* as
   `na?ve caf?.txt` (code-page bytes), and `sub dir/nested/日本語.txt` was
   looked up as `???.txt`, so `fs_file_size` returned `-1` and the row was
   `..., -1, e3b0c44298fc1c14, ...` while Python reported `16,
   24d22f3d5e722ce4`. Fix direction: switch the runtime fs services to the
   `W` Win32 entry points (`CreateFileW`, `FindFirstFileW`, …) with UTF-8
   input, or document argv/paths as code-page only. *Linux untouched
   (frozen).*

2. **`MAX_LIVE_ALLOCS = 1024` caps simultaneously live allocations.**
   `src/runtime/abi.rs:107`. Measured: `a3_textgrep -c` works at 800 input
   lines and fails `E-R03` (exit 103) at 900; `b_csvdata agg` fails at
   2000 rows (~10000 field strings); `d_sqlite` fails at 2000 script lines.
   Any application that materialises a whole document as a `Vec<Str>` hits
   this after ~800-1000 elements. Fix direction: raise the table (24 B/entry,
   so 64k entries = 1.5 MiB of `.bss`) or add a bulk/block record mode.

**P1 — correctness/quality gaps**

3. **No long-path support.** The ANSI layer also caps paths at `MAX_PATH`
   (260). Fix direction: `\\?\` prefix handling together with gap 1.

4. **No console control handler.** Nothing in `src/runtime` or `stdlib`
   handles `CTRL_C_EVENT`/`SIGINT`, so a "graceful Ctrl+C shutdown" proof app
   cannot be written. Fix direction: `SetConsoleCtrlHandler` + an `on_signal`
   registration intrinsic (Windows), kept out of the frozen Linux emitter.

5. **JSON arena ceiling is lower than documented.** The header advertises a
   64KB arena / ~8000-byte input, but the serializer writes into arena slots
   7168..8191 and the parsed image grows from `input_len + 1`, so an input of
   5131 bytes (150 objects) already fails `E-R05` (exit 105) — the effective
   budget is `input + parsed ≤ 7168`. Fix direction: separate the output
   window or size it from `_ser_size`.

**P2 — performance and feature gaps**

6. **SQLite statement cost grows with database size.** Measured: 500
   sequential inserts took **17.9 s** in MINK vs **0.17 s** in Python (100x),
   because every `sql_exec` copies and re-parses the whole database text
   (the `_arena` builder copies the entire blob per statement). Fix direction:
   an incremental append path for `INSERT`.

7. **Bounded process capture.** stdout/stderr live in fixed runtime windows
   (overflow is `E-R02`, not truncation) and piped stdin is capped at one
   pipe buffer (4096 bytes).

8. **HTTP client scope.** `Connection: close` only, no keep-alive, no chunked
   decoding, no redirect following, 64-header and 4 MiB body caps.

9. **No on-disk database** (N7) and no SQL beyond the documented subset (N8).

**P3 — ergonomics (documented, not blocking)**

10. Read-borrowing intrinsics (`rt_str_concat`, `rt_print_str`, `rt_argv`,
    `rt_env_*`, `rt_map_*`, `rt_fs_*` reads) do not free arguments; a nested
    `rt_str_concat(a, rt_str_concat(b, ""))` leaks the inner temporary. Found
    in D, F and H. Fix direction: free the temporaries in the intrinsic, or
    make the checker flag nested calls.
11. User-function `Str` parameters are consuming and must be released by the
    callee — easy to forget (C, H).
12. `fail_msg`-style helpers that return `rt_stderr_write`'s byte count must
    not be used as an exit code (`return fail_msg(...)` produced exit 42/44
    in C).
