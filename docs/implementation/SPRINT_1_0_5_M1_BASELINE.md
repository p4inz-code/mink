# 1.0.5 sprint, M1: baseline

Date: 2026-11-22. Commit under test: `d7c979e` (HEAD of `main`, equal to `origin/main`).
Machine: Windows 11, Intel i9-14900K (32 threads), 31.7 GB RAM, rustc/cargo 1.97.1.

This is the starting point for the 1.0.5 work. Every number below comes from a command run
for this record. Raw logs were kept outside the repository.

## Full debug regression

`cargo test --no-fail-fast`, run through `tools/dev_guard.ps1` (8 jobs, below-normal
priority, pause after three samples above 75% CPU). The guard recorded a **peak of 93%**
in one sample and never saw three in a row, so it did not pause. That reading came from a
WMI query, which spikes the CPU by itself (the same query reported 82% for a command that
does nothing), so the true peak of that run is not known. Afterwards the guard was
tightened and fixed: it now reads the cheap performance counter (8% peak on an idle test
command), uses 6 jobs and 2-second samples, and pauses after two consecutive samples above
70%.

First run: 112 result lines, 3,585 passed, **55 failed**, 10 ignored. Every failure was in
`tests/process_lib.rs`. Two environment causes, neither a compiler defect:

| Cause | Tests | Evidence |
| --- | --- | --- |
| `file` was not on `PATH` when cargo ran from PowerShell | 47 `linux_*` tests | `file command failed: NotFound` at `tests/process_lib.rs:532` |
| `NoDefaultCurrentDirectoryInExePath=1` was set in the process environment | 8 `s97_*` tests | `process_run("small.exe")` returned 1; `cmd.exe` stops searching the current directory when that variable is set |

Second run of `process_lib` from Git Bash with the variable removed
(`env -u NoDefaultCurrentDirectoryInExePath`): **86 passed, 0 failed**.

Combined result for `d7c979e`: **3,640 passed, 0 failed, 10 ignored.** That is the same
total Session 282 recorded, so nothing has drifted since.

The `NoDefaultCurrentDirectoryInExePath` finding also matters for users. `process_run` goes
through `cmd.exe`, so a machine with that hardening setting cannot run a program that sits
only in the current directory. It is listed as limitation L8 in the 1.0.5 register.

## The 1.0.4 string leak (D3), reproduced

Program:

```
mod strings;
fn main() {
    let s = str_to_upper(rt_str_concat("ab", "cd"));
    rt_print_str(s);
    rt_str_free(s);
}
```

| Binary | Output | Exit code |
| --- | --- | --- |
| Published 1.0.4 (`mink.exe` sha256 `2bc97c92...`, identical to tag `v1.0.4`) | `ABCD`, then `E-R06: memory leak` | **106** |
| Repository 1.0.5 | `ABCD` | 0 |

So the leak is real in what users install today, and it is already fixed in the repository.

## The E-S02 report from the clean-install test

Reproduced on both binaries. A `json.mink` or `math.mink` sitting beside the program
replaces the bundled module, because a sibling file is looked up before the bundled library
(`resolve_module_path` in `src/driver.rs`). The program then has two `main` functions. In
1.0.4 both error locations were blamed on the root file; 1.0.5 names the right file. The
sibling lookup order is still unchanged and is fixed in M2.

## Resource guard

`tools/dev_guard.ps1` is the cap used for every build and test run in this sprint. It runs
the command at below-normal priority, limits Cargo and the test harness to a fixed number of
workers, and pauses the whole process tree if total CPU stays above the cap for three
samples in a row. It measures CPU load, not temperature: Windows does not give a
non-administrator program thermal readings.

## Findings carried into the 1.0.5 register

| ID | Finding | Status |
| --- | --- | --- |
| D1 | Sibling file silently shadows a bundled module | Fix in M2 |
| D3 | 1.0.4 string transforms leak (`E-R06`, exit 106) | Fixed in the repository; ships with 1.0.5 |
| L8 | `process_run` fails for programs found only in the current directory when `NoDefaultCurrentDirectoryInExePath` is set | Document in M10; code change not planned |
| L9 | Test suite needs `file` and a Python on `PATH`, and breaks under the variable above | Document in `TESTING.md` in M10 |
