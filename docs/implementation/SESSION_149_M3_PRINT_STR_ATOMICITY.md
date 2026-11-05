# Session 149 — M3 P9: `rt_print_str` single-write line atomicity

**Scope:** the final M3 implementation item only — make one `rt_print_str`
call emit its complete line (payload plus CRLF) in one OS write. Windows
x86_64 only; Linux frozen. Version remains **1.0.4**. No push, tag, publish,
merge, rebase, reset, clean, stash, or amend.

## 1. Baseline

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `691ec38386bb37e204b834e61b0ff8de99a0ac65` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Version | `1.0.4` |
| Full suite (Session 148) | 3,164 passed / 0 failed / 1 ignored |
| Test binaries | 77 |
| Compiler warnings | 59 |
| Worktree before this session | 39 modified tracked, 25 untracked |

Session 143 had already fixed `rt_print_int`, `rt_print_float`, and
`rt_print_char`: each formats in per-call stack scratch and writes its text
plus CRLF in one `WriteStdout` call. `rt_print_str` was explicitly left with
the old two-write protocol.

## 2. Current path and pre-fix reproducer

`rt_print_str` called `StrValidate`, then:

1. `WriteStdout(payload, len)`;
2. `WriteStdout(crlf_label, 2)`.

Two concurrent calls could interleave those writes. The pre-fix stress probe
used eight threads, each printing one distinct 32-byte payload 3,000 times
(24,000 calls). Result:

```
lines 24,000
malformed/merged lines 11,606
empty lines 6,245
payload counts: 1496, 1565, 1556, 1550, 1515, 1576, 1534, 1602
```

Each payload should have appeared exactly 3,000 times. The mismatch is a
direct torn-line defect, not a crash or ordering preference.

## 3. Root cause

The payload and the shared CRLF constant were two independent
`WriteFile` operations. `WriteStdout` fetches the cached stdout handle and
performs one `WriteFile` per call; it does not combine adjacent writes and
there was no print lock. A thread could therefore be scheduled between its
payload write and its CRLF write, letting another thread's payload/CRLF land
between them.

This was not shared formatting scratch (the string payload is caller-owned),
not a stdout handle problem, and not the Session 143 float redirect. It was
purely write-count/line-assembly.

## 4. Invariant and contract

For one `rt_print_str(s)` call:

- the payload bytes are copied unchanged, including embedded NUL, CR, and LF;
- exactly one CRLF is appended after the payload;
- the complete payload+CRLF byte sequence is passed to **one**
  `WriteStdout`/`WriteFile` operation;
- no shared scratch, print lock, TLS buffer, or global line buffer is used;
- concurrent calls may appear in any order, but each call contributes one
  complete line (subject only to the documented long-line allocation note);
- empty strings produce exactly one CRLF line;
- no byte is truncated or translated.

The line is assembled in per-call storage:

| Payload length | Storage |
| --- | --- |
| `0 ..= 4,062` | 4,064-byte per-call stack buffer, no allocation |
| `4,063 ..` | temporary block from the validated runtime heap, freed immediately after the write |

The stack frame is 4,096 bytes: spills at `[rbp-8..-32]` and the buffer at
`[rbp-4096..-33]`. The earlier 4,112-byte layout was rejected during audit:
its buffer overlapped the `[rbp-32]` ownership flag, so a payload near 4,080
bytes overwrote the flag and sent a stack pointer to `rt_free` (E-R04/E-R07).
The final layout keeps the buffer and spills disjoint; boundary tests cover
4,062 (stack path) and 4,063 (heap path).

A long line uses `RuntimeService::Alloc(len + 2)` and `Free`, which is safe
here because printing a `Str` already requires `rt_init` (`StrValidate` reads
the image string region recorded by init). No new intrinsic, IR variant,
import, BSS cell, ABI offset, or Linux path was added. Allocation failure is
the existing allocator `E-R02`; it does not truncate or emit a partial line.

## 5. Implementation

`src/backend/emit/runtime.rs`, `emit_print_str`:

- reserve a 4,096-byte frame and keep four 8-byte spills outside the buffer;
- validate the string and save length/source;
- if `len <= 4,062`, point the line buffer at the stack area;
- otherwise `Alloc(len + 2)`, point the line buffer at the heap block, and
  mark it for freeing;
- copy the payload, append CR/LF, and call `WriteStdout` once;
- free only the heap path (byte-sized flag check);
- return 0.

`src/runtime/abi.rs`, PE imports, intrinsics, IR, lowering, and stdlib APIs
were untouched. Numeric/char print paths and the Session 143 redirect owner
tag were untouched.

## 6. Permanent tests

`tests/runtime.rs`:

- `print_str_binary_bytes_are_preserved` — embedded NUL/0xFF/CR/LF exact;
- `print_str_long_lines_at_and_above_the_stack_boundary` — 4,062-byte stack
  path, 4,063-byte heap path, and 65,536-byte heap path, byte-exact with one
  CRLF each.

`tests/threads_lib.rs`:

- `s149_concurrent_print_str_emits_one_intact_line_per_call` — 8 × 3,000
  distinct payloads; exact 24,000-line multiset, no empty/merged/partial
  lines, each payload exactly 3,000 times;
- `s149_concurrent_print_str_repeated_payload_count_is_exact` — 8 × 2,000
  identical payloads; exact 16,000-line count and content.

Existing coverage retained: normal and empty string lines, embedded escapes,
`rt_print_int`/`rt_print_float` concurrency, `rt_print_char`, and the
`rt_str_from_float` redirect regression.

## 7. Verification

| Check | Result |
| --- | --- |
| `cargo test --test runtime` | **28 passed / 0 failed** |
| `cargo test --test threads_lib` | **24 passed / 0 failed** |
| `cargo test --test parser` | **116 passed / 0 failed** (Session 148 green) |
| Full suite (`cargo test --no-fail-fast`) | **3,168 passed / 0 failed / 1 ignored**, 77 binaries, exit 0 |
| Delta vs Session 148 | **+4** (two runtime, two thread tests) |
| `cargo build --release` | exit 0; 59 existing warnings |
| Release concurrent probe | 24,000 lines, 0 malformed, 0 empty, 3,000 of each payload |
| Release long probe | 73,667 bytes exact (4,062 + 4,063 + 65,536 payloads and three CRLFs) |
| `git diff --check` | clean |

The full suite needs the established `tmp/m3_print/bin/python3` shim on
`PATH`; this is the known environment limitation, not a code failure.

## 8. Audit

- **Buffer safety:** buffer `[rbp-4096..-33]` and spills `[rbp-8..-32]` are
  disjoint; 4,062-byte payload + CRLF fills 4,064 bytes exactly. The 4,063
  path never touches the buffer.
- **Truncation:** none. Long lines allocate the exact `len + 2` and copy all
  bytes; OOM is a structured allocator error, not a short write.
- **CRLF placement:** always immediately after the copied payload, exactly
  once. Embedded CR/LF are payload and are not translated.
- **Embedded NUL/binary:** preserved (tested with NUL, 0xFF, CR, LF).
- **Concurrency:** no shared print scratch; each call has its own stack or
  heap line. No new lock/TLS/global state. The allocator's existing lock is
  only used for over-capacity lines.
- **Ownership:** heap line is freed after the write; stack line is never
  freed. The flag is checked as a byte, and the audit specifically caught and
  fixed the earlier 4,112-byte overlap.
- **Lifecycle:** string printing already depended on `rt_init` for
  `StrValidate`; no new initialization dependency was introduced.
- **Regression:** Session 143 numeric/char and redirect tests remain green;
  Session 148 parser tests remain green.
- **Architecture/ABI:** no import, BSS, IR, intrinsic, or ABI change.
- **Known limitations:** lines above 4,062 bytes allocate a temporary heap
  block; a failed allocation reports `E-R02`. Output ordering between threads
  is unspecified (only per-call line integrity is guaranteed). Linux remains
  frozen.

## 9. Git state and commit decision

HEAD remains `691ec38386bb37e204b834e61b0ff8de99a0ac65`; `origin/main`
remains `b860837a8a6039f304a503e3d85728a643ab470b`; version `1.0.4`.

No commit was created. `src/backend/emit/runtime.rs` also carries
uncommitted Sessions 144–147 network/HTTP work, and `tests/threads_lib.rs`
carries uncommitted Session 144 tests. File-level staging would absorb
unrelated work; a hunk split would produce an emitter commit whose surrounding
test/runtime modules are absent from HEAD. The verified changes remain in the
working tree, consistent with the Sessions 128–147 tree.

Final worktree after this record: **40 modified tracked, 26 untracked**
(25 pre-existing untracked + this record). `git diff --check` clean. No
push/tag/publish/reset/clean/stash/rebase/amend.

---

# M3 final audit / closure

M3 scope was carried from the M2 evidence and implemented in Sessions 140–149.
The historical session labels are kept as written in their records; this audit
does not renumber them.

| Session | Item | Current-tree evidence | Status |
| --- | --- | --- | --- |
| 140 | `if` statement vs if-expression in expression blocks | Parser snapshot/fallback; Session 140 tests; parser suite 116 | COMPLETE |
| 141 | `net_dial` Winsock error preservation + closure trailing-expression MIR panic | Record; network/MIR/closure suites; full suite | COMPLETE |
| 142 | `net_last_error()` before `net_init()` guard | Record; network tests; full suite | COMPLETE |
| 143 | Concurrent numeric/char print scratch and one-write lines; float redirect ownership | `tests/runtime` 28; `tests/threads_lib` 24; s143 tests green | COMPLETE |
| 144 | Thread-local HTTP last-error state | Record; `s144_thread_local_word_is_isolated_per_thread`; HTTP tests | COMPLETE |
| 145 | Bounded socket connect/read timeouts | `net_recv_timeout` / `net_connect_timeout`; network tests | COMPLETE |
| 146 | UDP receive-buffer parity | `u01`–`u11`; network tests | COMPLETE |
| 147 | HTTP/1.1 chunked decoding | `h55`–`h73`; HTTP fragmentation matrix | COMPLETE |
| 148 | `parse_block_expr` trailing-token recovery | Commit `691ec38`; 12 parser regressions | COMPLETE |
| 149 | `rt_print_str` one-write line atomicity | This record; 4 new regressions; release probes | COMPLETE |

Closure evidence:

- Full suite: **3,168 passed / 0 failed / 1 ignored**, 77 binaries, exit 0.
- Windows x86_64 release build: exit 0, 59 existing warnings.
- Session 148 parser fix: 116/116 parser tests.
- Session 143 print fixes: runtime 28/28 and threads 24/24, including the
  numeric concurrency and `rt_str_from_float` redirect regressions.
- Session 149 atomicity: pre-fix 11,606 malformed + 6,245 empty lines out of
  24,000; post-fix 0 malformed, 0 empty, exact 3,000 of each payload, in both
  debug and release builds.
- No new critical/P0/P1 defect is known. The only defect found during this
  session (buffer/flag overlap) was fixed and covered.
- No unrelated feature work was added. Known limitations remain recorded per
  session (Linux frozen; UDP max payload; chunked extensions/trailers ignored;
  timeout bounds; long print lines allocate and can report E-R02; thread
  ordering unspecified).

**M3 is COMPLETE.** The next milestone is **M4 — Developer Experience
Benchmark** (comparison against Python, Rust, Go, and C#). M4 implementation
must not begin by adding ecosystem, platform, or language features.

## Files changed in Session 149

- `src/backend/emit/runtime.rs` — `emit_print_str` line assembly, stack/heap
  boundary, single write
- `tests/runtime.rs` — binary and long-line boundary regressions
- `tests/threads_lib.rs` — two concurrent string-line regressions
- `docs/implementation/RUNTIME_IMPLEMENTATION.md` — print contract
- `docs/roadmap/IMPLEMENTATION_ROADMAP.md` — M3 closure status
- `docs/implementation/SESSION_149_M3_PRINT_STR_ATOMICITY.md` — this record
