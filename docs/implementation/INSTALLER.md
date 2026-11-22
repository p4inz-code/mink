# The MINK installer

This is how the Windows setup file works, why it is built this way, and how it is tested. For what a user
sees, read [`docs/language/WINDOWS_INSTALLATION.md`](../language/WINDOWS_INSTALLATION.md).

## What gets built

`tools/build_installer.ps1 -Exe <mink.exe>` writes, under `dist\`:

| File | What it is |
| --- | --- |
| `payload\` | `bin\mink.exe`, `stdlib\`, `windows\`, `LICENSE` and `payload-manifest.txt` |
| `payload.zip` | the payload, for the setup wrapper |
| `mink-<version>-portable.zip` | the same files plus a short `README.txt`, with no manifest |
| `installer\mink-Setup-<version>.exe` | the setup program |
| `*.sha256` | the SHA-256 of each of the three files above |

`mink.exe` gets its icon and version information in this step, from a copy of the compiler running
`mink resource`. The setup file is not code-signed yet, so Windows SmartScreen may ask you to confirm it.
Signing will be added later.

## How it runs

`mink-Setup-<version>.exe` is a thin Inno Setup wrapper, built like the Kinoz setup file. It never shows an
Inno wizard. It unpacks the payload into a temporary folder with the `tar.exe` that ships with Windows and
starts `mink.exe installer` from there. Everything else is MINK's own code in `src/installer/`:

| Part | Where | What it does |
| --- | --- | --- |
| Command line | `installer/mod.rs` | the switches, silent runs, the terminal fallback, removal from a temporary copy |
| Install steps | `installer/flow.rs` | the order of an install and of a removal |
| File work | `installer/engine.rs` | the layout, staging, rollback, the launcher, versions |
| Payload check | `installer/manifest.rs` | the manifest and SHA-256 verification |
| Machine work | `installer/host.rs` | PowerShell scripts, Apps & features, elevation check (behind a trait, so tests use a fake) |
| The window | `installer/server.rs` and `assets/installer/` | a local page, shown in an Edge app window |
| Versions | `versions.rs` | the launcher's dispatch, `mink versions`, `mink use` |

The compiler crate forbids `unsafe` code and has no dependencies, and the installer keeps to both. The registry
is changed through `reg.exe` and the existing registration scripts, not through Windows API calls.

## The window

The window is an HTML page served on `127.0.0.1` and opened in Microsoft Edge's `--app` mode, which has no tabs or
address bar. That gives a fully custom look with the MINK artwork, with nothing to install and no GUI library in
the compiler. If Edge cannot be found, setup carries on with questions in the terminal.

Because a local server is a door, it is kept narrow:

- it listens on the loopback address only, on a random port;
- every request must carry the exact `Host` it expects, which defeats DNS rebinding;
- every `/api/` call needs a random token that only the page it served knows;
- the page's Content-Security-Policy allows only its own scripts and styles;
- request bodies are capped at 64 KB, and the choices it accepts are checked (a relative folder, `..`, or an
  editor the installer did not find are refused);
- the window sends a heartbeat. If it goes quiet, setup treats the window as closed and exits with code 8.

## Where things go

```
%LOCALAPPDATA%\Programs\MINK\
  bin\mink.exe             the launcher (a copy of the newest mink.exe); this folder is on the PATH
  windows\                 the file-type kit, shared by every version, so the icon path never changes
  versions\<version>\      bin\, stdlib\, LICENSE and the manifest, one folder per version
  default-version.txt      the version the launcher runs
  mink-install.json        what setup did, so removal can follow it
```

The launcher decides which version to run: the `MINK_VERSION` variable, then a `.mink-version` file in the current
folder or above, then the default. `mink versions` lists them and `mink use <version>` changes the default.

## Safety rules

- **Per user.** Nothing is written outside the profile except the per-user registry. An administrator run is
  refused (exit code 3) unless asked for, because MINK would be installed for that account only.
- **Nothing of yours is touched.** A folder that already holds other files is refused (exit code 6). Removal only deletes
  the paths in the layout above, and leaves the folder if anything else is in it.
- **Verified first.** Every payload file is checked against the manifest before anything is copied. A mismatch stops
  setup with exit code 7 and writes nothing.
- **All or nothing.** A version is staged in `versions\<version>.new` and renamed into place, the kit and the launcher
  the same way, and a failure restores what was there.
- **Launcher swap.** Windows will not overwrite a running program but will rename it, so the old launcher is moved
  aside and the new one renamed in. Terminals that are already open keep working.
- **Removal runs from a copy.** The program that removes MINK would be deleting itself, so `mink installer --uninstall`
  copies itself to `%TEMP%`, exits, and the copy does the work. The install record is removed before the launcher, so
  a silent removal can report success once the record is gone.
- **PATH.** The installer only removes a PATH entry it added itself.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | done |
| 1 | failed |
| 2 | the switches were not understood |
| 3 | running as an administrator |
| 4 | a file is in use (MINK is running) |
| 6 | the folder is not allowed |
| 7 | the installer files are damaged |
| 8 | cancelled, or the window was closed |

A silent run through `mink-Setup-<version>.exe` ends with 0 when it worked. When it does not, the setup file's
own failure code applies, because Inno cannot return the inner program's code.

## How it is tested

- **Unit tests** (`src/installer/`, `src/versions.rs`): the engine and the flow against a temporary folder and a
  fake machine (fresh install, reinstall, upgrade, rollback, refusals, removal, PATH ownership); the manifest; the
  JSON; and the window's server against real local connections (token, host, headers, install, failure, removal,
  idle, hostile input).
- **The validator** (`validation/windows_filetype/verify_filetype_association.ps1`, step 6f): the real compiler
  installs, upgrades and removes MINK in a temporary folder, checks the registry and Apps & features through
  Windows, refuses a folder with other files and a damaged payload, and the machine is restored afterwards.
- **By hand, in a browser:** the install and removal pages were walked through in the built-in browser against the real
  server, and a silent run of the real `mink-Setup-<version>.exe` installed and removed MINK in a scratch folder.
