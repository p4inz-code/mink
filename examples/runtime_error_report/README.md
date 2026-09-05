# MINK Runtime Error Report (R06 diagnostic proof)

A **diagnostic** example: `main.mink` intentionally triggers a runtime
failure so the reported source location can be verified end-to-end. It is
the Session 100 proof application for R06 (runtime error-location
metadata). It is **not** a template for normal programs — it deliberately
crashes with exit code 110 on every run.

## What it demonstrates

- Runtime failures carry stable identity: `E-R10` (array index out of
  range), exit code `100 + 10 = 110`.
- The failure reports the exact source file and line, read from a compact
  source-location table **embedded in the executable**. No source files,
  no `.map`/`.json`/debug files, no development checkout, no environment
  variables, and no current-working-directory dependency are required at
  run time.
- The location is reported from the function that failed (`pick`, line 20)
  even though the call originates in `main` (line 24).
- The embedded location survives copying the executable away from the
  source tree and renaming it.
- The failure is deterministic: identical exit code and identical stderr
  on every run.
- Failures not tied to one user operation (e.g. an exit-time leak check)
  print no location line, so the runtime never fabricates a location.

## Build

From a clean environment with the public `mink` CLI:

```text
mink check main.mink
mink build main.mink
```

This produces exactly one standalone executable: `main.exe`.

## Run

Run `main.exe` directly — no arguments, no stdin, no source files:

```text
main.exe
```

Expected result (stderr):

```text
mink: runtime error[E-R10]: array index out of range: the index must be below the array's length
  at <path>\main.mink:20
```

Exit code is 110.

## Standalone proof

Copy `main.exe` to an **empty** temporary directory (so no `.mink`
source is reachable) and run it there:

```text
mkdir empty-dir
copy main.exe empty-dir\
cd empty-dir
main.exe
```

The reported location still names the original `main.mink:20`; the exit
code is still 110. This is covered automatically by the Session 100
regression tests (`tests/session100.rs`), which build, copy, and run the
generated executables natively on Windows.

## Location format

The location line is `  at <file>:<line>` where `<file>` is the source
path exactly as given to `mink build` and `<line>` is the 1-based line of
the failing operation. Column information is intentionally not reported
(the compiler pipeline measures columns in bytes and the runtime contract
only promises file + line).
