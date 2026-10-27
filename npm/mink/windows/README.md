# Windows integration for MINK

Shipped inside the npm package (`@p4inz-code/mink`), next to `..\bin\mink.exe`.

| File | Purpose |
| --- | --- |
| `register-mink-filetype.ps1` | Registers / removes the per-user `.mink` file association |
| `mink-file.ico` | Icon for `.mink` source files (7 sizes: 16–256) |
| `mink-app.ico` | Application icon for `mink.exe` (7 sizes: 16–256) |
| `mink.rc` | Reference resource script for embedding the icons in `mink.exe` |

## Register the `.mink` file type

```powershell
$package = Join-Path (npm root -g) '@p4inz-code\mink'
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1"
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
- The script resolves the icon from its own directory, so it works from any working
  directory, with any npm prefix, and from a path containing spaces or non-ASCII
  characters.
