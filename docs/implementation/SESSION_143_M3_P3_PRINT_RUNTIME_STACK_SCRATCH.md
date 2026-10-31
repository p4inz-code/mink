# Session 143 — M3 P3: print runtime stack scratch and lifecycle

**Scope:** the Windows x86_64 print subsystem only — `rt_print_int`,
`rt_print_float`, `rt_print_char`, their formatting/output scratch, the write
path, and the print initialization/lifecycle contract. Two print defects were
reproduced and fixed: the process-global `.bss` formatting scratch shared by
concurrent printers, and the two-write line protocol. While investigating the
same shared state, a third, directly related defect was found and fixed: the
`rt_str_from_float` write redirect captured another thread's prints.

Windows x86_64 only. Linux is untouched. Version stayed **1.0.4**. No push,
tag, publish, merge, rebase, reset, clean, stash or amend. Every pre-existing
dirty file was preserved.

**Outcome:** each numeric print formats in per-call stack scratch and emits its
text and CRLF in one `WriteStdout` call; the float-to-string redirect is tagged
with the arming thread's id. Four permanent regressions were added. The print
path has no initialization dependency: removing `rt_init` from the entry stub
produces identical output. Full suite: **3,107 passed, 0 failed, 1 ignored**
(with the `python3` shim on `PATH`).

## 1. Baseline

| Item | Value |
| --- | --- |
| Branch | `main` |
| HEAD | `27ce9c84d3552eca02f7586569a7378d09eee00c` |
| `origin/main` | `b860837a8a6039f304a503e3d85728a643ab470b` |
| Divergence | ahead 5, behind 1 |
| Staged | none |
| Modified tracked | 35 |
| Untracked | 20 |
| `git diff --check` | clean |
| Version / warnings | **1.0.4** / 59 warnings |

The working tree carried the uncommitted Sessions 128–142 work. `git status`,
`git diff --stat`, `git diff --check`, `git branch -vv` and the log graph were
captured before any edit. The print functions themselves were unchanged in
HEAD; the pre-existing runtime diff was in `emit_str_from_int` (Session 134)
and the networking services (Sessions 128–142). Those hunks were preserved.

Baseline tests: the previous full-suite figure was **3,103 passed, 0 failed,
1 ignored** (Session 142). Focused pre-fix runs on this tree:

| Command | Result |
| --- | --- |
| `cargo test --test threads_lib s143` (new tests) | **2 failed** — the defect reproductions |
| `cargo test --test runtime print_int_float` (new matrix) | 1 passed (single-thread behavior was already correct) |

## 2. Shared-buffer defect

### Reproducer A — concurrent integers

Eight threads each print 3,000 distinct values (`base * 100000 + i`,
`base = 1..8`), so the correct output is 24,000 lines and every value appears
exactly once.

```
pre-fix:  lines 24,000 | 13,895 lines held a valid value (13,857 distinct)
          10,105 malformed | 5,385 empty lines
          37 values appeared twice — one thread wrote another thread's digits
post-fix: lines 24,000 | all values exact and unique | 0 empty lines
```

### Reproducer B — concurrent floats

Four threads each print one thread-unique constant (`1.5`, `2.5`, `3.5`,
`4.5`) 400 times.

```
pre-fix:  lines 1,600 | 804 single-value lines | 421 empty lines
          counts 194 / 224 / 173 / 213 instead of 400 each
post-fix: lines 1,600 | 400 of each value | 0 empty lines
```

### Root cause

1. **Process-global formatting scratch.** `emit_print_int` built its digits in
   `.bss print_buf`; `emit_print_float` built its output in `print_buf` and its
   exact expansion in `.bss dtoa_words` / `dtoa_digits`. Formatting and the
   `WriteStdout` call are not atomic, so another thread could overwrite the
   text between generation and writing. That is the defect Session 134 fixed
   for `rt_str_from_int` and Session 134 §B1 carried for the print paths.
2. **Two writes per line.** Every numeric print called `WriteStdout` twice:
   once for the digits and once for the `\r\n` constant in `.text`. Two
   concurrent calls could interleave those two writes, producing a merged line
   and an empty line. This produced most of the observed corruption (5,385
   empty lines) even when the digits themselves were intact.

The following were checked and are **not** the defect: capacity (the integer
path needs 20 bytes of the 32-byte buffer; the float format never exceeds 24
characters), termination (all lengths are explicit; there is no NUL contract),
stale length (every call computes its own length), a shared length word (none),
buffer reuse across calls (irrelevant once the length is exact), and cleanup
(none exists). Splitting the buffers was not needed: the buffers had to become
per-call, not separate.

## 3. `rt_print_int` initialization lifecycle

Proven from the emitter, not assumed:

- There is no print initialization flag, no allocation, and no print cleanup.
- `rt_init` writes only `cursor`, `free_head`, `heap_base`, `heap_committed`,
  `str_data_start`, `str_data_end` and `rng_state`. The print path reads none
  of them.
- `WriteStdout` fetches the stdout handle with `GetStdHandle` on the first
  write and caches it; when the handle is absent or `INVALID_HANDLE_VALUE` the
  write is skipped and the exit code stays authoritative. Nothing is required
  before the first print beyond the loader's zero-filled `.bss`.
- No supported MINK-facing path reaches a print before `rt_init`: the PE and
  ELF entry stubs call `Init` before `main`. To test the underlying contract,
  the `call Init` was removed from the PE entry stub in a temporary experiment,
  the compiler was rebuilt, and a print-only program was run:

  ```
  with rt_init:    0 42 -7 9223372036854775807 -9223372036854775808
                   0 1.5 -2.25 0.014999999999999999 123456.25 1e+308 …
  without rt_init: byte-identical output, exit 0
  ```

  The source was restored and verified byte-identical (`cmp`), `git diff` for
  `src/backend/emit/x86_64.rs` is empty, and the normal compiler was rebuilt.
  The correct contract is therefore: **print needs no initialization and no
  ordering guarantee; it must not, and does not, read `rt_init` state.** No
  guard was added because there is no invalid state to guard.
- There is no runtime shutdown API (only `net_cleanup`, which clears network
  state), so "print after cleanup" does not exist. A print after
  `rt_str_from_float` was exercised in the new matrix test.
- The integer and float paths use the same lifecycle and no shared state;
  neither depends on the other.

## 4. Implementation

`src/backend/emit/runtime.rs`:

| Service | New scratch | Write |
| --- | --- | --- |
| `rt_print_int` | 48-byte frame; digits downward from `[rbp-17]`, sign below, CRLF at `[rbp-16]`/`[rbp-15]` | one `WriteStdout(digits + CRLF)` |
| `rt_print_float` | 1,440-byte frame: 40-byte output at `[rbp-88..-48)`, 320-byte big integer at `[rbp-408..-88)`, 1,024-byte digit expansion at `[rbp-1432..-408)` | CRLF appended; one `WriteStdout`; the Inf/NaN path appends at `len`/`len+1` too |
| `rt_print_char` | 16-byte frame: char + CRLF at `[rbp-16..-14)` | one `WriteStdout` |

Every buffer is per-call stack memory. The 1,440-byte frame keeps the stack
16-byte aligned at calls and removes the last allocation-free dependency on
`.bss` from the print path. The `.bss` `print_buf`/`dtoa_words`/`dtoa_digits`
cells remain in the ABI for the Linux runtime, which is frozen.

**Directly related fix — float-to-string redirect.** `rt_str_from_float`
formats by arming `WRITE_REDIRECT` and calling `rt_print_float`; the write thunk
appended every write into `float_sink` while the flag was set. The flag was
process-global, so a concurrent `rt_print_int` was appended into the
formatter's sink: in the probe only **126 of 3,000** printer lines reached
stdout and **117 of 3,000** formatted strings were corrupted. The flag now
stores the arming thread's id, and the write thunk compares it with
`RuntimeService::ThreadId` before redirecting. No new BSS cell, no new import,
no lock and no thread-local storage; the normal path is one compare, and the
redirect path is entered only by the arming thread. After the fix: 3,001 lines
(3,000 exact integers + the `0` bad-string count), 0 corrupt strings.

No intrinsic declaration, IR variant/arity, numeric semantics or public API
changed. No unrelated compiler behavior was touched.

## 5. Tests added

| File | Test | Kind |
| --- | --- | --- |
| `tests/runtime.rs` | `print_int_float_interleaving_and_repeats_are_exact` | exact single-thread matrix: first/repeated/zero/negative/Int extremes, fractional and extreme Float values, every int/float ordering, and `rt_str_from_float`/`rt_str_free` between prints |
| `tests/threads_lib.rs` | `s143_concurrent_print_int_emits_one_intact_line_per_call` | 8 × 3,000; every value exactly once, no merged/empty lines |
| `tests/threads_lib.rs` | `s143_concurrent_mixed_int_and_float_lines_stay_intact` | 4 × 500 alternating int/float; exact int multiset and 500 of each float |
| `tests/threads_lib.rs` | `s143_str_from_float_does_not_swallow_another_threads_print` | formatter + printer concurrently; 3,000 intact int lines, 0 bad strings |

Pre-fix evidence (actual runs, not descriptions):

```
threads_lib s143 (first two): FAILED — merged/duplicate values, line count intact
redirect test pre-fix:        FAILED — got 230 lines, expected 3,001
runtime matrix pre-fix:       passed (single-thread output was already exact)
```

Post-fix: all four pass, and no existing test was changed, skipped or weakened.

## 6. Verification

| Check | Result | Classification |
| --- | --- | --- |
| `cargo build` | success, 59 warnings (unchanged) | PASS |
| `cargo test --test threads_lib s143` | **3 passed, 0 failed** | PASS |
| `cargo test --test runtime` | **26 passed, 0 failed** | PASS |
| `cargo test --test scalar_types` | **27 passed, 0 failed** | PASS |
| `cargo test --test strings` | **63 passed, 0 failed** | PASS |
| `cargo test --test session99` | **13 passed, 0 failed** | PASS |
| Int probe (8 × 3,000) | 24,000 lines, exact multiset | PASS |
| Float probe (4 × 400) | 1,600 lines, 400 of each | PASS |
| Redirect probe | 3,001 lines, 0 bad strings | PASS |
| Pre-init experiment | byte-identical output, exit 0 | PASS |
| `cargo build --release` | success, 59 warnings | PASS |
| Release compiler, matrix probe | output identical to debug | PASS |
| Release compiler, int probe | 24,000 lines, exact multiset | PASS |
| Full suite (`cargo test --no-fail-fast`, `python3` shim on `PATH`) | **3,107 passed, 0 failed, 1 ignored** | PASS |
| `git diff --check` | clean | PASS |

The shim is the Session 142 convention: `python3` on this machine resolves to
the Windows Store stub, so `tests/tls_lib.rs` cannot start its fixture server
(`bad port line from TLS test server: ""`). The 18 fixture-backed TLS tests fail
before any MINK code runs; with `tmp/m3_print/bin/python3.exe` (the Python
launcher) first on `PATH`, all of them pass. This is an environment limitation,
not a code failure. The suite total is Session 142's 3,103 plus the four new
tests.

## 7. Audit

| Area | Result |
| --- | --- |
| Initialization | No flag, no allocation, no cleanup; no cell written by `rt_init` is read. The no-init experiment is byte-identical, so the contract is proven rather than assumed. No guard was added. |
| Buffer — capacity | Int: 20 bytes max in 48; float: 24 chars max in 40, 767 digits max in 1,024, 2,548-bit integer max in 2,560; char: 3 bytes in 16. |
| Buffer — length/termination | Every write length is computed from the write pointer and the buffer end; no NUL is read or written. |
| Buffer — reuse/stale/cross-type | All scratch is per-call stack memory; no byte survives a call, so int/float/char cannot see each other's text. |
| Output | Int and float bytes and formatting are unchanged; mixed/repeated calls are exact; the string formatter cannot corrupt or swallow another thread's print. |
| Architecture | Intrinsic declarations, IR variant and arity, backend emission and ABI are unchanged; only the internal scratch location, write count and redirect owner changed. |
| Concurrency | Each print is one `WriteFile`; concurrent calls now cannot overwrite digits or split a line. Inter-thread ordering remains unspecified. No lock/TLS was added. |
| Tests | Four permanent regressions; pre-fix failures captured; no skipped or weakened assertions; existing coverage intact. |
| Scope | Emitter print services and the directly related redirect, four tests, runtime documentation and this record. Linux, networking, compiler and numeric semantics untouched. |

## 8. Known limitations

1. Linux's `rt_print_int`/`rt_str_from_int` still use `.bss print_buf` and two
   writes. Linux is frozen and outside this Windows session; the ABI cell is
   retained.
2. `rt_print_str` still writes the string and then the CRLF in two calls, so
   concurrent string prints can merge lines. It does not use a shared
   formatting buffer and was outside this defect.
3. `WRITE_REDIRECT` and `float_sink` remain process-global. The owner tag makes
   only the arming thread eligible to redirect; two `rt_str_from_float` calls
   on one thread cannot overlap by construction.
4. The stdout/stderr handle caches and `bytes_written` are shared cells. They
   are write-once caches / scratch and no output decision reads them.
5. Concurrency guarantee is per print call, not a global line ordering.
6. The full suite needs the `python3` shim on this machine.

## 9. Files changed

- `src/backend/emit/runtime.rs` — int/float/char stack scratch, one write per
  print, redirect owner tag
- `src/runtime/abi.rs` — `print_buf` and `write_redirect` comments
- `tests/runtime.rs` — exact print matrix
- `tests/threads_lib.rs` — three concurrency regressions
- `docs/implementation/RUNTIME_IMPLEMENTATION.md` — print lifecycle/scratch
  contract
- `docs/implementation/SESSION_143_M3_P3_PRINT_RUNTIME_STACK_SCRATCH.md` — this
  record

## 10. Git

The fix, the four regressions, the runtime documentation and this record form
one focused commit on `main`, built from the HEAD blobs plus only this
session's hunks; the working tree continues to carry the uncommitted Sessions
128–142 work. The commit hash and the final tree state are recorded here after
the commit is created.

## 11. Remaining M3 work

1. Thread-local HTTP last-error.
2. Socket connect/read timeouts.
3. UDP buffer parity.
4. HTTP chunked decoding.
5. `parse_block_expr` trailing-token recovery (Session 140 §9.3).
6. `rt_print_str` single-write line atomicity (print-adjacent, not this
   defect).
7. Linux print scratch and one-write lines, if Linux is ever unfrozen.
