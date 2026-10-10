# Session 125 — `process_run_input`, unbounded capture growth, Linux heap alignment

**What landed:** child-stdin support (`rt_process_run_input`), grow-on-demand
stdio capture, `stdlib/process.mink`'s `process_run_input` wrapper, a
on-demand-growth rewrite of `http_recv_all`, and a Linux ELF `.bss` alignment
fix that had silently broken the heap of every Linux program with static
data.

**Linux:** frozen for *features*, not for correctness. The new process service
is emitted only by the Windows runtime; Linux keeps the service table total
with the established `-1` stub. The `.bss` alignment fix is a backend
correctness repair that changes no Linux capability and no Windows output.

## 1. `rt_process_run_input(cmd, input) -> Int`

Feeds `input` to a child's stdin, captures stdout/stderr, and returns the
exit code — the stdin counterpart of `rt_process_run`.

- Emitted by `emit_process_run_common(code, with_input)` in
  `src/backend/emit/runtime.rs`; both process services share one code path so
  the pipe setup, the drain loop and the exit-code read can never drift apart.
- Wired as `RuntimeService::ProcessRunInput` (`src/backend/ir.rs`,
  `src/backend/lower.rs`, `src/runtime/intrinsics.rs`). The Linux runtime
  binds it to `emit_linux_stub_minus_one`, the answer `process_run` already
  returns for a failed launch.
- `stdlib/process.mink`: `process_run_input(cmd, input) -> Int`, which frees
  both strings it consumes (mirrored into `npm/mink/stdlib/process.mink`).

Implementation notes that are easy to get wrong:

- `R10`/`R11` are **volatile across calls** on the Windows x64 ABI. The write
  loop keeps its "bytes left" count in a frame slot (`[rbp-208]`), reloading
  and re-storing it around every `WriteFile`, because holding it in a
  register made the parent flood the child with its own stack memory
  (`FINDSTR: Line 46 is too long`) and hang once the pipe filled.
- The child must inherit the **read** end of the stdin pipe, so the parent's
  write end has `HANDLE_FLAG_INHERIT` cleared (`SetHandleInformation`,
  `[rbp-64]`) before `CreateProcessA`. Leaving it inheritable means the child
  keeps a copy of the write end open and the EOF never arrives.
- `STARTUPINFOA` (x64) places `hStdInput` at offset **80** (not 76), with
  `dwFlags`/`STARTF_USESTDHANDLES` at 60.
- Input delivery is capped at one pipe buffer (`STDIN_PIPE_LIMIT`, 4096
  bytes) and the cap is documented: writing the whole input synchronously
  deadlocks against a child that does not read stdin while the parent blocks
  on the child's stdout pipe.
- The bytes come from `ptr + 8` — a MINK `Str` is an 8-byte length prefix
  followed by the data.

## 2. Capture grows instead of truncating

`STDIO_CAPTURE_WINDOW_BYTES` (8 MiB) is a *window*, not a cap: both streams
grow on demand, and a stream that exceeds the window fails the program with
the structured `E-R02` rather than silently discarding bytes. The Session 97
regression expectations were updated to match (`drains_fully=5002`,
`just_above_pipe_buffer=4097`, `boundary=4098`, `64k=70002`,
`1mb=1000002`, `s97_large_stderr_captured=12000`).

## 3. `http_recv_all` grows its accumulation buffer

`stdlib/http.mink` raised `MAX_BODY()` to 4 MiB, and `http_recv_all` allocated
`MAX_BODY() + 8192` **up front**. On Linux — whose heap is a fixed 4 MiB
`.bss` arena — that allocation cannot fit, so every Linux HTTP program died
with `E-R02` before reading a byte.

The accumulation buffer now starts at 8 KiB and doubles (clamped to
`max_size`) as the body arrives, copying the prefix on each growth, exactly as
`stdlib/tls.mink`'s `_recv_all` already does. The overflow rule is unchanged:
one extra `net_recv(sock, 1)` distinguishes an exactly-limit response from an
overflow, and the result is still copied into an exact-length `Str` so
`rt_str_len(response)` equals the bytes actually received.

## 4. Linux ELF `.bss` alignment (the real bug behind `linux_h01`)

With the buffer fixed, the Linux HTTP client still exited `107` (`E-R07`,
misaligned free) on its first request. The cause was not the HTTP code:

- The ELF patch resolver places `.bss` at `data_rva + data.len()`
  (`src/backend/emit/x86_64.rs`), and the loader places it at
  `p_vaddr + p_filesz` (`src/backend/emit/elf.rs`). The runtime BSS layout
  puts the heap arena at a 16-byte-aligned offset (`BSS.arena = 1488`), and
  `rt_alloc`/`rt_free` require 16-byte-aligned blocks.
- A program whose module-binding image is 8 bytes (any program importing a
  module with bindings — `http.mink` included) therefore had its entire `.bss`
  shifted by 8: the arena landed at `0x40e5d8` and **every** heap block was
  misaligned, so every `rt_str_free` failed `E-R07`. Programs with no `.data`
  (the loader reported `p_filesz = 0`) escaped it, which is why the small
  allocator probes passed while any real library program failed.

The fix pads `.data` to a 16-byte multiple (`src/backend/emit/x86_64.rs`, with
`build_elf` enforcing the same invariant), keeping the patcher and the loader
in agreement and the arena aligned. This is a single-line invariant worth
keeping in mind when touching either layout.

## Tests

- `tests/process_lib.rs`: `s125_input_reaches_child_stdin`,
  `s125_input_eof_is_delivered_to_child` (proved by `sort` finishing),
  `s125_empty_input_still_runs_child`, `s125_exit_code_preserved_with_input`,
  and `s125_child_that_ignores_stdin_does_not_deadlock` (8 KiB of heap input
  against a child flooding stdout). All guarded by a timeout helper that
  redirects child output to files, so a regression fails the test instead of
  wedging the suite.
- `tests/stdlib_ownership.rs`: `process_run_variants_release_heap_inputs`
  (heap `cmd` + `input` must exit 0 — a leak is `E-R06` / exit 106).
- `linux_h01_http_execution_verified` (multi-recv, 20 KB body, refused and
  premature-close paths) passes again.

## Test-environment note

Windows reserves whole TCP port ranges (WSL/Hyper-V "excluded port ranges",
e.g. 19440–20039 on the development machine), and `bind` then fails with
`WSAEACCES` (10013). The live-socket tests (`http_lib` h40, `network_lib`
n20/n21/n22/n96/n97) now try a list of candidate ports and use the first that
binds instead of assuming a fixed port is free.