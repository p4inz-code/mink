# Windows integration for MINK

Shipped inside the npm package (`@p4inz-code/mink`), next to `..\bin\mink.exe`.

| File | Purpose |
| --- | --- |
| `register-mink-filetype.ps1` | Registers / removes the per-user `.mink` file association |
| `mink-file.ico` | Icon for `.mink` source files (7 sizes: 16–256) |
| `mink-app.ico` | Application icon for `mink.exe` (7 sizes: 16–256) |
| `mink.rc` | Reference resource script for embedding the icons in `mink.exe` |

## Register the `.mink` file type

This directory only exists in packages that ship `windows/` — the current candidate, not the
published `1.0.4` (28 files, no `windows/`). Run the script from wherever this kit actually
is:

```powershell
$package = Join-Path (npm root -g) '@p4inz-code\mink'   # once such a package is installed
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1"
# or, from a repository checkout:
powershell -ExecutionPolicy Bypass -File npm\mink\windows\register-mink-filetype.ps1
```

Per-user only (`HKCU`), no administrator rights, no `HKLM`. `.mink` files become a file
type named **MINK Source File** with the MINK source-file icon, distinguished by content
type `text/x-mink`.

Double-clicking a `.mink` file does **not** compile or run it. The registration adds no
`open` command, so Windows asks which application to use the first time and remembers
the answer; MINK never executes source. To bind one editor explicitly:

```powershell
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1" `
    -Editor "C:\Program Files\Microsoft VS Code\Code.exe"
```

Refuses to replace a `.mink` association owned by another application unless `-Force` is
given; `-Force` records the previous owner and `-Uninstall` restores it.

## Remove it

```powershell
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1" -Uninstall
```

Only MINK-owned values are removed, including the `MINK.SourceFile` ProgID and the
`OpenWithProgids` entry this script added. Another application's `.mink` association and
the user's own "Open with" choice are left untouched.

Full design, validation and limitations:
[`docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md`](../../../docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md).

## Notes

- `mink.exe` in this package has no embedded icon resource; the `.mink` file type is
  what carries MINK's identity in Explorer. `mink-app.ico` is provided for shortcuts,
  installers and embedding (`mink.rc`).
- `.mink` is the only extension MINK registers. `mink.toml` and `mink.lock` are
  recognised by file name, not by extension, so associating them would take over every
  `.toml` and `.lock` file on the machine. Do not add them here.
- The script resolves the icon from its own directory, so it works from any working
  directory, with any npm prefix, and from a path containing spaces or non-ASCII
  characters. It never assumes a drive letter or a development directory.
- The resolved icon must be a real ICO (header and frame count checked). If it is missing,
  empty or malformed, registration exits 1 with the paths it searched and changes nothing.
- The registered icon path is absolute and captured when the script runs. After moving,
  upgrading or reinstalling the package, run it again; nothing repairs the path itself.
- `.mink` files are not registered with an `open` command, so nothing in this kit can
  compile or run source on double-click.

Validation of this directory is permanent:
[`validation/windows_filetype/verify_filetype_association.ps1`](../../../validation/windows_filetype/verify_filetype_association.ps1)
(88 checks) checks the package layout, the icon resolution and the registry effects.
