# M6 Session 185 - Windows installation and first-run audit

**Date:** 2026-11-05
**Status:** **COMPLETE (audit)**. Repository-side association behavior is safe;
the **published package cannot provide it** because `windows/` is absent.
**Starting HEAD:** `40facceae9161666db5fcd68f8e8ba1c00c9fdc0`
**Ending code HEAD:** `b67fda4fb8044c0b3db1b06ba8230d3ad0a34cb1`
**Branch:** `main`
**Worktree preserved:** 63 pre-existing entries; 63 after the bundle fix.

**Measurement:** measured from the first package artifact to the documentation
check: **621 s (10 min 21 s)**. This covers registry inspection, clean
installs, first-run checks, association validation and record writing;
baseline review preceded the marker. No human study is claimed.

## 1. Registry state before testing

Read-only queries only:

| Location | State |
| --- | --- |
| `HKCU\Software\Classes\.mink` | **ABSENT** |
| `HKCU\Software\Classes\MINK.SourceFile` | **ABSENT** |
| `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.mink` | Present: `OpenWithList` subkey (pre-existing, not a MINK ProgID) |

No MINK file association existed on this machine before the test. No
machine-wide (`HKLM`) keys were read as mutable targets or changed.

## 2. Association harness

The existing repository harness was run without administrator rights:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File `
  validation/windows_filetype/verify_filetype_association.ps1
```

Result:

```text
RESULT: PASS - 56 checks
```

The harness covers registration, command quoting, paths with spaces, a Unicode
installation path, repeated runs, editor binding, rebinding without `-Editor`,
uninstall from a clean extension, and restoration of the starting registry
state. It restored the registry to the state in section 1.

The implementation contract was also inspected in
`npm/mink/windows/register-mink-filetype.ps1`:

- all writes are under `HKCU`; no `HKLM` writes and no elevation request;
- registration creates `MINK.SourceFile` and records the extension owner;
- **no default `open` verb is installed unless `-Editor` is supplied**, so a
  double-click does not execute `.mink` source;
- with `-Editor`, the command is `"<editor>" "%1"` -- both the executable and
  the source path are quoted;
- `-Uninstall` removes only values/keys the script created and restores a
  recorded prior owner; unrelated registrations are preserved;
- `-Force` is required to replace another owner.

**Classification: VERIFIED PASS** for the repository implementation and its
safety contract.

## 3. Published-package association is blocked

The published 1.0.4 tarball contains **no `windows/` directory**. The README
association instructions point at a package-relative `windows/` script, so an
external developer who installs from npm cannot register `.mink` files or
obtain the icons. This is a **VERIFIED DISTRIBUTION DEFECT**, not an
association-script defect. The script is only available in a repository
checkout at present.

## 4. CLI first-run workflow

The workflow used the cleanly installed published package, a project path with
spaces, and no repository `stdlib` in the working directory.

| Step | Exact command / condition | Result |
| --- | --- | --- |
| Install | `npm install --prefix ... @p4inz-code/mink@1.0.4` | PASS |
| Discover/version | `<package>/bin/mink.exe --version` | `mink 1.0.4` |
| Create project | `project with space/hello.mink` | PASS |
| Build first program | `mink build hello.mink` | exit 0 |
| Run | `./hello.exe` | `42`, exit 0 |
| Add stdlib | `mod strings;` + `str_trim` | build 0; **run 106, E-R06** |
| Environment module | `mod environment;` | **build 1, E-P01** |
| Intentional type error | `let x: Int = "not an int";` | exit 1, `E-T01 expected Int, found Str` with source span |
| Correct and rebuild | `let x: Int = 7; rt_print_int(x);` | build 0; run prints `7`, exit 0 |
| Second project | `second project/two.mink` | build 0; run `99`, exit 0 |
| Build writes | package tree hash before/after | unchanged |

The diagnostic is actionable: `mink: error[E-T01]: expected `Int`, found
`Str`` plus the file, line and column. The corrected build is immediate.

## 5. First-run usability assessment

This is an internal reproducible simulation, not an external developer study.

**What works:** install, version reporting, project creation, intrinsic-only
build/run, paths with spaces, foreign working directories, multiple projects,
clear type diagnostics, no administrator requirement for prefix installation,
and no writes into the package.

**What blocks a normal onboarding path:**

1. The published standard library is pre-fix. A first standard-library example
   (`str_trim`) prints the expected text and then exits 106 with E-R06. The
   README and reference describe the fixed behavior.
2. `mod environment;` is advertised as repaired but the published package
   still contains the old `extern fn` module and fails E-P01.
3. The published package has no `windows/` directory, so the documented file
   association cannot be installed from the npm artifact.
4. The version string `1.0.4` identifies both the older published tag and the
   newer repository HEAD. A user cannot distinguish them by version alone.
5. README still contains a pre-existing paragraph about 1.0.3; it was not
   rewritten because of the protected worktree.
6. A global `-g` install and a network-disabled run were not tested; prefix
   installs are the verified path.

**Onboarding verdict:** the first intrinsic program succeeds, but the
standard-library and association steps do not. The published 1.0.4 artifact is
not sufficient for the documented Windows first-run experience.

## 6. Documentation

Created `docs/language/WINDOWS_INSTALLATION.md` with verified install, first
program, standard-library, association and removal commands, plus an explicit
published-1.0.4 caveat and version guidance. `README.md` was not modified;
its pre-existing worktree content remains protected.

## 7. Findings

| Finding | Classification |
| --- | --- |
| Repository association is per-user `HKCU`, no HKLM, no admin | **VERIFIED PASS** |
| No default `open` action; double-click does not execute source | **VERIFIED PASS** |
| Editor command quotes executable and `%1`; spaces and Unicode paths | **VERIFIED PASS** |
| Uninstall is reversible and preserves unrelated settings | **VERIFIED PASS** |
| Harness restores the starting registry | **VERIFIED PASS** (56/56) |
| Published package ships the association script/assets | **VERIFIED DEFECT** - absent |
| Published package completes the stdlib first-run path | **VERIFIED DEFECT** - exit 106 / E-P01 |
| Type-error diagnostic and corrected rebuild | **VERIFIED PASS** |
| Global `-g` install, offline operation | **NOT TESTED** |
| Published file association from npm | **BLOCKED** by missing `windows/` |

## 8. M6 status consequence

The repository-side M6 closure (Sessions 182-183) remains valid for the
repository build. The **published 1.0.4 distribution gate is BLOCKED**: the
artifact on npm does not contain the stdlib and Windows assets that the M6
documentation and acceptance depend on. A corrected release is required before
M6 can be described as externally distributable. M7 was not started.

## 9. Exact next task

Cut and verify a corrected package from a commit containing the post-tag fixes
under a new version, then re-run this session's install, stdlib, verifier and
association checks against that tarball. Do not publish from the current
working tree, and do not start M7.
