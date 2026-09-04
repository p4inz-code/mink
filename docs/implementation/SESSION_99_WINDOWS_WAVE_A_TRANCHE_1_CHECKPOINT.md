# Session 99 — Windows Wave A Tranche 1 — Checkpoint (safe pause)

**Status: paused mid-implementation. Code reverted to the Session 98 baseline
(`da2e3cc`); this document is the only Session 99 artifact committed.**

- Starting commit: `da2e3cc` (clean)
- Tree state now: clean at `da2e3cc` (all Session 99 source edits reverted)
- MINK version: 1.0.1 · Windows x86_64 only · Linux frozen

## Why the pause

A real correctness bug in the new Windows `rt_env_set` was found while
proving the environment intrinsics, and the session budget ran out before a
root cause was identified. Per the project rule that nothing unverified is
committed, the entire Session 99 working set was reverted so the tree is
exactly the known-good Session 98 state. All findings and the implementation
design below are preserved for Session 100.

## What was implemented this sitting (all in `src/backend/emit/runtime.rs` +
wiring; all reverted)

Rust-level wiring (verified to compile before revert):

- `src/runtime/intrinsics.rs` — 7 new intrinsics appended to `ALL`:
  `rt_sleep(Int)->Unit`, `rt_stderr_write(Str)->Int`, `rt_stdin_read()->Str`,
  `rt_argc()->Int`, `rt_argv(Int)->Str`, `rt_str_from_float(Float)->Str`,
  `rt_str_format(fmt,a0,a1,a2)->Str`.
- `src/backend/ir.rs` — `RuntimeService` variants + `arity()` + `is_callable()`.
- `src/backend/lower.rs` — `rt_*` name → service mappings.
- `src/backend/emit/pe.rs` — kernel32 imports appended: `Sleep` (35),
  `GetCommandLineA` (36).
- `src/runtime/abi.rs` — BSS additions: `argv_ready`, `argv_count`,
  `argv_table` (64×16 entry table), `arg_data` (4096 B copy area),
  `stdin_buf` (65536 B), `write_redirect` (ptr), `float_sink` (80 B).
- `src/backend/emit/linux_runtime.rs` — 7 deterministic stub services
  (`emit_linux_stub_unit/minus_one/zero/empty_str`) registered so the shared
  service table stays total. **Linux behavior unchanged** (frozen; no Linux
  program calls these). This was the only Linux-file edit, a compile-totality
  necessity, and it is reverted with the rest.

Windows emitter implementations (all reverted):
- Environment made real: `emit_env_get` / `emit_env_set` / `emit_env_has` /
  `emit_env_remove` via `GetEnvironmentVariableA` / `SetEnvironmentVariableA`
  (two-call length-query pattern, owned exact-length `Str` results, missing →
  owned empty `Str`; set-to-empty removes the variable per Windows semantics).
- `emit_sleep` (kernel32 `Sleep`).
- `emit_stderr_write` (validated bytes through the `WriteStderr` thunk).
- `emit_stdin_read` (chunked `ReadFile` loop into the fixed stdin buffer up to
  65528 B, EOF/error stop, owned exact `Str`).
- `emit_argv_parse` (lazy quote-aware parser: skips the executable name,
  records ≤64 args / ≤4080 content bytes into `arg_data` with an index table;
  quotes stripped, `""` = empty arg) + `emit_argc` / `emit_argv`.
- `emit_str_from_float` — reuse trick: a `write_redirect` BSS flag makes the
  `WriteStdout` thunk (`emit_write`) append into `float_sink` instead of the
  console, so the exact ~1000-line `rt_print_float` dtoa path is reused
  verbatim; the formatter's trailing CRLF is stripped. Zero dtoa duplication.
- `emit_str_format` — two-pass scan of `fmt` (`{}` → next of a0..a2, `{{`/`}}`
  → literal brace, lone braces literal, placeholder with no argument removed),
  exact output length computed first, one allocation, arguments borrowed.
  Fixed arity 4 is an intentional V1 contract difference from Python.

## What was execution-verified before the bug halted progress

Using `target/debug/mink.exe build` + running the generated PE directly:
- `rt_sleep(1)` — exit 0.
- `rt_env_get("PATH")` — returned length 1856, correct.
- `rt_env_get` of a missing variable — owned empty `Str`, `rt_env_has` false.
- `rt_env_set` + `rt_env_has` + `rt_env_get` round trip — worked.
- `rt_env_remove` — worked (var gone afterward).
- `rt_stderr_write` — worked (byte count returned).
- All other new services (argv/stdin/float/format) were **not yet tested**.

## The bug (unfixed, needs Session 100 root cause)

**Signature:** `rt_env_set(name, value)` fails with
`ERROR_INVALID_PARAMETER` (GetLastError 87) only when the process heap state
is a specific free-list shape:

- Repro (pure MINK, no env APIs involved before the failing call):
  ```
  let a = rt_str_alloc(5);
  let b = rt_str_alloc(1900);
  rt_str_free(a);
  rt_str_free(b);
  rt_env_set("MINK_S99_TEST", "hello worldX");   // returns -1 (error 87)
  ```
- Freeing the two blocks in the opposite order (big then small) does **not**
  fail; setting with a short literal value ("A") after the same frees does
  **not** fail; setting with a heap-built value (`rt_str_concat`) does **not**
  fail; a single freed block reused by the name CStr does **not** fail.
- Observed mapping of the failure: value CStr length ≥ ~10 (needs a 16-byte
  alloc) **and** the value CStr lands on the *second* freed block via
  free-list reuse. When the value CStr is a fresh bump allocation it always
  works.
- `rt_env_get` + `rt_str_free` + long-literal `rt_env_set` reproduces the same
  failure (env_get's name CStr is freed first, then the big value block, so
  the free list is small→big at the time of the failing set).

**What is known correct:** the CStr contents and pointers are believed fine
(the same code succeeds from a fresh-bump layout); GetLastError is genuinely
87 from `SetEnvironmentVariableA`. The interplay is between the arena
free-list reuse state and the Win32 call, not the environment text.

**Leads for Session 100 (in order):**
1. Inspect `emit_alloc`'s reuse path vs `emit_free`'s bookkeeping in
   `src/backend/emit/runtime.rs` (free stores next-link at `[block+0]` and
   saved size at `[block+8]`; reuse unlinks head only — verify no stale
   `[block+8]` size vs table re-record confusion when a 16-byte block is
   reused for a 16-byte request right after a large block).
2. Dump both CStr pointers and bytes with `rt_mem_load` right before the call
   in a probe to rule content corruption in/out.
3. Try swapping the allocator order by allocating the value CStr first, to
   confirm the failing layout is exactly "value on second free-list block".
4. If the allocator is exonerated, test `SetEnvironmentVariableA` against the
   same two addresses from a C harness to check for an address-window quirk.

## Recommended Session 100 scope (unchanged from Session 99)

1. Root-cause and fix the `rt_env_set` free-list-reuse failure.
2. Re-apply the reverted Session 99 implementation (git is clean at
   `da2e3cc`; the edits are described above and reconstructable in ~2–3 hours),
   then execution-verify each of: env set/get/has/remove, argv (argc + argv
   incl. spaces/quotes/zero args), stdin read (piped multi-line), Sleep,
   stderr write, float→Str (0, negatives, fraction, Inf/NaN forms vs
   `rt_print_float`), and `str_format` (substitution, repeats, `{{`/`}}`,
   missing args).
3. Add durable Rust regression tests (native execution through the CLI) in a
   new `tests/session99.rs` or extend `tests/windows_hardening.rs`.
4. Stdlib-in-npm bundling + module include/search path (compiler-side module
   loader work; npm currently ships only the compiler exe — see
   `docs/implementation/SESSION_97_WINDOWS_SHIPMENT_AND_COMPLETION.md`).
5. Proof application `examples/env_report` exercising the new capabilities
   through the public CLI, standalone `.exe` validation outside the repo.
6. Parity-matrix updates, full quality gates (fmt/clippy/test/build/release),
   SESSION_99 documentation, one focused commit.

## Files that will change when Session 99 is re-applied

`src/runtime/intrinsics.rs` · `src/backend/ir.rs` · `src/backend/lower.rs` ·
`src/backend/emit/pe.rs` · `src/backend/emit/runtime.rs` ·
`src/runtime/abi.rs` · `src/backend/emit/linux_runtime.rs` (stub-only,
totality) · tests · docs · npm packaging · examples.

## Known-good state being preserved

- `da2e3cc` — Session 98 parity audit (docs only), 2547 tests green,
  Windows base COMPLETE / STABLE, Linux frozen, env services stubbed on
  Windows (empty/-1) exactly as before this session.
