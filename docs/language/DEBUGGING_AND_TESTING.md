# MINK Debugging and Testing

**Applies to:** MINK 1.0.5 (current repository build) and MINK 1.0.4 (published
npm package) on Windows x86_64. The commands and diagnostics below were
verified on 2026-11-05 (M6 Sessions 179-180) against the then-current 1.0.4
repository build and a 1.0.2 install; audit-time version numbers are kept where
they are part of the recorded evidence.
**Status:** Working guide, verified 2026-11-05 (M6 Sessions 179-180); current
version line re-checked 2026-11-08 (Session 217).
**Companions:** [`LANGUAGE_BASICS.md`](LANGUAGE_BASICS.md),
[`STANDARD_LIBRARY_REFERENCE.md`](STANDARD_LIBRARY_REFERENCE.md).

This guide is about moving from "my program is wrong" to a precise, reportable
fact. Every command and diagnostic below was executed on the audit machine;
where a diagnostic is imprecise, that is stated rather than smoothed over.

## Contents

1. [Environment and compile/run](#1-environment-and-compile-run)
2. [Exit codes](#2-exit-codes)
3. [Standard-library resolution](#3-standard-library-resolution)
4. [Reading diagnostics](#4-reading-diagnostics)
5. [Runtime failures, leaks and invalid frees](#5-runtime-failures-leaks-and-invalid-frees)
6. [Testing methodology](#6-testing-methodology)
7. [Comparing installed and repository compilers](#7-comparing-installed-and-repository-compilers)
8. [Bug-report template](#8-bug-report-template)
9. [Worked example](#9-worked-example)
10. [Limitations of this guide](#10-limitations-of-this-guide)

## 1. Environment and compile/run

```bash
mink --version            # installed package: 1.0.2 on the audit machine; the registry now serves 1.0.4
./target/release/mink.exe --version   # repository build: mink 1.0.5 (1.0.4 at audit time)
mink help                 # command list
```

A MINK program is one or more `.mink` files. `fn main` is the entry point. A
`main() -> Int` return value becomes the process exit code; `main()` without a
return type exits 0.

```bash
mink build hello.mink     # writes hello.exe beside the source
mink run hello.mink       # builds and runs; propagates main's exit code
mink check hello.mink     # front end only; exit 1 on errors
mink check --json hello.mink
mink test hello.mink      # runs top-level fn test_* functions
mink repl                 # interactive compile-eval session; EOF/:quit exits
mink explain E-T01        # documented explanation for a code
mink explain              # list documented codes
```

`mink run` propagates the program's exit code: `fn main() -> Int { return 7; }`
exits 7 (verified). `mink build` prints one line naming the source, output and
target; that line goes to stdout, so do not treat stdout as program output when
scripting a build.

There is no `-o` output option in this CLI. A build writes beside the source
with the source stem and `.exe`. Keep generated executables out of source
directories by copying the source to a disposable directory first.

## 2. Exit codes

| Code | Source | Meaning |
| ---: | --- | --- |
| 0 | program or compiler | Success |
| 1 | `mink check`, `mink test` | Diagnostics present / a test failed |
| 2 | `invsync` example | Usage error (application convention) |
| 3-5 | `invsync` example | Invalid input / unreadable input / unwritable output |
| 104 | runtime | `E-R04` invalid free (`100 + 4`) |
| 106 | runtime | `E-R06` memory leak at exit (`100 + 6`) |
| 111 | runtime | `E-R11` map key not found (`100 + 11`) |
| any | `main() -> Int` | The returned value, unless a runtime error overrides it |

The 100-offset convention means runtime error codes surface as three-digit
process codes. `rt_exit(n)` with live allocations does **not** guarantee `n`:
the exit-time leak check still runs and reports `E-R06`, exit 106. Free owned
values before calling `rt_exit`.

Compiler diagnostics write to stderr and exit non-zero. `mink check --json`
writes a JSON object to stdout with `success`, `files_checked`, `token_count`
and an `errors` array, and still exits 1 when `success` is false.

## 3. Standard-library resolution

The two distributions differ by design:

- **Installed npm package.** `bin/mink.exe` sits beside `stdlib/`, so
  `<package>/stdlib` is found from any working directory. Verified: building a
  `mod filesystem;` program from an unrelated temporary directory succeeded.
- **Repository build.** `target/release/mink.exe` looks for `<exe>/../stdlib`
  (which is `target/stdlib`, absent) and then `<cwd>/stdlib`. Run it from the
  repository root, or copy the needed `stdlib/*.mink` files beside the source.
  Verified: the same program built from a foreign directory fails with
  `E-S01: module file 'filesystem.mink' not found`.

A local file shadows the bundled module. This bit the audit itself: a probe
named `strings.mink` containing `mod strings;` loaded itself, so every stdlib
function was "not found". If every function from one module is missing, check
for a name collision first.

Module lookup order, first match wins:

1. `name.mink` beside the declaring file;
2. `name/mod.mink` beside it;
3. `stdlib/name.mink` next to the installed compiler;
4. `stdlib/name.mink` under the current working directory;
5. package roots from the active project or environment.

`mod environment;` loads and exposes `env_get`, `env_set`, `env_has`,
`env_remove` and `env_cwd`. Before Session 182 the module contained only
`extern fn` declarations and failed to parse (`E-P01`); that was a defect in
the module, not in module loading. The `rt_env_*` intrinsics are also
predeclared and work without loading the module.

## 4. Reading diagnostics

Real diagnostics from this audit, with what they do and do not establish.

### 4.1 Syntax

```mink
fn main() {
    let x = ;
}
```

```text
mink: error[E-P03]: expected an expression
  --> d_syntax.mink:2:13
```

`E-P03` establishes that the parser reached a position where an expression was
required and found a terminator. It does not establish why; check the previous
token and the statement's terminating semicolon. Several `E-P01`/`E-P11`
diagnostics can cascade from one missing brace; fix the first error and rerun.

### 4.2 Type mismatch

```mink
fn f(x: Int) -> Int { return x; }
fn main() { f("s"); }
```

```text
mink: error[E-T01]: expected `Int`, found `Str`
  --> d_type.mink:3:7
```

This is precise for the annotated position. It is less helpful when the
message names an unresolved generic: `Map<unresolved, ...>` means a map
component could not be inferred; add the annotation (`fn f(m: Map<Str, Int>)`).

### 4.3 Module not found

```mink
mod nosuch;
fn main() {}
```

```text
mink: error[E-S01]: module file 'tmp/s178\nosuch.mink' not found
  --> d_mod.mink:1:1
```

`E-S01` covers both an unresolved module and an unresolved name. Read the path
it prints: the resolver searched beside the declaring file first. The printed
path can mix separators (`\` on Windows), so do not paste it into a shell
without normalizing.

### 4.4 Ownership

```mink
fn main() {
    let s = rt_str_concat("a", "b");
    rt_str_free(s);
    rt_str_free(s);
}
```

```text
mink: error[E-S10]: use of moved value `s`
  --> d_move.mink:4:17
```

`E-S10` is a compile-time ownership error. It means the checker believes the
value was moved; it does not decide whether your program is leak-free. A value
freed inside a conditional is treated as moved for the rest of the function,
so prefer one return path with unconditional cleanup.

Qualified module calls are unsupported and produce a misleading diagnostic:

```text
mink: error[E-T15]: `` is not a valid key type: cannot find type helper
```

The message points at map key typing, but the real issue is that
`helper::double(4)` is not a supported expression. Call `double(4)` after
`mod helper;`.

### 4.5 What diagnostics do not tell you

- A clean compile says nothing about runtime behavior, leaks, or file
  permissions.
- `E-S01 cannot find name` after `use x;` usually means the file was never
  loaded: `use` does not load. Add `mod x;`.
- `mink explain` documents 50 codes and does **not** include runtime `E-R04`,
  `E-R06` or `E-R11`: `mink explain E-R06` prints `unknown error code`. Do not
  assume an absent explanation means an invalid code.

## 5. Runtime failures, leaks and invalid frees

Runtime errors write to **stderr**, prefix output with `mink: runtime error`,
and exit with a 100-offset code. Output written before the failure is kept.

### 5.1 Leak

```mink
fn main() -> Int {
    let s = rt_str_concat("a", "b");
    rt_print_str(s);
    return 0;
}
```

stdout: `ab`. stderr:

```text
mink: runtime error[E-R06]: memory leak: live allocations remain at exit
```

exit 106. Every owned string, collection handle, directory handle and arena
must be released. The check runs after `main` returns and also after
`rt_exit`.

### 5.2 Invalid free

```mink
mod process;

fn main() -> Int {
    let out = process_stdout();
    rt_print_str(out);
    rt_str_free(out);   // wrong: BSS-backed capture buffer
    return 0;
}
```

```text
mink: runtime error[E-R04]: invalid free: the pointer is not the start of a live allocation
  at app.mink:7
```

exit 104. `process_stdout()` and `process_stderr()` must not be freed. The same
error appears when a second free is hidden behind a helper (for example calling
`zip_free` after `zip_finish`, which already frees the archive arena).

### 5.3 Separating environment failures from defects

An `E-S01 module file ... not found` from the repository compiler outside the
repository root is an environment/layout issue, not a compiler defect. Confirm
by running the installed compiler on the same source from the same directory,
or by rerunning the repository compiler with cwd set to the repository root.
Record both results.

## 6. Testing methodology

### 6.1 Minimal test program

Reduce the failure to one file with one `main`, no unrelated modules, and no
timing or randomness. Print values rather than asserting them first; then turn
the observation into an assertion.

### 6.2 `mink test` and the assert module

```mink
mod assert;

fn test_add() {
    assert_eq_int(2 + 2, 4);
}

fn test_strings() {
    assert_eq_str("ab", "ab");
}
```

```bash
mink test demo.mink
```

```text
mink: test: found 2 test(s) in 'demo.mink'
  PASS: test_add
  PASS: test_strings

mink: test result: 2 passed, 0 failed, 2 total
```

A failing test prints `FAIL: name - ...`, a summary and `Failed tests:`, and
the process exits 1 (verified). Assertions call `rt_exit(1)` on failure.

Observed limitation: the `assert_eq_int` failure message was truncated to
`assertion failed: expected ` in the test runner's stderr. Do not build tooling
that parses assertion text; use the exit code and print your own values.

### 6.3 Shell/Python harness for exact output

For anything beyond unit assertions, drive the executable from a script:

- build once into a disposable directory;
- run each case with `cwd` set to its own fresh directory;
- capture stdout, stderr and the exit code separately;
- normalize CRLF (`\r\n` to `\n`) before comparing text, or compare bytes when
  line endings are part of the contract;
- assert exact expected text, not substrings;
- check artifacts (files written) and their absence on failure paths;
- delete only the temporary tree you created.

`benchmarks/m6/session180/verify.py` is a complete example: 12 cases, exact
stdout/stderr/exit/report checks, one temp root, no repository writes.

### 6.4 Cases worth writing

| Case | What it catches |
| --- | --- |
| Normal input | Baseline behavior and deterministic output |
| Header-only / empty | Off-by-one row handling, empty-file confusion |
| Missing file | `fs_read` returning `""` for both missing and empty |
| Malformed record | Field-count validation, partial output |
| Non-numeric / negative | Parse flags (`Bool` vs `Int`) and range checks |
| Boundary values | Zero, large integers, CRLF input |
| Unwritable artifact | Failure ordering: no success message before the write |
| Extra arguments | Silent argument confusion |
| Repeated run | Determinism and leftover state |

### 6.5 Isolating filesystem effects

Run in a fresh temporary directory per case. `fs_read` cannot distinguish a
missing file from an empty one; call `fs_exists` or `fs_file_size` (`-1` vs
`0`) when the distinction matters. Never test against repository files.

### 6.6 Reproducing an issue

1. Record the exact compiler path and `--version`.
2. Record `pwd` and the source path (module resolution depends on both).
3. Reduce to one file; inline every helper that is not part of the bug.
4. State expected and actual stdout/stderr and exit code.
5. Run the case three times to establish determinism.
6. Run it with both compilers and from both a foreign directory and the
   repository root when the suspicion is module resolution.

### 6.7 Do not mistake an environment problem for a defect

The three common false alarms are: repository compiler run outside the
repository root (stdlib not found), a local file shadowing a stdlib module, and
an executable run from a different directory than the one that contains its
data files. Each is visible in the search path or `pwd`; confirm before
reporting.

## 7. Comparing installed and repository compilers

```bash
# installed (1.0.2 on the audit machine; the registry now serves 1.0.4)
mink build app.mink && ./app.exe

# repository (1.0.5 now; 1.0.4 at audit time), from the repository root
./target/release/mink.exe build /path/to/app.mink
```

Record both versions. At audit time they differed (1.0.2 vs 1.0.4); the
registry now serves 1.0.4 and the repository build is 1.0.5, so they can still
differ. A difference in behavior is not automatically a regression in either. When
comparing, hold the source, arguments and working directory constant; change
only the compiler path. The Session 180 verifier supports this directly:

```bash
python benchmarks/m6/session180/verify.py
python benchmarks/m6/session180/verify.py --compiler /path/to/other/mink.exe
```

## 8. Bug-report template

```markdown
### Summary
One sentence: what fails, where.

### Compiler
- Version: `mink --version` -> ...
- Binary: installed npm package | repository target/release/mink.exe
- Path: ...
- OS/arch: Windows 11 x86_64 (or exact)

### Minimal source
```mink
fn main() -> Int { ... }
```

### Commands
- `pwd`: ...
- Build: `... build repro.mink`
- Run: `... repro.exe`

### Expected
Exact stdout/stderr and exit code.

### Actual
Exact stdout/stderr and exit code. Include `od -c` output when whitespace or
line endings matter.

### Reproduction
- Deterministic? yes/no (three runs)
- Also reproduces with the other compiler? yes/no/not tested
- Also reproduces from a different working directory? yes/no
- Stdlib module involved and how it was resolved: ...

### Notes
What you already ruled out (environment, file shadowing, missing data file).
```

## 9. Worked example

`benchmarks/m6/session180/invsync.mink` compiles, runs and passes its verifier.
It reports `invsync: ok` plus a one-line summary on stdout and writes
`report.txt`. Failures produce no report and a non-zero exit. The verifier
covers all of the case classes above.

Two defects were found while building it, and both are worth copying into
future tests:

1. The first report version concatenated the summary and the first category
   without a newline (`...total_value=204category=hardware80`). Exact-output
   assertions caught it; visual inspection had not.
2. A `Vec<Str>` accumulated inside nested `if`/`else` blocks reported length 16
   for one element and produced `E-R04` on `rt_vec_free`. The application was
   restructured to derive order from the input instead. A reduced reproduction
   is preserved in the consolidated session record.

## 10. Limitations of this guide

- Windows x86_64 only; Linux behavior is outside this audit.
- The installed package used for the audit was 1.0.2 (the registry now serves
  1.0.4, and the repository build is 1.0.5); version-specific behavior may
  differ elsewhere.
- `mink repl`, package commands (`init`, `add`, `install`, environments) and
  `--target` were not exercised.
- `mink explain` coverage is incomplete for runtime codes.
- No external developer was observed; timings are not human learning times.
