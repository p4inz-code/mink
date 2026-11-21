# Clean Windows installation test

A step-by-step check of what a first-time Windows user actually gets from MINK. The machine
this was prepared on is described in
[`../implementation/SESSION_287_CLEAN_PC_PREPARATION.md`](../implementation/SESSION_287_CLEAN_PC_PREPARATION.md):
npm `latest` is **1.0.4**, nothing from MINK is installed, and no `.mink` association exists,
so the test starts from zero.

Every command and expected result below was executed against the real artifacts — the
tarball downloaded from the registry for `1.0.4`, and a locally packed `1.0.5` candidate.
The test itself has **not** been performed as an installation; running it is the owner's
step.

Two tracks exist, and they are not interchangeable:

| Track | Package | Association and icon | Purpose |
| --- | --- | --- | --- |
| A — public stable | `@p4inz-code/mink@1.0.4` from npm | **cannot be tested** — 1.0.4 ships no `windows/` | proves the public install path a stranger follows |
| B — local candidate | `p4inz-code-mink-1.0.5.tgz`, unpublished | testable — the kit is in the tarball | proves the fixed compiler, stdlib and file-type kit |

Run track A first. Track B is optional and must not be described as a public release.

## 0. Preconditions

- Windows x64, Node.js 18 or newer (this machine: Node 24.14.0, npm 11.9.0).
- A terminal you can run PowerShell from.
- No `mink` on `PATH` before starting:

```powershell
where.exe mink        # expected: INFO: Could not find files for the given pattern(s).
mink --version        # expected: 'mink' is not recognized ...
```

- No MINK file type registered:

```powershell
Test-Path 'HKCU:\Software\Classes\.mink'              # expected: False
Test-Path 'HKCU:\Software\Classes\MINK.SourceFile'    # expected: False
```

## Track A — public stable 1.0.4

### A1. Install

```powershell
npm install -g @p4inz-code/mink
```

No administrator terminal is needed when the npm global prefix is user-writable. On this
machine the prefix is `C:\Users\Admin\AppData\Roaming\npm`, which is. If npm reports
`EACCES` or `EPERM`, the prefix is a protected directory and an elevated terminal is
required — that is the only step in this document that can need one.

Expected: about 1.3 MB unpacked to 28 files, with `added 1 package` or similar.

### A2. Verify the version and the resolved executable

```powershell
npm view @p4inz-code/mink version     # expected: 1.0.4
mink --version                        # expected: mink 1.0.4
(Get-Command mink).Source             # expected: ...\npm\mink.cmd
npm ls -g --depth=0                   # expected: @p4inz-code/mink@1.0.4
```

`npm view` must say `1.0.4`. If it ever says `1.0.5`, that release has been published since
this checklist was written and the "1.0.5 is unpublished" statements in the repository are
out of date.

### A3. Help

```powershell
mink help
```

Expected: the usage block listing `build`, `run`, `check`, `test`, `repl`, `explain`,
`version`, `help`, then the package commands (`init`, `add`, `remove`, `install`, `update`,
`env`), then options. `mink build --help` is **rejected** on purpose:

```text
mink: error: unknown option '--help' for 'build'
Run 'mink help' for usage.
```

### A4. A minimal program

```powershell
New-Item -ItemType Directory -Force C:\mink-test | Out-Null
Set-Location C:\mink-test
@'
fn main() {
    rt_print_int(42);
    return 0;
}
'@ | Set-Content -Encoding ASCII hello.mink

mink check hello.mink    # expected: ... passed parsing, semantic analysis, ... (14 tokens)
mink run hello.mink      # expected: 42
mink build hello.mink    # expected: ... -> 'C:\mink-test\hello.exe' (target: x86_64-windows-pe ...)
.\hello.exe              # expected: 42
```

The built executable is standalone: 40,960 bytes when measured here, running with no MINK
installation present.

### A5. Standard library — expect the documented 1.0.4 defect

```powershell
@'
mod strings;

fn main() {
    let s = rt_str_concat("hi", "");
    let u = str_to_upper(s);
    rt_print_str(u);
    rt_str_free(u);
    return 0;
}
'@ | Set-Content -Encoding ASCII strings.mink

mink run strings.mink
```

Expected on 1.0.4: prints `HI`, then aborts with **exit code 106**:

```text
mink: runtime error[E-R06]: memory leak: live allocations remain at exit
```

That is the artifact's defect, not a mistake in the program: the same source exits 0 on
the 1.0.5 candidate. It affects string transforms (`str_trim`, `str_to_upper`,
`str_to_lower`, `str_reverse`, and the rest of that family), not intrinsics. Anything that
matters for the public verdict should be noted against this, not worked around silently.

`mink test` works normally, because `mod assert;` is not affected:

```powershell
@'
mod assert;

fn test_math() {
    assert_eq_int(2 + 2, 4);
}

fn main() {
    return 0;
}
'@ | Set-Content -Encoding ASCII t.mink

mink test t.mink
```

Expected: `found 1 test(s)`, `PASS: test_math`, `1 passed, 0 failed, 1 total`, exit 0.

### A6. The file association cannot be registered from this package

```powershell
$package = Join-Path (npm root -g) '@p4inz-code\mink'
Get-ChildItem $package        # expected: bin, package.json, stdlib — no windows
```

There is no `windows\register-mink-filetype.ps1` in 1.0.4, so steps A7–A9 are **not
applicable to track A**. The correct outcome is a recorded gap, not a workaround that
points the association at a development directory.

## Track B — local 1.0.5 candidate (optional)

Candidate artifact, built from this repository without publishing:

```text
F:\Works\Mink\tmp\cleanpc\p4inz-code-mink-1.0.5.tgz
SHA-256 890bf1fb769a551f0036b59250de65747f76c24738ec11cd419f7c76e5c56476
35 files · bin/mink.exe · 27 stdlib modules · windows/ (2 ICOs, mink.rc, script, README)
```

This file lives under `tmp/`, which is git-ignored: it is a local build artifact, not a
release. To test it, install the tarball into an isolated prefix rather than globally, so
the machine stays free of MINK afterwards:

```powershell
$Prefix = "$env:LOCALAPPDATA\mink\1.0.5-test"
npm install --prefix $Prefix "F:\Works\Mink\tmp\cleanpc\p4inz-code-mink-1.0.5.tgz"
$Mink = "$Prefix\node_modules\@p4inz-code\mink\bin\mink.exe"
& $Mink --version                     # expected: mink 1.0.5
```

### B1. The strings program, fixed

```powershell
Set-Location C:\mink-test
& $Mink run strings.mink              # expected: HI, exit 0
```

### B2. Register the association from the installed candidate

```powershell
$package = Join-Path $Prefix 'node_modules\@p4inz-code\mink'
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1"
```

Expected tail of the output: the ProgID, `Content Type` `text/x-mink`, `PerceivedType`
`text`, a `DefaultIcon` under `$package\windows\mink-file.ico`, and
`Done. MINK file association registered.` Exit code 0.

Running it a second time must produce the same state (idempotent), not a second set of
values.

### B3. Inspect the icon in Explorer

```powershell
New-Item -ItemType File -Force C:\mink-test\fresh.mink | Out-Null
Start-Process explorer.exe C:\mink-test
```

In Explorer switch to **Details** and confirm the Type column reads **MINK Source File**
next to `fresh.mink`; switch to **Large icons** and confirm the dark document sheet with
the white mink mark and the MINK wordmark. Both views were inspected on this machine with
the same kit, and the shell's own bitmap for `.mink` matches the official 32 px artwork
(10 of 1,024 pixels differ, all antialiased edges; a generic page icon differs in 696).

If an older generic icon is still cached, refresh without clearing the whole icon cache:

```powershell
ie4uinit.exe -show
```

### B4. Paths containing spaces

```powershell
$dir = "C:\mink test dir"
New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item C:\mink-test\strings.mink "$dir\spaced.mink"
Set-Location $dir
& $Mink run spaced.mink               # expected: HI, exit 0
```

### B5. Double-click never executes source

```powershell
Test-Path 'HKCU:\Software\Classes\MINK.SourceFile\shell\open'   # expected: False
```

With no `open` command registered, Windows asks which editor to use and remembers the
answer. Nothing in the kit can compile or run a `.mink` file.

### B6. Uninstall and verify the cleanup

```powershell
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1" -Uninstall
Test-Path 'HKCU:\Software\Classes\.mink'                          # expected: False
Test-Path 'HKCU:\Software\Classes\MINK.SourceFile'                # expected: False
Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.mink\OpenWithProgids'
Remove-Item -Recurse -Force $Prefix                               # removes the test install
```

The uninstaller removes only MINK-owned values. A per-user "Open with" choice, and any
editor you picked by hand, are user-owned and are left alone.

## What the owner should record

For each track: version printed, resolved executable, whether `check`/`run`/`build`/`test`
matched the expected output, the A5 exit code, whether the association and icon appeared in
both Explorer views, the uninstall result, and the machine state afterwards. Any deviation
from this document is a finding worth recording against the exact command that produced it.
