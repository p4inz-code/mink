# Session 205 — TLS Test Harness Portability and Verification

Windows x86_64, 2026-11-07. Starting commit
`fa2a796b25c9b40d5707eb8cc869db655798939d` (Session 204, local == origin/main
verified before any change).

## 1. Objective

Establish why `tls_lib` could not run in this environment, fix any real harness
weaknesses, and execute the complete TLS suite without silently skipping tests
or accepting false positives.

## 2. Files inspected

- `tests/tls_lib.rs` (22 tests, server lifecycle, build/run harness).
- `tests/tls/server.py` (file and binary-echo TLS endpoints).
- `tests/tls/make_certs.py` and the committed PEM fixtures.
- Toolchain and PATH: `python3`, `python`, `py`.

## 3. Reproduction and root cause

```
$ python3 --version
Python was not found; run without arguments to install from the Microsoft
Store ...                                  (exit 49, Microsoft Store alias)

$ python --version
Python 3.11.9

$ timeout 5 python tests/tls/server.py --cert good.pem --key good.key.pem \
    --root target/mink-artifacts/tls_root_x --expect -1
44989                                      (exit 124 = still serving)
```

`python3` on this machine is
`C:\Users\Admin\AppData\Local\Microsoft\WindowsApps\python3.exe`, the App
Execution Alias redirector. It successfully spawns (so `Command::spawn` does
not fail) but exits 49 with "Python was not found" on stderr. The harness
launched it with `stderr(Stdio::null())`, so the actual cause was invisible and
the test failed only as "TLS test server did not report a port within 30s".
A real CPython 3.11.9 is installed and reachable as `python`; the server works.

This was an environment problem *and* a harness portability problem: the
harness hard-coded one interpreter name, discarded startup diagnostics, and did
not reap the child on a readiness failure (Rust does not kill a `Child` on
drop, so a failed startup leaked a process).

## 4. Fixes

`tests/tls_lib.rs`:

- **Interpreter discovery.** Candidates `python3`, `python`, `py -3` are probed
  with `--version`; a candidate is accepted only when it exits successfully
  **and** reports `Python 3`. The Store alias therefore cannot be accepted, and
  the real interpreter is found. The resolution is cached per process and its
  failure message lists every probe and its output, with instructions to
  install Python 3 or fix PATH.
- **Startup diagnostics.** Server stderr is captured to
  `target/mink-artifacts/tls_server_<n>.err` instead of `NUL`. On a readiness
  timeout or a bad port line, the child's exit status and log are included in
  the panic, and the log is kept for diagnosis. On success `Drop` deletes it.
- **Lifecycle.** The new `fail_server` helper kills and reaps the child before
  panicking, so a failed server start can no longer leave a stray process.
- **Permanent prerequisite test.** `s205_tls_interpreter_is_usable` asserts the
  resolved interpreter is CPython 3 and explicitly rejects the Store alias
  message, so discovery cannot regress silently.

No runtime, compiler or public API code changed. `tests/tls/server.py` was not
modified.

## 5. Verification

| Check | Result |
| --- | --- |
| `cargo test --test tls_lib` (run 1) | **21 passed, 0 failed, 1 ignored** (4.31 s) |
| `cargo test --test tls_lib` (run 2) | **21 passed, 0 failed, 1 ignored** (4.28 s) |
| `cargo test --test tls_lib` (run 3, process counting) | **21 passed, 0 failed, 1 ignored** |
| Python processes before / after run 3 | 2 / 2 (the suite leaks none) |
| Leftover `tls_server_*.err` logs | none |
| Interpreter used | CPython 3.11.9 via `python` |
| Ignored test | `s62_system_store_public_https` — opt-in, live public internet |

The ignored test is the only skip; it is a deliberate network-dependent
positive control, and the same system-store code path is exercised
deterministically by `s62_system_store_does_not_trust_private_ca`.

Covered by the executed suite: pinned-CA handshake, SAN matching (second entry
and IP literal), HTTPS GET in both forms, 300 KiB transfer, binary echo with
partial reads and EOF, repeated connections with the leak check, untrusted
root, system store rejecting a private CA, wrong hostname, wildcard label
rules, expired certificate, wrong EKU, refused connection, missing and
malformed CA files, malformed URL, invalid/closed handle operations, and
fixture-race freedom. Server termination is proven by process counting and by
the absence of listeners after each test.

## 6. Environment blocker found during verification

Release-mode compilation currently fails **before** any TLS test can run:

```
rustc ... -C opt-level=3 ... (exit code: 0xc000001d, STATUS_ILLEGAL_INSTRUCTION)
rustc ... -C opt-level=3 ... (exit code: 0xc0000374, STATUS_HEAP_CORRUPTION)
rustc ... -C opt-level=3 ... (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)
```

This reproduces on a known-good release target (`--test capacity`), with
`-j 1`, `CARGO_INCREMENTAL=0`, `codegen-units=1` and `target-cpu=x86-64`, and
in three consecutive attempts. Debug builds compile and run normally, and the
last successful release artifact is dated 2026-11-07 11:17. The crash is in
`rustc.exe` itself (toolchain 1.97.1, LLVM 22.1.6), not in MINK source, and the
varying exception codes indicate memory corruption rather than a deterministic
compiler diagnostic. It is recorded as an **environment blocker**, not a
repository defect; release verification for this session is therefore
**not run**, and this is not reported as a pass.

## 7. Known limitations

- The TLS suite requires a real CPython 3 installation. Failure to find one is
  now a clear, diagnosed error instead of a 30-second timeout.
- One opt-in test needs public internet and stays ignored.
- `tls_lib` did not previously contribute to the regression totals because it
  could not run; it now adds 21 executed tests and 1 ignored test.

## 8. Documentation

- `docs/roadmap/IMPLEMENTATION_ROADMAP.md`: Session 205 status.
- `docs/audits/SYSTEMS_READINESS_AUDIT.md`: Session 205 addendum superseding
  the "tls_lib cannot run" limitation.
- This record.

## 9. Verdict

**PASS — the suite now runs. 21 passed / 0 failed / 1 ignored, repeated three
times, with no leaked processes or logs.** Root cause established (Store alias
accepted because spawn succeeds; diagnostics discarded; child not reaped) and
all three harness weaknesses fixed. Release-mode execution is blocked by an
environmental `rustc` crash and is recorded as not run.
