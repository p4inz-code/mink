# SESSION 97 — Windows Ship Validation + Final Completion Gate

## Summary

- Starting commit: `e08458a` (clean tree)
- Final commit: *(see git log)*
- Rust: 1.97.1 · MINK version: **1.0.1** (consistent across `mink --version`, `-v`, `-V`, `version`, and `npm` package.json)
- Test count: **2547 passed / 0 failed** (Session 96 baseline 2538 → +9 new process tests; no unexplained change; no panics)

## 1. process_run root cause (P1, fixed)

**Root cause:** the Windows `emit_process_run` runtime waited with
`WaitForSingleObject(hProcess, INFINITE)` *before* reading the child's stdout/stderr
pipes. A child that writes more than the ~4 KiB anonymous-pipe buffer blocks forever
in `WriteFile`; the parent is stuck in the infinite wait, so neither side ever
progresses — a classic pipe-buffer deadlock.

**Independent reproduction (current checkout):** a MINK child printing ~5000 bytes
hung the parent (`process_run` never returned; the child PID stayed alive,
blocked on the full pipe). Small outputs (≤ 4096 bytes) completed normally.
The same behavior was confirmed against the pre-Session-96 release binary,
proving the defect predates this session.

## 2. process_run fix architecture

The wait-then-read tail of `emit_process_run` was replaced with a
**poll-and-drain loop**:

1. Loop while the child has not exited:
   - `PeekNamedPipe` on the stdout and stderr read handles to query available bytes
     (the pipe is byte-mode; the number of bytes available is authoritative).
   - If bytes are available, read them with `ReadFile` into the runtime's fixed
     output buffers, accumulating stdout/stderr separately up to the documented
     cap; anything beyond the cap is drained and discarded into stack scratch so
     the child can never block on a full pipe.
   - `WaitForSingleObject(hProcess, 20)` — a 20 ms bounded poll, *not* an
     infinite wait and *not* a busy spin.
2. After the child exits, one final drain pass picks up any remaining bytes,
   then `GetExitCodeProcess` returns the real exit code.

Imports: added `PeekNamedPipe` to the PE IAT (one additional import; the idata
table is dynamic and re-verified). No overlapped I/O was needed because
peek-then-read on byte-mode anonymous pipes is race-free (a successful peek
guarantees the subsequent read returns at least the peeked bytes).

Verified behavior:
- small output (12 bytes): unchanged, exit code preserved
- 5002-byte stdout burst: completes, exit code 0, output capped at 4088 bytes
- 1 MB stdout: drains in well under a second, no hang
- large stderr and large stdout+stderr combined: same
- non-zero exit code (3) with large output: preserved
- empty output, repeated executions, output after immediate child exit: correct
- no stale output between runs

The output cap (4088 bytes per stream) is a documented V1 runtime limitation
(fixed-size output buffers), not an artifact of the fix; overflow is drained and
discarded rather than deadlocking.

## 3. Regression suite (9 new tests in tests/process_lib.rs)

| Test | Coverage |
|---|---|
| `process_large_output_above_pipe_buffer_completes` | 5000-char child stdout (the former deadlock case) |
| `process_output_at_pipe_buffer_boundary` | 4096-byte boundary |
| `process_output_slightly_above_boundary` | 100-byte output with exact content |
| `process_64kb_stdout_completes` | 64 KB stdout |
| `process_1mb_stdout_completes` | 1 MB stdout (cap semantics asserted) |
| `process_large_stderr_capture` | large stderr via `cmd /c ... 1>&2` |
| `process_large_combined_streams` | large stdout + stderr together |
| `process_large_output_nonzero_exit` | large output + exit code 3 |
| `process_repeated_large_runs` | 3 sequential large-output runs, no stale output |

Each asserts deterministic completion, exit code, and exact/expected content.
Full process suite: **80/80 pass**.

## 4. Ownership audit (process)

Every HANDLE in `emit_process_run` (process, thread, stdout read/write, stderr
read/write) has a single close point on the success path; creation failures
(pipe creation, `CreateProcessA` failure) close the partial set and return a
documented failure without hanging. The MINK output strings are heap-owned and
freed by the caller via the stdlib wrapper (established contract). The leak
checker remains enabled in all tests; no process test reports leaks.

## 5. Shipped artifact validation

**Clean npm install #1 (dir A)** — fresh temp dir, `npm install` of the tarball
packed from this checkout:
- `added 1 package`; shims created (`mink`, `mink.cmd`, `mink.ps1`)
- `--version` / `-v` / `-V` / `mink.cmd --version` → all `mink 1.0.1`
- `check`/`build`/`run` of the example → `main.exe` (20,480 bytes), correct
  report.json, exit 0
- Installation directory removed; the built `main.exe` then ran standalone
  from an empty directory (exit 0, correct output) with no MINK/Rust/Node/npm
  present.

**Clean npm install #2 (dir B)** — independent second environment reproduced
the full workflow: install → version → build → run → standalone .exe → exit 0.

**Artifact truth:** the packed tarball's embedded `bin/mink.exe` SHA-256
(`813520f027a35c2bd8e5a8461f950ceadb880a085d12437234c6f87c8e99f43f`) exactly
matches the release-built `target/release/mink.exe` and `npm/mink/bin/mink.exe`.
Package version 1.0.1 matches the CLI. The package ships only the self-contained
compiler executable (no stdlib files needed — the compiler embeds its runtime
and the shipped example uses only runtime intrinsics).

## 6. Example application (committed)

`examples/system_report/` — a self-contained Windows utility that gathers the
process ID, boot-relative time, current working directory length, and its own
executable's existence/size, builds a JSON document with string intrinsics, and
writes `report.json`. Uses only runtime intrinsics (no stdlib modules), so it
builds with the bare `mink` CLI exactly as shipped. Verified end-to-end:

- `mink check` / `mink build` / `mink run` → exit 0
- standalone `main.exe` runs from a path containing spaces and from an empty
  temp dir with no MINK installed (exit 0, repeatable, fresh PID per run)
- README documents build/run and the standalone-exe workflow

## 7. CLI quality recheck

- `--version`, `-v`, `-V` all report `mink 1.0.1`
- invalid command, missing file, directory-as-source, missing argument,
  syntax error → clear message, exit code 1, no panic, no hang
- relative paths, absolute paths, and paths containing spaces: all work
- valid program → exit 0

## 8. Windows capability matrix (execution-verified)

Compiler/codegen/executable execution · strings (exact-length, ownership) ·
allocator (reuse, leak checker) · filesystem (read/write/mkdir/remove/rename/
cwd) · process (run/capture/exit codes, large-output now fixed) · TCP · UDP ·
HTTP GET · HTTP POST (Session 96, byte-exact echo) · crypto (bcrypt-backed
random, SHA-256/HMAC/HKDF vectors) · time · CLI · npm distribution. All green
in the 2547-test suite plus this session's real-execution evidence.

**Environment: STUB (documented P2)** — Windows `rt_env_*` remain Session-73
V1 stubs returning empty/-1 (deterministic, leak-clean owned empty Str). The
shipped CLI/README make no environment claim. `env_get`/`env_has` are
execution-verified only on the Linux target (frozen).

## 9. P0/P1 audit

- P0 remaining: **0**
- P1 remaining: **0** (the process_run deadlock — the only open P1 — is fixed
  and covered by the regression suite)
- P2 remaining (documented): Windows environment stubs; process output cap
  4088 bytes/stream; no user-facing stderr-write intrinsic (P3-class, capture
  itself verified byte-exact); no argv support; HTTP/1.1 without TLS/keep-alive/
  chunked (per API); Linux frozen by policy
- P3: cosmetic/docs items as previously recorded

## 10. Quality gates

- `cargo fmt --check`: clean
- `cargo clippy --all-targets`: no warnings in changed files (63 pre-existing
  warnings, all in frozen Linux code or pre-existing style items)
- `cargo test`: **2547 passed / 0 failed** (57 binaries, exit 0)
- `cargo build`: clean · `cargo build --release`: clean

## 11. Files changed

- `src/backend/emit/pe.rs` — add `PeekNamedPipe` IAT import
- `src/backend/emit/runtime.rs` — poll-and-drain rewrite of `emit_process_run`
- `tests/process_lib.rs` — 9 large-output regression tests
- `examples/system_report/main.mink` + `README.md` — new shipment example
- `docs/implementation/SESSION_97_WINDOWS_SHIPMENT_AND_COMPLETION.md` — this file

Linux implementation: **untouched** (frozen). The deadlock fix is in the
Windows-only emitter; Linux `process_run` drains concurrently by design and was
not modified.

## 12. Final status

- **WINDOWS RUNTIME = STABLE**
- **WINDOWS SHIPMENT = EXECUTION VERIFIED** (clean npm install ×2, remove
  install, standalone .exe works)
- **WINDOWS P0 = 0, P1 = 0**
- **WINDOWS COMPLETION GATE = PASSED** (per the Session 97 criteria:
  process fix verified, tests green, CLI clean, npm clean ×2, shipped artifact
  hash-verified, example committed, standalone .exe proven outside the repo and
  after installation removal)
