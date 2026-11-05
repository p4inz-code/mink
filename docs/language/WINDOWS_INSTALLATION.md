# Windows Installation and First Run

**Platform:** Windows x64 only. Linux is not supported by this package.
**Audited:** 2026-11-05 against the published `@p4inz-code/mink@1.0.4` artifact
and the repository build.
**Companion documents:** [`LANGUAGE_BASICS.md`](LANGUAGE_BASICS.md),
[`STANDARD_LIBRARY_REFERENCE.md`](STANDARD_LIBRARY_REFERENCE.md),
[`DEBUGGING_AND_TESTING.md`](DEBUGGING_AND_TESTING.md).

> **Published 1.0.4 caveat (important).**
> npm `1.0.4` is the tag `v1.0.4` artifact: 28 files, no `windows/`
> directory, and pre-fix `strings.mink` / `environment.mink`. On that artifact
> `str_trim` over a heap string exits 106 (`E-R06`) and `mod environment;`
> fails `E-P01`. The repository HEAD contains the fixes but has no published
> version yet. Use the published package for intrinsic-only programs, or build
> from the repository if you need the audited standard library. A corrected
> release is required; 1.0.4 will not be republished in place.

## 1. Requirements

- Windows x64 (the package declares `os: win32`, `cpu: x64`).
- Node.js 18 or newer and npm.
- No administrator rights for a per-user/prefix installation.
- A terminal in the project directory. PowerShell examples are shown.

The compiler is a native `bin/mink.exe`; Node is used only to install and
launch the package.

## 2. Install without changing the machine PATH

This is the form verified in the audit: an isolated prefix, then the declared
executable.

```powershell
$Prefix = "$env:LOCALAPPDATA\mink\1.0.4"
npm install --prefix $Prefix "@p4inz-code/mink@1.0.4"
$Mink = "$Prefix\node_modules\@p4inz-code\mink\bin\mink.exe"
& $Mink --version
```

Expected:

```text
mink 1.0.4
```

The package also creates `node_modules\.bin\mink.cmd`. A global install (`npm install -g`)
was **not tested** in this audit; use it only if your npm global prefix is
writable without elevation. If `mink --version` prints `1.0.2`, that is an
older unsupported install and is not interchangeable with 1.0.4.

## 3. First program

```powershell
New-Item -ItemType Directory -Force C:\work\hello | Out-Null
Set-Location C:\work\hello
@'
fn main() -> Int {
    rt_print_int(42);
    return 0;
}
'@ | Set-Content -Encoding ASCII hello.mink
& $Mink build hello.mink
.\hello.exe
```

Expected output:

```text
42
```

`build` writes `hello.exe` beside the source. Run it from the same directory
(or give its path). Paths containing spaces work; keep the executable path
quoted when it is stored in a variable or passed to another program.

## 4. Standard-library example

```powershell
@'
mod strings;

fn main() -> Int {
    let s = str_trim(rt_str_concat("  hi  ", ""));
    rt_print_str(s);
    rt_str_free(s);
    return 0;
}
'@ | Set-Content -Encoding ASCII stdlib.mink
& $Mink build stdlib.mink
.\stdlib.exe
```

With the **repository build** (or a corrected future package) the output is:

```text
hi
```

With the **published 1.0.4 artifact** the same program prints `hi` and then
exits 106:

```text
mink: runtime error[E-R06]: memory leak: live allocations remain at exit
```

That is the published-1.0.4 caveat above, not a mistake in the program.

To use the repository instead:

```powershell
git clone https://github.com/p4inz-code/mink.git
Set-Location mink
cargo build --release
.\target\release\mink.exe --version
```

Run the repository compiler from the repository root so it can find
`stdlib/`, or use the package bundle. Do not copy individual modules into a
project unless you intend to shadow the bundled module.

## 5. `.mink` file association

The association is **per-user**, reversible, and does not execute source on
double-click. The implementation is in a repository checkout only:

```text
npm/mink/windows/register-mink-filetype.ps1
```

The published npm 1.0.4 package does **not** contain `windows/`, so this step
is unavailable from that package. From a repository checkout:

```powershell
# Register the .mink extension and icons for the current user.
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\npm\mink\windows\register-mink-filetype.ps1

# Optional: safely open .mink files in an editor instead of executing them.
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\npm\mink\windows\register-mink-filetype.ps1 `
  -Editor "C:\Program Files\Notepad++\notepad++.exe"

# Remove only the MINK-owned association.
powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\npm\mink\windows\register-mink-filetype.ps1 -Uninstall
```

Safety properties verified by
`validation/windows_filetype/verify_filetype_association.ps1` (56/56 checks):

- writes only under `HKCU`; no `HKLM`, no elevation;
- no default `open` verb is installed unless `-Editor` is supplied;
- the editor command is `"<editor>" "%1"`, so the file path is quoted;
- source files are opened in the editor, never compiled or executed;
- `-Uninstall` removes only values/keys the script created and restores a
  recorded prior owner;
- a different existing owner requires `-Force` and is preserved on uninstall.

Do not hand-edit `HKCU\Software\Classes\.mink` or `MINK.SourceFile`. Do not
associate `.mink` with `mink.exe` directly: the compiler is not an editor and
a bare association can execute source unexpectedly.

## 6. Troubleshooting

| Symptom | Meaning / action |
| --- | --- |
| `mink 1.0.2` | Older, unsupported package. Install 1.0.4 from the registry or use the repository build. |
| `str_trim` prints then exit 106 | Published 1.0.4 has the pre-fix `strings.mink`; use the repository build or wait for the corrected release. |
| `E-P01` on `mod environment;` | Published 1.0.4 has the old module; use the repository build. |
| `windows\register-mink-filetype.ps1` not found | You installed from npm 1.0.4; the published package omits `windows/`. Use a repository checkout. |
| `mink` not found | The prefix install does not add itself to `PATH`; invoke the full `bin\mink.exe` path or add its directory deliberately. |
| Build succeeds but run fails with E-R06 | A `Str` returned by a helper was not freed exactly once. See the ownership cheat sheet in the standard-library reference. |

## 7. Version and support boundary

- **Published 1.0.4:** intrinsic-only programs and CLI basics verified; stdlib
  and Windows association are not distribution-equivalent to HEAD.
- **Repository HEAD (`1.0.4` in the manifest):** audited 12/12 application
  verifier, fixed `strings` and `environment` modules, Windows association
  harness 56/56.
- **1.0.2:** unsupported.
- **Linux:** not supported by this package.
