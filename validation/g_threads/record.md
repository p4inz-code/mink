# G — poolrun (worker pools and pipelines)

PROJECT: G concurrency
PURPOSE: Run a real concurrent workload — split work across OS threads, share
one lock-guarded accumulator, and pipeline two stages through a shared buffer —
the task a Python developer reaches for with `threading.Thread` + `Lock`.
MINK SOURCE: `validation/g_threads/main.mink` (51 functions emitted)
PYTHON COMPARISON: `validation/g_threads/python_ref.py` (`threading`, `Lock`)
CROSS-CHECK HARNESS: `validation/g_threads/verify.py`

BUILD: PASS — `mink build validation/g_threads/main.mink` → `main.exe`
(x86_64-windows-pe, no external toolchain, no runtime DLL).

EXECUTION: PASS — 18/18 cases produce byte-identical stdout, stderr and exit
codes compared with the Python reference (`python verify.py` → `18/18 cases
match the Python reference`) across 9 workloads (1–16 workers, up to 10,000
items, divisible and non-divisible splits) and the usage-error paths.

FUNCTIONAL CORRECTNESS: PASS — `squares` spawns one worker per disjoint range
of 1..n, each returns its subtotal and adds it to a shared counter under a
single `lock`; main joins in order, prints every subtotal, the locked total
and the joined total, and verifies both against the closed form
n(n+1)(2n+1)/6. `pipeline` runs stage 1 (workers publish subtotals into their
own slots of a shared buffer), a join barrier, then stage 2 (one reducer
thread sums the slots). Results are deterministic because the ranges are
disjoint; only scheduling varies, and join order fixes the print order.

ERROR HANDLING: PASS — exit 1 for usage errors (missing/extra arguments,
non-numeric or non-positive `n`/`workers`, unknown mode, any option); exit 3
if a computed total ever mismatched the closed form (not observed). Messages
go to stderr (`rt_stderr_write`), stdout stays clean.

MEMORY: PASS — the runtime leak check (`E-R06`, exit 106) is enabled; all 18
cases exit 0/1, never 106. No leaks were found during validation; every
`rt_alloc`/`lock_new` is released and every `thread_join` frees its control
block.

REALISTIC INPUT: PASS — 16 concurrent workers, 10,000 items, a 999/7
non-divisible split, and a single-worker edge case in both modes.

REPEATED EXECUTION: PASS — concurrent results are identical across runs
(the workload is a deterministic reduction); `verify.py` has been run
repeatedly with identical results (18/18 each time).

STANDALONE EXE: PASS — `main.exe` runs with no interpreter and no runtime;
the only dependency is the Windows API.

WINDOWS: PASS.

MINK LIMITATION: see CLASSIFICATION; none of the findings blocked the
application.

CLASSIFICATION:
- C (architectural limitation) — the thread API is deliberately GIL-free and
  minimal: `thread_spawn(f, arg)` / `thread_join(handle)` and a mutex
  (`lock_new`/`acquire`/`release`/`free`). There are no condvars, semaphores,
  read-write locks, thread pools, futures or cancellation; coordination beyond
  a mutex needs the `tasks` module or hand-rolled flags, and there is no
  data-race detector — correctness here rests on disjoint ranges plus the
  single lock.
- C (architectural limitation) — a spawned thread's entry point is a function
  of one `Ptr<Int>` returning an `Int`; arguments must be packed into raw
  memory blocks by the caller, and the pointer/int conversion
  (`rt_ptr_to_int`/`rt_int_to_ptr`) is explicit.
- C (ergonomic trap, documented) — nested `rt_str_concat` leaks the inner
  temporary (the intrinsic is read-borrowing); the label builder binds each
  step (`indexed`).
- C (ergonomic trap, documented) — `rt_alloc`/`lock_new` blocks and
  `thread_join` handles are outside the `Str` ownership system, so they are
  freed by hand; the leak checker still validates the heap, which is why the
  blocks are freed before exit.
- D (deliberate difference) — `rt_print_str` appends CRLF; the Python
  reference writes raw bytes + `\r\n` to match.
- D (deliberate difference) — `rt_argc`/`rt_argv` exclude the program name;
  a leading `-` marks a usage error because the tool defines no options.
- D (semantics verified) — integer division in the range split (`n / workers`)
  matches Python's `//`, and the last worker takes the remainder; both
  implementations print in join order, not completion order.
- D (semantics verified) — Python's GIL makes its `+=` effectively atomic,
  but the reference still takes the explicit lock; the point of the
  cross-check is the deterministic total, not a claim about Python's
  internals.
- A (no application defect) — unlike every earlier milestone, G needed no
  leak or logic fix: the first version already exited 0 with matching totals
  on all 18 cases. No MINK compiler/runtime defect was found; the thread
  runtime, the mutex and the leak checker behaved as documented.

EVIDENCE:
- TEST — `validation/g_threads/verify.py`, 18 cases, 0 differences (stdout,
  stderr, exit code).
- OUTPUT — see the transcript below.
- REGRESSION — the concurrency runtime is covered by `cargo test --test
  threads_lib` (17 passed) and the full suite (73 integration targets +
  `--lib`, 3040 tests) was re-run green before starting G.

## Transcript

```
$ mink build validation/g_threads/main.mink
mink: build: 'validation/g_threads/main.mink' -> '.../main.exe'
(target: x86_64-windows-pe, 51 function(s), 0 binding(s))

$ python validation/g_threads/verify.py
ok   squares_100_4 (exit 0)
ok   squares_1_1 (exit 0)
ok   squares_97_5 (exit 0)
ok   squares_10000_8 (exit 0)
ok   squares_1024_16 (exit 0)
ok   pipeline_1_1 (exit 0)
ok   pipeline_100_4 (exit 0)
ok   pipeline_1000_3 (exit 0)
ok   pipeline_999_7 (exit 0)
ok   no_args (exit 1)
ok   missing_workers (exit 1)
ok   extra_args (exit 1)
ok   bad_n (exit 1)
ok   bad_workers (exit 1)
ok   zero_n (exit 1)
ok   zero_workers (exit 1)
ok   unknown_mode (exit 1)
ok   option_looking_arg (exit 1)

18/18 cases match the Python reference

$ main.exe squares 100 4
workers=4
n=100
subtotal[0]=5525
subtotal[1]=37400
subtotal[2]=100525
subtotal[3]=194900
locked-total=338350
joined-total=338350
expected=338350
exit=0

$ main.exe pipeline 1000 3
workers=3
n=1000
stage1[0]=12364179
stage1[1]=86327142
stage1[2]=235142179
total=333833500
expected=333833500
exit=0
```

## Python comparison notes (concrete differences, no ranking)

1. Python's `threading.Thread(target=..., args=...)` maps to
   `thread_spawn(func, arg_block)`; MINK has no closures, so the target is a
   top-level function of one raw pointer and the arguments are packed into an
   `rt_alloc` block.
2. Python's `Lock` maps to `lock_new`/`lock_acquire`/`lock_release`/
   `lock_free`; both implementations hold it only around the accumulator
   update, never around the computation.
3. Python's `Thread.join()` returns `None`; MINK's `thread_join` returns the
   worker's `Int` return value, which is how the subtotals reach `main`.
4. Python's list slicing maps to disjoint index ranges computed with the same
   integer division; the shared buffer is a raw `rt_alloc` block addressed by
   byte offset (`base + index * 8`), not a `Vec`.
5. Python prints from the main thread after joining in index order; MINK does
   the same, so no output interleaving is possible in either implementation.
