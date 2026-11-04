# M6 Session 176 - first program and basic language learning

**Date:** 2026-11-04
**Status:** **COMPLETE**
**Starting HEAD:** `39de912`
**Branch:** `main` (33 ahead / 1 behind `origin/main`; 63 pre-existing worktree entries)
**Compiler under test:** installed npm package `@p4inz-code/mink@1.0.4`
**Evidence mode:** Mode B internal cold-start simulation (no external developer)

## 1. Objective

Determine whether a developer can learn enough MINK from developer-facing
material to write, compile, run and modify programs using variables, primitive
types, operators, conditionals, loops, functions, returns, strings, collections
and module imports, then build three small useful programs.

No compiler, runtime, standard-library, README or language-documentation file
was changed. All new source is under `benchmarks/m6/session176/`.

## 2. Environment

| Item | Value |
| --- | --- |
| OS | Windows x86_64 |
| Compiler | `tmp/m6_install_175/node_modules/@p4inz-code/mink/bin/mink.exe` |
| Version | `mink 1.0.4` |
| Harness | `benchmarks/m6/session176/verify.py` |
| Work directories | Disposable directories under `tmp/m6_session176_work` |
| Source tree output | None; each case is copied out before building |

## 3. Exercises

All exercises were written from the README, `CORE_GRAMMAR.md`, and existing
examples. The verifier builds and runs each in a fresh directory and checks
exact stdout after CRLF normalization and the process exit code.

| ID | Construct | Source | Build | Run | Exit | Result |
| --- | --- | --- | ---: | ---: | ---: | --- |
| 01 | Minimal entry point | `01_hello.mink` | 0.007s | 0.007s | 0 | PASS |
| 02 | Variables and primitive types | `02_variables.mink` | 0.007s | 0.007s | 0 | PASS |
| 03 | Expressions and operators | `03_operators.mink` | 0.006s | 0.006s | 0 | PASS |
| 04 | Conditionals | `04_conditionals.mink` | 0.006s | 0.007s | 0 | PASS |
| 05 | While and for loops | `05_loops.mink` | 0.006s | 0.006s | 0 | PASS |
| 06 | Functions and parameters | `06_functions.mink` | 0.006s | 0.007s | 0 | PASS |
| 07 | Return values | `07_returns.mink` | 0.006s | 0.007s | 0 | PASS |
| 08 | Strings | `08_strings.mink` | 0.006s | 0.007s | 0 | PASS |
| 09 | Arrays and maps | `09_collections.mink` | 0.006s | 0.006s | 0 | PASS |
| 10 | Module import | `10_imports.mink` | 0.008s | 0.007s | 0 | PASS |

Exact outputs:

```text
01: 42
02: 3 / 2.5 / 1 / mink
03: 10 / 4 / 21 / 2 / 1 / 14 / 3 / 1 / 1
04: -1 / 0 / 1
05: 10 / 15 / 3
06: 9 / 9
07: -1 / 0 / 1 / 10 / 20
08: mink-lang / 9 / 109 / 176
09: 100 / 2 / 200 / 1
10: module-ok / 9 / 0
```

Each slash above is a line break in the actual output.

## 4. Programs

### Program A - basic calculator

`program_a_calculator.mink` defines `add`, `sub`, `mul` and `divide`, uses a
conditional guard for division by zero, and prints a header followed by five
integer results and one error result.

```text
add,sub,mul,div,neg
90
78
504
14
-78
div0=error
```

Exit 0. The zero-division path is exercised by `divide(7, 0)`.

### Program B - data processor

`program_b_data_processor.mink` builds a `Map` of integer values, computes
count, sum, minimum, maximum and a scaled mean, and uses scalar helper
functions. The map stays inside `main` (see finding L2).

Default output:

```text
count,sum,min,max,mean_x100
8
360
10
80
4500
```

Boundary modes, selected with the first argument:

```text
program_b_data_processor.exe empty
count,sum,min,max,mean_x100
0
0
-1
-1
0

program_b_data_processor.exe single
count,sum,min,max,mean_x100
1
10
10
10
1000
```

All three exits are 0.

### Program C - modular utility

`program_c_modular_utility.mink` imports the standard-library `filesystem`
module, writes, reads, sizes and removes a file, and handles two failure
conditions: a missing file and a write into a nonexistent directory.

```text
write,read,exists,size,missing_len,missing_size,bad_write,removed,cleanup
8
hello-m6
1
8
0
-1
-1
0
1
```

Exit 0 only when every check passes; the program returns 1 if the write,
read, missing-file behavior, bad-path behavior or cleanup differs.

## 5. Exact commands

```bash
python benchmarks/m6/session176/verify.py \
  --compiler tmp/m6_install_175/node_modules/@p4inz-code/mink/bin/mink.exe
```

The verifier performs, per case:

```text
mink build <source>.mink
<source>.exe [arguments]
```

and compares stdout and exit status. It also supports `--compiler` for an
independent compiler binary.

## 6. Failed attempts and recovery

### F1 - print intrinsics are line-oriented

The first version tried to compose labels and values on one line:

```mink
rt_print_str("add=");
rt_print_int(90);
```

Actual output:

```text
add=
90
```

Adding `rt_print_str("\n")` produced extra blank lines because every print
intrinsic already terminates its output. The first verifier run passed 7/15;
02, 08, 10, Program A and Program C failed only on output shape.

**Recovery:** print a single header line, then one value per print call. This
is consistent with the runtime's line-oriented print contract, but that contract
is not stated in the README or `CORE_GRAMMAR.md`.

### F2 - map handles are not `Int`

The first Program B passed a map handle through `Int`-typed parameters:

```text
mink: error[E-T01]: expected `Map<unresolved, unresolved>`, found `Int`
```

Three builds failed with five diagnostics each. There is no developer-facing
Map type reference in the README or `docs/language/`; the inferred type name
appears only in the compiler error.

**Recovery:** keep all map operations in `main`; use scalar helper functions
for `value_for` and `mean_x100`. This avoids an undocumented map-crossing
function signature. The workaround is recorded as a language/documentation gap,
not as a compiler defect.

### F3 - repository compiler cannot resolve stdlib from a temp directory

Running the same verifier with `target/release/mink.exe` passed 13/15. The two
module cases failed with:

```text
mink: error[E-S01]: module file 'filesystem.mink' not found
```

The packaged compiler resolves its bundled `stdlib/` from the disposable
directory; the repository compiler does not. Root cause was not investigated
and no source was changed. Session 176 uses the installed 1.0.4 compiler, as
specified.

## 7. Documentation consulted

| Lookup | Material |
| --- | --- |
| 1-4 | README: What is MINK, Install, Quick Start, Build and Run |
| 5-12 | `CORE_GRAMMAR.md` sections 5, 6, 7, 8, 17, 19, 23, 28-30 |
| 13 | `examples/fs_util/main.mink` |
| 14 | `examples/freq_count/main.mink` |
| 15 | `examples/system_report/main.mink` |
| 16 | `examples/system_report/README.md` |
| 17 | `stdlib/filesystem.mink` signature search |
| 18 | `src/runtime/intrinsics.rs` float-print confirmation |
| 19 | existing tests for float/int printing |

The last two are repository-internal lookups and are counted as undocumented
knowledge events, not as developer documentation.

## 8. Undocumented knowledge required

1. `rt_print_str` and `rt_print_int` each emit a terminating newline; there is
   no same-line composition without building one `Str`.
2. `rt_print_float` exists and formats a `Float`; this was confirmed in runtime
   source/tests, not in the README.
3. Map handles have a distinct `Map<...>` type; they cannot be stored in an
   `Int` binding or passed to an `Int`-typed function.
4. `mod filesystem;` makes `fs_*` callable unqualified in practice, while
   `CORE_GRAMMAR.md` says module items become visible via `use`. The examples
   use the unqualified form.
5. Returned `Str` values and map handles require `rt_str_free` / `rt_map_free`;
   ownership rules are demonstrated in examples, not in a reference.
6. `fs_write` to a missing directory and `fs_file_size` on a missing file
   return `-1`; `fs_read` on a missing file returns an empty string. These were
   verified by execution, not found in a developer-facing contract.

## 9. Blocker classification

| Category | Finding |
| --- | --- |
| Documentation gap | Line-oriented print contract; Map type; module/use wording; filesystem failure return conventions |
| Unclear language behavior | Map handles crossing function boundaries; no documented map type annotation |
| Compiler diagnostic problem | `E-T01` reports `Map<unresolved, unresolved>` but gives no correction or annotation hint |
| Actual compiler defect | None confirmed in this session |
| Standard-library limitation | No documented route for a function to accept a map handle; not proven impossible |
| Unsupported feature | None required by the three programs |

## 10. Measurements

- **Instrumented verification window:** 195 seconds, from exercise-directory
  creation through two installed-compiler runs, one repository-compiler run
  and the example compile sweep. Earlier document reading was not separately
  timed.
- **Build time:** 0.005-0.011 seconds per case.
- **Run time:** 0.006-0.009 seconds per case.
- **Documentation lookups:** 19, including two internal source searches.
- **Undocumented knowledge events:** 6.
- **Failed attempts:** 1 first verifier run (7/15), 3 map builds, 1 repository
  compiler run (13/15).
- **Workarounds:** line-oriented output shaping; map operations kept in `main`.
- **Blockers:** none that prevented a program; the map typing issue required a
  workaround.

These are process measurements, not human learning times. Per-exercise
authoring time was not separately instrumented; the 195-second window covers
the whole authoring and verification batch, so it must not be presented as a
per-task human timing.

## 11. Verification

| Check | Result |
| --- | --- |
| Installed 1.0.4, run 1 | **15/15 PASS**, exit 0 |
| Installed 1.0.4, run 2 | **15/15 PASS**, exit 0 |
| Repository compiler | 13/15, exit 1; two `E-S01` module failures |
| Program A zero division | exit 0, `div0=error` |
| Program B empty / single | exit 0, exact boundary outputs |
| Program C missing file / bad path | exit 0, expected `-1` and empty values |
| Existing examples, compile only | **15/15 builds succeeded** |
| ASCII check | all Session 176 sources are ASCII |
| Standalone execution | every case executed from a disposable directory outside the source tree |

The existing-example sweep compiled each `examples/*/main.mink` with the
installed compiler in a temporary copy. It did not execute those examples; a
runtime audit remains out of scope for this session.

## 12. Learning usability findings

1. The smallest workflow is excellent: one source file, `mink build`, run the
   `.exe`; no configuration.
2. Basic types, conditionals, loops and typed functions are learnable from the
   grammar, but the README does not teach them.
3. The print API is the largest early surprise: its line-oriented behavior is
   not documented where a beginner will look.
4. Collections are usable, but map typing is opaque; the compiler tells the
   developer the type only after a failed build.
5. Module imports work, but the grammar's `use` wording and the examples'
   unqualified calls disagree.
6. All three programs were implementable without a language change.

## 13. Remaining limitations

- No external developer; this is an internal simulation by the author.
- No per-exercise human timing; only process build/run times and a batch window.
- The repository-compiler stdlib failure was not root-caused or fixed.
- Map type annotations were not explored; the workaround avoids them.
- Existing examples were compile-checked, not runtime-checked.
- Session 177 must use these findings; no README or language-documentation
  correction was made here.

## 14. Repository state

- Starting HEAD `39de912`; branch `main`; 33 ahead / 1 behind `origin/main`.
- 63 pre-existing M3/M4 worktree entries preserved; no unrelated file changed.
- New files: `benchmarks/m6/session176/` and this record.
- No compiler, runtime, standard-library, README or language-documentation
  change.
- Commit hash and final HEAD are reported in the session report and ledger.
