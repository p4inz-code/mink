# Session 129 — M2 P1: Windows stress baseline, prior-finding reconciliation, and a closure-body compiler ICE fix

STATUS: **M2 P1 complete — PASS WITH FINDINGS.** One genuine compiler defect (A) reproduced,
root-caused, fixed, and given permanent regression coverage. Several prior findings were
reconciled (most already fixed by Session 125); several new findings are recorded for the next
phase. **Documentation + one compiler fix + tests only.**

Baseline: HEAD `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d` ("validation: close Windows practical
parity milestone 1"), branch `main`, `Cargo.toml` **1.0.4** — **no version bump, no tag, no
npm publish, Linux FROZEN and untouched.** The working tree additionally carried the
uncommitted Session 128 roadmap-lock documentation (10 modified `.md` files + 2 new docs), which
this session preserves unchanged.

Windows x86_64 is the only active platform. No Linux development, optimization, portability or
validation was performed.

## 1. Starting revision and pre-existing untracked files

- `git rev-parse HEAD` → `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d`; branch `main`;
  `origin/main` at `dd556a6` (local is 1 commit ahead).
- Version: `mink 1.0.4` (`Cargo.toml`, `npm/mink/package.json`); tags `v1.0.0`–`v1.0.4`.
- Pre-existing untracked files (NOT created, modified, deleted or absorbed by this session):
  `nl1.exe`, `nl1.mink`, `stdlib/httpserver.mink`, `npm/mink/stdlib/httpserver.mink`,
  `tests/httpserver_lib.rs`, `freebuff-chat-2026-10-01T19-08-00.600Z.md`, plus the Session 128
  docs `docs/implementation/SESSION_128_STRATEGIC_ROADMAP_LOCK.md` and `docs/roadmap/archive/`.
  (Note: `stdlib/httpserver.mink` + `tests/httpserver_lib.rs` are a pre-existing, untracked
  HTTP-server library; its `httpserver_lib` target was run and is **10/10 green** in the
  baseline. It is left exactly as found.)
- Scratch probes produced by this session live in the git-ignored `tmp/m2_p1/` directory and are
  not part of the change set.

## 2. Baseline results

`cargo check --all-targets` → clean (only the pre-existing warning baseline). Focused
M2-relevant baseline (smallest sufficient first, per the phase brief):

| Target | Result |
|---|---|
| `runtime` | 25 passed |
| `process_lib` | 86 passed |
| `network_lib` | 36 passed |
| `http_lib` | 38 passed |
| `httpserver_lib` (pre-existing untracked) | 10 passed |
| `threads_lib` | 17 passed |
| `async_lib` | 23 passed |

All green. No baseline failure, so work continued.

## 3–4. Previous findings checked: reproduced / not reproduced

The M1 report and capability matrix were treated as a **hypothesis list**, re-run against the
current repository.

| Prior finding (M1) | Reproduced on current repo? | Evidence | Status |
|---|---|---|---|
| ~4 MiB initial heap limitation | **No** | `rt_alloc(67108864)` (64 MiB) + touch → exit 0 | SUPERSEDED (Session 125 growth region; `HEAP_MAX_BYTES` = 1 GiB) |
| Large filesystem read | **No** | `fs_read` of an 8 MiB file → `8388608` | WORKS |
| Large stdin (>64 KiB) | **No** | 100 KiB piped → `rt_stdin_read` len `102400` | SUPERSEDED (8 MiB stdio capture window) |
| Child stdout capture ~4088 B | **No** | child prints 100 000 B → `process_stdout_len()` = `100002` | SUPERSEDED (24 MiB capture region) |
| Allocation-count limit (`MAX_LIVE_ALLOCS`) | **Yes** | 2000 live `rt_alloc`s → `E-R03` (exit 103) at 1024 | REPRODUCED (finding A1 below) |
| Nested closure check/build consistency | **Yes** | `mink check`/`build` **panic** (`literal_operand is only called for literals`) | REPRODUCED as a compiler ICE (finding A2 below → **fixed**) |
| Hostname TCP / DNS | **No** | `net_resolve("localhost",80)` → `127.0.0.1`; `net_hostname()` → host name | WORKS |
| TCP connect/accept loopback | **No** | listen→dial→accept→send→recv → `"ping"` | WORKS |
| Connection-failure reporting | **Partial** | `net_dial` to a closed port → `-1`, but `net_last_error()` = `0` (expected `10061`) | NEW finding (A3 below) |
| UDP loopback | **Inconclusive** | `net_bind("127.0.0.1", 52421)` → `-1`, `net_last_error()` = `10013` (`WSAEACCES`) | E — Windows *excluded port range*, not a MINK defect (M1 recorded UDP VERIFIED); not re-verified here |
| HTTPS / TLS | **Not re-verified** | `tls_lib` fixture needs `python3`, which is the Microsoft Store stub on this machine | E — environment (see §11) |
| Plain HTTP | **No** | `http_lib` 38/38 green in baseline | WORKS |

## 5–6. Selected issue and failure classification

**Selected: A2 — the compiler crashes with an internal error (ICE) on a closure whose body is a
statement block with no trailing expression** (`|x| { ...; return v; }`).

- Failure classification: **A — MINK defect.** The compiler panics (`unreachable!` internal
  error) instead of compiling a valid program. It is reachable through the ordinary public CLI
  (`mink check` *and* `mink build` both panic, exit 101) and through both the outer and nested
  closure shapes.
- Why this one: it blocks an extremely common closure idiom, is trivially reproducible, has a
  clear root cause, and admits a small, safe, well-tested fix. It is the highest-value genuine
  defect found in P1.

## 7. Root cause

`desugar_closure_to_fn` (`src/monomorphize/mod.rs`) lowers a closure to a named function whose
body is always `{ return <closure body>; }`. When the closure body is itself a **block with no
trailing expression** — e.g. `|y: Int| { return y + 1; }` — the generated `return <block>` puts
a value-less block in operand position.

1. `StmtEval::eval_operand`'s `HirExprKind::Block` arm handled `result == None` by calling
   `self.literal_operand(expr)`. That helper is documented to receive only literals
   (`Int`/`Float`/`Str`/`Char`/`Bool`/`Null`); a block fell through to
   `unreachable!("literal_operand is only called for literals")` → **panic**.
2. Secondary: the block's own statements (the inner `return`) are lowered while the *enclosing*
   `return`'s value is being evaluated, which terminates the current block. The enclosing
   `return` then had no live block and produced `E-M06` ("block is missing a terminator") — the
   shape a closure body actually lowers to.

Nesting was incidental: the same ICE reproduces with a single (non-nested) closure
(`|y: Int| { return y + 1; }`), which is why the M1 "nested closure" label was only a special
case of the real trigger.

## 8. Fix

`src/mir/lower.rs`, two minimal hunks (no architecture change, no new API):

1. `StmtEval::eval_operand`, `HirExprKind::Block` with `result == None`: emit a zero constant
   operand (`MirConstantKind::Bool(false)` typed with the block's own type) instead of calling
   `literal_operand`. This matches the existing, proven handling of a value-less block in
   `lower_binding` and the defensive constants already used by the sibling `IfExpr`/`Closure`/
   `MatchExpr` arms. The block's value is `unit`/unreachable (the type checker enforces that a
   unit block cannot be used where a value is required), so the placeholder is never observable.
2. `FnBuilder::lower_stmt`, `HirStmtKind::Return`: after lowering the return value, if the value
   terminated the current block (`self.current.is_none()`), emit nothing — the return is
   unreachable. This extends the arm's existing "a `return` after a terminator is unreachable
   dead code; skip it" rule to a value that itself terminates the block.

The `Template` for the `IfExpr` arm was deliberately **not** changed: an if-expression in
operand position has a real value and needs CFG lowering, not a placeholder (finding A4 below).

## 9. Regression test

Six tests added to the existing `tests/closures.rs` target (values checked via `main`'s exit code,
so the assertions test runtime results, not implementation details):

- `closure_native_statement_block_body_ending_in_return` (would panic before the fix)
- `closure_statement_block_body_checks_and_builds` (`check` and `build` must agree)
- `closure_native_statement_block_body_captures_and_returns`
- `closure_native_statement_block_body_with_nested_closure`
- `closure_native_statement_block_body_with_trailing_expression` (boundary: `Some` path preserved)
- `closure_native_unit_statement_block_body` (boundary: unit closure, exit 0)

Failure-without-the-fix was verified directly: reverting `src/mir/lower.rs` to HEAD and running
the new tests produced **5 failed / 1 passed** with the original panic
(`literal_operand is only called for literals`); re-applying the fix gives **6/6 pass**. The
one that passes either way is the intentional boundary guard for trailing-expression bodies. No
existing test was deleted, weakened or skipped.

## 10. Focused and broader verification

Focused (directly related targets, all green):

`closures` 43 · `mir` 34 · `backend` 46 · `semantics` 75 · `typecheck` 163 · `ownership` 49 ·
`loop_expressions` 39 · `match_expressions` 41 · `aggregate_returns` 52 · `try_operator` 12 ·
`function_annotations` 60 · `generics` 42 · `smoke` 13 — plus the §2 baseline targets.

Broader (whole suite, run in sequential batches because a single `cargo test` exceeds the time
budget — the M1 report ran in 10 batches):

- Batch 1 (29 targets): `hir optimization adversarial parser parser_hardening source lexer
  references aggregate strings struct_destructure sum_types enums tuples pattern_matching
  option_result let_annotations scalar_types discriminants richer_patterns tuple_destructure
  repl modules_check packages release cli test_runner stdlib_ownership bool_packing` →
  **1268 passed, 0 failed**.
- Batch 2 (23 of 24 targets green): `session99 session100 collections collections_lib
  filesystem_lib json sqlite_lib windows_hardening s69_hardening strings_lib csv_lib
  encoding_lib hashing_lib crypto_lib math_lib random_lib re_lib time_lib logging_lib
  assert_lib zlib_lib zip_lib package_manager` → **737 passed, 0 failed**.
- Library unit tests (`--lib`): **121 passed, 0 failed**.
- **`tls_lib`: 1 passed, 18 failed, 1 ignored — NOT a regression.** The failures are all
  `bad port line from TLS test server: ""` raised in the Rust harness `start_server()` *before
  any MINK program is compiled*: the target spawns `python3`, and on this machine `python3`
  resolves to the Microsoft Store stub (`/c/Users/Admin/AppData/Local/Microsoft/WindowsApps/python3`),
  which prints a "Python was not found…" message instead of a port. `python` (3.11) exists and
  runs the fixture server correctly. This is a pre-existing **environment/test-fixture (E)**
  condition, independent of the compiler change (verified: the failing code path never invokes
  `mink`).

Total green this session ≈ **2900+ tests across ~70 targets, 0 failures** attributable to the
change. One transient build issue was observed and is pre-existing/environmental, not a test
failure: `link.exe` intermittently returned `LNK1104` (a file lock) when many test binaries were
linked in one `cargo test` invocation; per-target runs link cleanly (this matches the note in
the M1 report).

## 11. Real-world verification

`tmp/m2_p1/byteclass.mink` — a small complete Windows utility that fingerprints a **real file**
by applying a per-byte mix callback whose body is a statement block ending in `return` (the
exact shape the defect blocked):

```mink
mod filesystem;
fn main() -> Int {
    let mix = |b: Int| { let mut h = b; h = h * 31; h = h ^ (h >> 7); return h % 256; };
    let data = fs_read("tmp/m2_p1/app.log");
    ...
}
```

Built with the public CLI to a standalone `x86_64-windows-pe` executable. MINK output `3159`,
exit `2`; an independent CPython 3.11 computation of the same arithmetic over the same file
returns `3159`, `2` — **exact match**. Before the fix this program shape did not compile at all.

## 12. Remaining M2 findings (recorded; not fixed in P1)

- **A1 — `MAX_LIVE_ALLOCS = 1024` caps simultaneously live allocations.** Reproduced:
  `E-R03` (exit 103) at 1024 live allocations; a `Vec<Str>`-style workload fails after
  ~800–1000 elements. Classification: **C (architectural limitation)** — a fixed-size liveness
  table whose bound is part of the ABI; raising it is cheap in `.bss` but changes a constant
  shared with the frozen Linux emitter, so it is a deliberate decision, not a mechanical patch.
- **A4 — if-expression in operand position crashes the compiler.** Reproduced:
  `rt_print_int(if true { 1 } else { 2 });` and a closure body whose trailing expression is an
  if/else both panic in the same `literal_operand` site (the `IfExpr` arm). Classification:
  **A (defect)**, but the correct fix is real CFG lowering (a placeholder operand would return
  the wrong value), so it is out of P1 scope.
- **A5 — `if` statement without `else` as a non-final statement in a closure block is a parse
  error.** Reproduced: `|x: Int| { if x > 0 { return 1; } return 0; }` → `E-P26` ("an
  if-expression requires an `else` branch"), while the identical body in a normal function
  compiles. Classification: **A (parser defect / inconsistency)** pending root cause; this is
  an un-narrowed observation.
- **A3 — a failed `net_dial` does not report the Winsock reason.** Reproduced:
  `net_dial("127.0.0.1", 1)` → `-1` with `net_last_error()` = `0` (expected `10061`
  `WSAECONNREFUSED`). `net_dial` documents "`net_last_error()` holds the reason" but calls
  `net_close(sock)` before returning, and closing the socket overwrites the last error.
  Classification: **A (defect)**, small and safe to fix, but not selected for P1.
- **Not re-verified in this environment (E):** UDP loopback and HTTPS/TLS — see §3–4.

## 13. Repository state

Change set for this session (audited — see §14):

- `src/mir/lower.rs` — modified (2 hunks, +25/−1).
- `tests/closures.rs` — modified (+70, six new tests).

Pre-existing Session 128 documentation changes remain in the working tree untouched. No other
file was modified. `Cargo.toml`/`npm/mink/package.json` unchanged at **1.0.4**; no tag added; no
npm/release/publish command run; no file under `src/backend/emit/linux_runtime.rs` or the ELF
emitter touched; no pre-existing untracked file deleted or absorbed. Nothing was committed.

## 14. Next recommended M2 phase

**M2 P2 — continue Windows practical stress with the networking and concurrency areas, and
resolve the two compiler-defect findings A4 (if-expression in operand position) and A5
(if-without-else inside a closure block), which are closely related to the closure-body defect
fixed here.** Concretely: (1) root-cause and fix A4 with proper `if`-expression CFG lowering in
operand position and a permanent regression; (2) reproduce and root-cause A5; (3) re-run the
networking stress area on a machine with a real `python3` (or update the `tls_lib` harness to
fall back to `python`) so HTTPS/TLS and UDP can be re-verified rather than recorded as
environment-blocked. Do not begin Linux work.
