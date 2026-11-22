# Windows Installation and First Run

**Platform:** Windows x64 only. Linux is not supported by this package.
**Audited:** 2026-11-05 against the published `@p4inz-code/mink@1.0.4` artifact
and the then-current 1.0.4 repository build.
**Current repository build:** **1.0.5**, unpublished. The install examples
below intentionally pin the published `1.0.4` artifact, which is what the
registry still serves; a repository build prints `mink 1.0.5`.
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
older unsupported install and is not interchangeable with 1.0.4. (Session 217
re-ran this prefix install against the registry: 28 files, 26 `stdlib/`
modules, no `windows/`, and `mink --version` printed `mink 1.0.4`.)

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

MINK can give `.mink` files their own identity in Windows: the MINK icon at every size,
right-click entries (Run, Check, Build, Edit), **New > MINK Source File**, preview and search
support, and a **MINK** entry under Settings > Apps > Default apps. It is **per user**,
reversible, and double-clicking a `.mink` file opens an editor; it never runs the file.

`.mink` is the only extension MINK registers. `mink.toml` and `mink.lock` are identified by file
name, so associating them would take over every `.toml` and `.lock` file on the machine.

> The published `1.0.2` and `1.0.4` packages do **not** contain the `windows/` folder or the
> `setup` command. This section describes the 1.0.5 release. Until it is published, the same thing
> works from a repository checkout with the script named at the end of this section.

### Use `mink setup`

```powershell
mink setup            # asks a few questions
mink setup --yes      # takes every default without asking
mink setup --status   # shows what is set up
mink setup --remove   # takes it all away again
```

The questions use numbered lists (`1 2 3`, `1-3`, `all`, `none`; Enter keeps the marked ones) and
`y`/`n` for yes or no. If you install with `npm install -g`, an install step runs
`mink setup --auto` for you. It asks nothing, keeps the choices of an earlier setup and only
renews the stored paths. It never takes `.mink` away from another program. Set
`MINK_SKIP_SETUP=1` before installing to skip it. `mink setup --help` lists every option.

Things worth knowing:

- Setup writes under `HKCU` only. It needs no administrator rights and leaves `HKLM` alone.
- If another program already owns `.mink`, setup says so and stops. `--force` takes it over and
  `--remove` gives it back. That program's own entries are left alone either way.
- With the right-click entries on, double-click opens Notepad, or the editor you choose. Turn the
  entries off and Windows asks which app to use the first time. If you have already told Windows
  which app to use, that choice wins.
- Setup can add the folder holding `mink.exe` to your own PATH. Open a new terminal afterwards.
- The icon and the entries store absolute paths. After you move or reinstall MINK, run
  `mink setup --auto` (or `mink setup`) again.

### What it runs

`mink setup` runs `windows\register-mink-filetype.ps1` from the MINK folder, which you can also run
yourself. Its switches are documented in `npm/mink/windows/README.md`.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\npm\mink\windows\register-mink-filetype.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\npm\mink\windows\register-mink-filetype.ps1 -Uninstall
```

The behaviour is checked by `validation/windows_filetype/verify_filetype_association.ps1`
(191 checks): it builds a throwaway install, registers it, asks Windows itself what it sees,
opens a real file by double-click, runs a registered right-click command with the real compiler
and puts your registry back exactly as it found it. Do not hand-edit `HKCU\Software\Classes\.mink`
or `MINK.SourceFile`, and do not associate `.mink` with `mink.exe` directly: the compiler is not an
editor, and a bare association can run source unexpectedly.

### Updating

`mink upgrade --check` tells you whether a newer version exists. MINK contacts the network only when
you run it, and only `api.github.com` and `github.com`. If you installed with npm it prints
`npm install -g @p4inz-code/mink@latest` for you to run in a new terminal, because Windows will not
let npm replace the program that is running it. An installer install downloads the new installer,
checks its SHA-256 against the checksum published beside it, and only then starts it.

## 6. Troubleshooting

| Symptom | Meaning / action |
| --- | --- |
| `mink 1.0.2` | Older, unsupported package. Install 1.0.4 from the registry or use the repository build. |
| `str_trim` prints then exit 106 | Published 1.0.4 has the pre-fix `strings.mink`; use the repository build or wait for the corrected release. |
| `E-P01` on `mod environment;` | Published 1.0.4 has the old module; use the repository build. |
| `mink setup` says it cannot find `register-mink-filetype.ps1` | The package you installed has no `windows` folder (1.0.2 and 1.0.4 omit it). Install 1.0.5 or later, or use a repository checkout. |
| `.mink` files show a generic icon | No association exists, or its `DefaultIcon` points at a package directory that has moved. Run `mink setup --auto` (or re-run `register-mink-filetype.ps1`) from the current checkout or package, then refresh the icon cache (see `docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md` §5, §5.2). |
| `mink` not found | The prefix install does not add itself to `PATH`; invoke the full `bin\mink.exe` path or add its directory deliberately. |
| Build succeeds but run fails with E-R06 | A `Str` returned by a helper was not freed exactly once. See the ownership cheat sheet in the standard-library reference. |

## 7. Version and support boundary

- **Published 1.0.4:** intrinsic-only programs and CLI basics verified; stdlib
  and Windows association are not distribution-equivalent to HEAD.
- **Repository HEAD (`1.0.5` in the manifest, unpublished):** the 12/12
  application verifier, fixed `strings` and `environment` modules and the
  Windows association harness 88/88 were audited at the 1.0.4 baseline and
  still describe the source; the current build reports `mink 1.0.5` and the
  Session 213 release verification passes.
- **1.0.2:** unsupported.
- **Linux:** not supported by this package.
