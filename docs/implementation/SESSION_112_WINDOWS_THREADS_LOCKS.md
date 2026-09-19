# Session 112 — Windows threads + locks (R19, R21, S71)

**Parity blockers closed:** R19 `Threads`, R21 `Locks / synchronization
primitives`, S71 `Threads + locks` — MISSING → **VERIFIED**
**Parity-blocking set:** 9 → 7 (R20, S62, S73, P04, P05, P06, P09 remain)
**Linux:** frozen. `src/backend/emit/linux_runtime.rs` and the ELF backend were
not touched; the Windows-only services live in `src/backend/emit/runtime.rs`
and the shared changes (`src/runtime/error.rs`, `src/runtime/intrinsics.rs`,
`src/backend/{ir,lower}.rs`) recompile the Linux target unchanged (the new
`RuntimeService` variants are emitted only by the Windows runtime).

## What landed

- Real OS threads on the Windows runtime: `rt_thread_spawn(fn, arg) -> Int`,
  `rt_thread_join(handle) -> Int`, `rt_thread_id() -> Int`
  (`CreateThread` / `WaitForSingleObject` / `CloseHandle` /
  `GetCurrentThreadId` / `ExitThread` imports).
- Locks: `rt_mutex_new() -> Int`, `rt_mutex_lock`, `rt_mutex_unlock`,
  `rt_mutex_free` — an 8-byte lock word in the MINK heap acquired with an
  atomic `xchg`.
- `rt_ptr_to_int(p) -> Int` / `rt_int_to_ptr(i) -> Ptr<Int>`: a word
  reinterpretation. Without it a `Ptr<Int>` cannot be stored by
  `rt_mem_store` (which takes an `Int` value), so a thread could not be handed
  a pointer to shared state.
- `stdlib/threads.mink` — `thread_spawn`/`thread_join`/`thread_self` and
  `lock_new`/`lock_acquire`/`lock_release`/`lock_free` (mirrored into the npm
  bundle).
- `tests/threads_lib.rs` — 17 permanent tests; each compiles a real Windows PE
  through the `mink build` CLI and executes it.
- `examples/threaded_work/main.mink` — a small end-to-end proof application.
- `E-R12` (`ThreadCreateFailed`) added to the runtime error catalog.

## Architecture

**Thread trampoline.** `CreateThread` calls a native x64 callback, but MINK
functions use a stack convention: the caller pushes an alignment pad (when the
argument count is odd) and then the arguments rightmost-first, so argument 1 is
at `[rbp + 16]`. The trampoline is a hand-written stub in `.text` that

1. saves `rcx` (`lpParameter`, a pointer to the thread control block),
2. loads `fn` from `block[1]` and `arg` from `block[2]`,
3. pushes the pad word and the argument and `call`s the MINK function,
4. stores the returned word into `block[3]`, and
5. calls `ExitThread(0)`.

Every call site is 16-byte aligned: entry `rsp ≡ 8 (mod 16)`, `push rbp` makes
it 0, `sub rsp, 32` keeps it 0, and the pad + argument total 16 bytes, so the
MINK `call` sees a 16-aligned `rsp` exactly like a generated caller would.

**Thread control block.** `block[0]` is the OS handle, `[1]` the MINK function
pointer, `[2]` the argument, `[3]` the result. `rt_thread_spawn` allocates the
block with the runtime allocator and returns its address as an opaque word
handle; `rt_thread_join` waits, closes the kernel handle, reads `block[3]`,
frees the block and returns the result. The block is therefore *owned by the
join*: a spawn without a join is a genuine leak and the exit-time scan reports
`E-R06`. The result write happens-before the join because the join only reads
after `WaitForSingleObject` has observed thread termination.

**Shared runtime state.** Every thread shares one arena, one free list and one
liveness table, so the allocator and the raw memory accessors
(`rt_mem_load`/`rt_mem_store`) take a runtime-wide spin lock. The lock is a
plain test-and-set — `mov rax, 1; xchg [rt_lock], rax; test rax, rax; jnz` —
using a RIP-relative `xchg` so it clobbers only `rax`, and a RIP-relative
immediate store to release so a live result in `rax` survives. It is
intentionally **not** reentrant: the allocator, `rt_free` and the accessors
never nest, and none of them calls another locked service. The error paths call
`rt_fail`, which never returns, so no path exits holding the lock.

**User locks.** `rt_mutex_new` allocates and clears an 8-byte word;
`rt_mutex_lock` spins on `xchg`; `rt_mutex_unlock` stores 0; `rt_mutex_free`
returns the word to the allocator. This is deliberately non-reentrant, matching
Python's `threading.Lock` (a second acquire from the same thread deadlocks).

Crucially, the runtime's per-access lock does **not** make a user
read-modify-write atomic: the load and the store are two separate critical
sections, so two threads can still interleave between them. That is why
`s71_unlocked_increments_are_lossy` exists as the control for the locking
tests.

## Defects found and fixed (all by native execution)

1. **Clobbered frame slots.** The first `rt_thread_spawn` wrote its spills to
   `[rbp-8]`/`[rbp-16]`/`[rbp-24]` without reserving them, so the `rt_alloc`
   call overwrote them (Windows has no red zone). Fixed with an explicit
   `sub rsp, 16` frame.
2. **`CreateThread` stack arguments.** The first version passed the two stack
   arguments at `[rsp+0]`/`[rsp+8]` instead of after the mandatory 32 bytes of
   shadow space, and used the wrong register for the block pointer. Fixed to
   `sub rsp, 48` with the arguments at `[rsp+32]`/`[rsp+40]`.
3. **Missing shadow space.** `rt_thread_join`, `rt_thread_id` and the
   trampoline's `ExitThread` call had no shadow space, and the trampoline's
   `sub rsp, 40` left `rsp` misaligned for the MINK call.
4. **Result delivery.** `rt_thread_join` originally returned
   `WaitForSingleObject`'s status rather than the thread function's result;
   the control block now carries the result so `join` can return it.
5. **Kernel-handle leak.** A spawn/join cycle originally never closed the
   thread handle.
6. **Type friction.** A `Ptr<Int>` could not be stored by `rt_mem_store` and
   could not be passed to `rt_thread_spawn`, so a thread could never receive
   shared state. Fixed by adding `rt_ptr_to_int`/`rt_int_to_ptr`.

Two defects were found by running the existing full regression, not by the new
thread work:

7. **Wrong command name in `mink test` diagnostics.** `mink test` shares
   `parse_build`, which hard-coded `build` in every message, so `mink test`
   with no path reported `missing path argument for 'build'`. `parse_build`
   now takes the command name and the message names the command the user
   typed (`src/cli.rs`).
8. **Stale CLI test.** `tests/release.rs::cli_unknown_commands_rejected`
   asserted that `mink test` is an unknown command — untrue since the `test`
   subcommand landed in Session 108. The test now uses genuinely unknown names
   (`fmt`, `frobnicate`) and a new `cli_test_command_requires_a_path` pins the
   `build`-vs-`test` diagnostic fix. This test had been failing since Session
   108 and was masking a real (if small) diagnostic defect.

## Verification

Every test compiles a genuine Windows PE through the real `mink build` CLI and
runs it as a separate process; a clean exit is also the leak proof, because
`rt_exit` runs the arena validation and the leak scan (`E-R06` = exit 106).

| Test | What it proves |
|---|---|
| `r19_spawn_join_returns_thread_result` | the thread runs and `join` returns its result |
| `r19_thread_id_is_a_live_tid` | each thread has its own positive OS thread id |
| `r19_repeated_spawn_and_join` | 200 sequential spawn/joins, allocator reuse, no leak |
| `r19_many_threads_compute_and_join` | 8 concurrent threads, per-handle results |
| `r19_thread_allocates_in_parallel` | concurrent allocation/free integrity under the runtime lock |
| `r19_thread_uses_strings_and_printing` | thread ↔ string interop (allocation, validated access) |
| `r19_ptr_cast_round_trips_a_context_pointer` | pointer ↔ word cast preserves the address |
| `r19_stdlib_threads_and_locks_module` | the shipped `stdlib/threads.mink` public path |
| `r19_example_threaded_work_is_deterministic` | the proof app, twice, same closed-form total |
| `s71_mutex_uncontended_acquire_release` | 1000 lock/unlock cycles, no lockup |
| `s71_mutex_serializes_two_threads` | 2×1000 locked increments = exactly 2000 |
| `s71_mutex_serializes_four_threads_under_contention` | 4×5000 = exactly 20000, twice |
| `s71_mutex_excludes_threads_from_the_critical_section` | occupancy counter never exceeds 1 inside the critical section |
| `s71_unlocked_increments_are_lossy` | the control: the same workload without a lock is not exact |
| `s71_mutex_repeated_construction_and_destruction` | 200 new/free cycles, no leak |
| `s71_released_lock_is_reacquirable_after_join` | a joined worker leaves the lock free |
| `s71_mutex_protects_a_shared_accumulator` | locked accumulation is exact |

Native probes run during development (not committed as tests) showed the
unprotected 4-thread workload producing 8745 / 13450 / 9094 where the locked
workload produced 20000 three times in a row.

## Documented limits (not defects)

- **Non-reentrant lock.** A second `lock_acquire` from the same thread
  deadlocks, like Python's `Lock`. There is no owner tracking, so no
  `RuntimeError` is raised as CPython would; MINK has no exceptions.
- **No thread-local storage.** A thread has one `Ptr<Int>` argument; anything
  else must be passed through it. `rt_thread_id` is the only per-thread value
  the runtime exposes.
- **No cancellation, no priorities, no timed join, no condition variables, no
  thread pools.** `rt_thread_join` waits forever (`INFINITE`).
- **Busy-wait locks.** Both the runtime lock and user locks spin. They are
  correct but not fair, and a heavily contended lock burns its slice.
- **No stack-size control.** `CreateThread` is called with the default stack
  size.
- **All threads share one arena.** A thread that exhausts the 1 MiB heap makes
  every other thread fail with `E-R02`.

## Matrix updates

R19 VERIFIED, R21 VERIFIED, S71 VERIFIED; `Blocks = Y` cleared, and the four
stale `Blocks = Y` flags on already-VERIFIED rows (S41, S75, S78, T07, with
S41 also carrying a stale `P1`) cleared so that the P1 set equals the
`Blocks = Y` set again. Every aggregate in §10 was recomputed mechanically by
row scan (232 rows; VERIFIED 92, MISSING 77, PARTIAL 34; P1 7).

## Final state

- Parity-blocking set: **7** (R20, S62, S73, P04, P05, P06, P09).
- `tests/threads_lib.rs` 17/17; regression re-run green for
  `collections_lib` 35, `strings_lib` 85, `encoding_lib` 75, `closures` 26,
  `zlib_lib` 15, `zip_lib` 10, `sqlite_lib` 16, `re_lib` 33, `session99` 13,
  `session100` 14, `time_lib` 29, `filesystem_lib` 45, `process_lib` 80,
  `network_lib` 26, `http_lib` 35, `json` 61, `crypto_lib` 18, `hashing_lib` 24,
  `math_lib` 106, `csv_lib` 14, `random_lib` 15, `logging_lib` 13,
  `assert_lib` 16, `runtime` 24, `ownership` 43, `backend` 46, `adversarial` 93,
  `packages` 5, `release` 68, `cli` 74 (1 ignored), `smoke` 13,
  `test_runner` 12, plus `cargo test --lib` 62 and the npm-bundle drift guard.
- `windows_hardening` 15/18 in parallel, **18/18 with `--test-threads=1`** (15s).
  The three parallel failures (`http_windows_post_exact_body_roundtrip`,
  `http_windows_split_server_multi_recv_and_errors`,
  `tcp_mink_server_echo_repeated_connections`) all pass individually and
  serially: `free_port()` binds an ephemeral port, closes it and only then
  hands the number to the program under test, so a sibling test can take the
  port in between. That is the known port-contention infrastructure flake, not
  a product defect.
- `cargo fmt --check` clean; `cargo clippy --all-targets` exit 0 (pre-existing
  warnings only).
- Linux frozen.
