# Session 114 — Windows async / event loop (R20, S73)

**Parity blockers closed:** R20 `Async / event loop`, S73 `async/await + event
loop` — MISSING → **VERIFIED**
**Parity-blocking set:** 7 → 5 (S62, P04, P05, P06, P09 remain)
**Linux:** frozen. The Windows-only services live in
`src/backend/emit/runtime.rs`; the shared changes (`src/runtime/error.rs`,
`src/runtime/intrinsics.rs`, `src/backend/{ir,lower}.rs`, `src/parser/*`,
`src/runtime/abi.rs`) recompile the Linux target unchanged — every new
`RuntimeService` variant is emitted only by the Windows runtime, and the
`async fn` / `await` desugar is a front-end transformation that produces the
same ordinary call and codegen path.

## What landed

- A real task runtime on the Windows side: `rt_task_spawn(fn, arg) -> Int`
  (handle), `rt_task_spawn0(fn) -> Int` (zero-argument form the language
  surface uses), `rt_task_await(handle) -> Int` (wait, collect, return the
  result word), `rt_task_run() -> Int` (collect every outstanding task,
  returning how many were collected), `rt_task_pending() -> Int` and
  `rt_task_stop()`.
- The language surface: `async fn name(args) -> T { body }` and `await expr`
  (`src/parser/mod.rs`: `parse_async_fn`, the `await` prefix in `parse_unary`,
  `pub`-qualified async fns handled in `parse_pub_items`).
- `stdlib/tasks.mink` — `task_spawn`/`task_await`/`task_run`/`task_pending`/
  `task_stop` (mirrored into the npm bundle).
- `tests/async_lib.rs` — 23 permanent tests; each compiles a real Windows PE
  through the `mink build` CLI and executes it.
- `examples/async_tasks/main.mink` — a small end-to-end proof application.
- `E-R13` (`AsyncTaskInvalid`) added to the runtime error catalog.

## Architecture

**A task is a thread.** The task loop is the asyncio-equivalent in MINK's own
architecture, not a copy of CPython's. A task is an ordinary
`fn(arg: Int) -> Int` executed on its own OS thread (the verified R19
`CreateThread` path and trampoline), which gives genuine concurrency rather
than a synchronous stand-in. `await` blocks only the awaiting thread: the
spawner's own work and every other task keep making progress, which
`r20_tasks_run_concurrently_with_the_spawner` asserts by printing from the
spawner between spawn and await.

**Task control block.** `block[0]` is the OS thread handle, `[1]` the MINK
function pointer, `[2]` the argument, `[3]` the result, `[4]` the loop link and
`[5]` a `collected` flag. `rt_task_spawn` allocates the block and links it into
the process-global loop list; the block is owned by whoever collects it, so a
spawn that is neither awaited nor drained is a genuine leak and the exit-time
scan reports `E-R06` (`r20_uncollected_task_is_a_leak`). A second collect of
the same handle is a stable error rather than a crash
(`r20_double_collect_is_a_stable_error`), and awaiting a heap pointer that was
never produced by `rt_task_spawn` is `E-R13`
(`r20_await_of_a_non_task_allocation_is_rejected`) — the handle is read through
the runtime's validated word load.

**Loop state and locking.** The loop list head and the outstanding-task count
live in `.bss` and are guarded by the same self-contained test-and-set spin
lock the runtime uses for the allocator. Collection unlinks under the lock and
joins outside it, so a task may collect its own children
(`s73_nested_async_tasks`).

**Language surface.** `async fn f(a) -> T { body }` desugars at parse time into
a task body plus an ordinary handle producer: the declared name becomes a
`fn(a) -> Int` that calls `rt_task_spawn0`/`rt_task_spawn` with the body
closure, so an async fn is called exactly like any other function and returns
an `Int` handle. `await expr` lowers to `rt_task_await(expr)`. Because the
surface is the ordinary call/ownership/codegen path, awaits compose with
loops, branches, nesting and module boundaries, and there is no fake syntax
that silently runs synchronously.

## Defects found and fixed (all by native execution)

1. **Stale-handle fault.** `await` originally dereferenced its operand as a raw
   pointer, so a stale or bogus handle crashed the process instead of
   reporting. It now goes through the runtime's validated word load, so the
   failure is the stable `E-R05`/`E-R13` path, with a permanent regression
   (`r20_await_of_a_non_task_allocation_is_rejected`,
   `r20_double_collect_is_a_stable_error`).
2. **Dropped `pub async fn`.** `pub async fn` in a module was parsed as a
   single `pub` item whose body was silently discarded, so a module exported
   an async fn that did not exist. `parse_pub_items` now handles the `async`
   modifier after `pub`; `s73_async_across_module_boundaries` covers both the
   `pub` and non-`pub` forms.

## Verification

Every test compiles a genuine Windows PE through the real `mink build` CLI and
runs it as a separate process; a clean exit is also the leak proof, because
`rt_exit` runs the arena validation and the leak scan (`E-R06` = exit 106).

| Test | What it proves |
|---|---|
| `r20_spawn_await_result_and_handle_accounting` | spawn/await returns the task's word; `task_pending` returns to zero |
| `r20_task_run_drains_the_loop` | `task_run` collects every outstanding task and reports the count |
| `r20_zero_tasks_is_a_no_op` | the empty case: `task_run` on an idle loop |
| `r20_repeated_spawn_and_await_cycles` | 200 spawn/await cycles, allocator reuse, no leak |
| `r20_many_concurrent_tasks` | 64 live tasks, per-handle results |
| `r20_task_stop_is_honored_by_run` | `task_stop` stills the loop |
| `r20_uncollected_task_is_a_leak` | the E-R06 ownership contract on the failure path |
| `r20_double_collect_is_a_stable_error` | a second collect is a reported error, not a crash |
| `r20_await_of_a_non_task_allocation_is_rejected` | a non-task handle is a reported error (E-R13) |
| `r20_tasks_run_concurrently_with_the_spawner` | genuine concurrency ordering, not a synchronous stand-in |
| `r20_four_tasks_share_state_under_a_lock` | 4×1000 locked increments = exactly 4000 under task threads |
| `r20_tasks_allocate_strings_safely` | task ↔ string interop under the runtime lock |
| `r20_repeated_stress_runs_are_deterministic` | 5 identical stress runs |
| `r20_stdlib_tasks_module` | the shipped `stdlib/tasks.mink` public path |
| `r20_shipped_example_runs_repeatably` | the proof app, twice, same closed-form total |
| `s73_await_direct_call_and_through_a_handle` | both await forms |
| `s73_zero_parameter_async_fn` | the zero-argument surface |
| `s73_multiple_async_fns_run_together` | three distinct async fns awaited out of spawn order |
| `s73_nested_async_tasks` | a task awaiting its own child |
| `s73_await_inside_control_flow` | awaits inside loops and branches |
| `s73_async_across_module_boundaries` | cross-module async, `pub` and non-`pub` |
| `s73_async_fn_rejects_two_parameters` | the rejected two-parameter form (E-P31) |
| `s73_bare_async_without_fn_is_rejected` | `async` without `fn` is rejected (E-P30) |

## Documented limits (not defects)

- **One word in, one word out.** A task is `fn(arg: Int) -> Int`; strings and
  aggregates travel as a context pointer, exactly as with R19 threads.
- **One collector per task.** Two threads must not collect the same task; the
  second collect is a stable error rather than a coordinated wait.
- **No readiness-driven wakeup.** S63's non-blocking sockets are usable from
  inside a task, but the loop does not wake a task on I/O readiness.
- **`async` is a declaration modifier.** There are no async closures or
  lambdas, and no `async` blocks.
- **Cooperative cancellation is not implemented.** `task_stop` stills the
  loop's own bookkeeping; a running task is not interrupted.
- **All tasks share one arena.** A task that exhausts the 1 MiB heap makes
  every other thread fail with `E-R02`.

## Matrix updates

R20 VERIFIED, S73 VERIFIED; `Blocks = Y` cleared on both, so the parity-
blocking set is **5** and equals the `Blocks = Y` set. Wave E is now empty
(Waves A, B, C, E and G are closed). Every aggregate in §10 was recomputed
mechanically by row scan (232 rows; VERIFIED 95, MISSING 74, PARTIAL 34; P1 5).

## Final state

- Parity-blocking set: **5** (S62, P04, P05, P06, P09).
- `tests/async_lib.rs` 23/23. Regression re-run green for `parser` 98,
  `threads_lib` 17, `zlib_lib` 15, `zip_lib` 10, `sqlite_lib` 16.
- `tests/typecheck.rs` has **one pre-existing failure**
  (`references_flow_through_calls`: the type of `*p` through a call is
  `unknown` instead of `Int`). It reproduces on clean `ae1aeba` with the
  Session 114 work stashed, so it is not an async regression; it is a real
  inference defect and is fixed in the following session.
- `cargo fmt --check` clean.
- Linux frozen.
