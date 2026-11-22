# Windows integration for MINK

Shipped inside the npm package (`@p4inz-code/mink`), next to `..\bin\mink.exe`.

| File | What it is |
| --- | --- |
| `register-mink-filetype.ps1` | Adds or removes the per-user `.mink` file type and its extras |
| `mink-file.ico` | Icon for `.mink` source files (7 sizes, 16 to 256 px) |
| `mink-app.ico` | Application icon for MINK, used on the right-click entries and for shortcuts |
| `template.mink` | The starter program that "New > MINK Source File" copies |
| `mink.rc` | Reference resource script, for anyone who prefers a resource compiler. `mink resource` does the same without one |

## Set it up

The easiest way is `mink setup`, which asks a few questions and runs this script for you.
You can also run the script yourself from wherever this folder is:

```powershell
$package = Join-Path (npm root -g) '@p4inz-code\mink'
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1"
# or, from a repository checkout:
powershell -ExecutionPolicy Bypass -File npm\mink\windows\register-mink-filetype.ps1
```

It writes to your own user registry only (`HKCU`). It never asks for administrator rights and
never touches `HKLM`.

## What you get

| Where | What happens |
| --- | --- |
| Explorer | `.mink` files are called **MINK Source File** and show the MINK icon at every size |
| Double-click | Opens the file in an editor. It never compiles or runs it |
| Right-click | **Run with MINK**, **Check with MINK**, **Build executable**, **Edit** |
| New menu | **New > MINK Source File** creates a file from `template.mink` |
| Preview pane and search | `.mink` is treated as plain text, the same way `.txt` is |
| Settings > Apps > Default apps | MINK is listed, and `.mink` points at it |

The three MINK entries run `mink.exe shell <command> "<file>"`, which keeps the console open
until you press Enter, so you can read the output. Nothing runs unless you pick the entry:
**Run** starts the program, **Build** writes an `.exe` next to the source, and **Check** only
analyses it. The three entries are single-selection, so selecting several files and clicking
cannot start many programs at once.

### Double-click and editors

Double-click uses the **Edit** entry, which opens Notepad. To use another editor, name it:

```powershell
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1" `
    -Editor "C:\Program Files\Microsoft VS Code\Code.exe"
```

With an editor set, the plain **Open** entry (relabelled **Edit**) opens the file in it. If you
have already told Windows which app to use for `.mink`, that choice wins.

### Options

| Option | Effect |
| --- | --- |
| `-Verbs run,check,build,edit` | Pick which right-click entries to add. `edit` is always present when any entry is, because double-click needs a safe default |
| `-NoContextMenu` | No right-click entries |
| `-NoNewMenu` | Leave out New > MINK Source File |
| `-NoPreview` | Leave out the preview pane and search settings |
| `-NoDefaultApps` | Leave MINK out of the Default apps list |
| `-Editor <path>` | Open files in this editor |
| `-Force` | Take over `.mink` from another application (see below) |
| `-Quiet` | Print only errors |
| `-Uninstall` | Remove everything this script added |

If another program already owns `.mink`, the script stops and tells you. `-Force` takes it over,
remembers who had it, and `-Uninstall` gives it back. Other programs' own entries, such as a New
menu entry or a preview handler they registered, are left alone.

## Remove it

```powershell
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1" -Uninstall
```

Only what MINK added is removed. Your own "Open with" choice for `.mink` belongs to you and is
left as it is.

## Notes

- `mink.exe` and every program `mink build` writes carry the MINK icon and version information.
  `mink build --icon app.ico` picks another icon, `--no-icon` drops it, and `mink resource`
  adds both to any Windows executable that has none. `mink-app.ico` is also used for shortcuts
  and installers.
- `.mink` is the only extension MINK registers. `mink.toml` and `mink.lock` are recognised by name,
  not by extension, so associating them would take over every `.toml` and `.lock` file on the
  machine. Do not add them here.
- The script finds the icons and the template next to itself, so it works from any folder, any npm
  prefix, and paths with spaces or non-ASCII characters. It never assumes a drive letter.
- The icon must be a real ICO. If it is missing or damaged, the script stops and changes nothing.
- The registered paths are absolute and captured when the script runs. After you move or reinstall
  the package, run the script again (or `mink setup`) so they point at the new place.

Design, validation and limits:
[`docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md`](../../../docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md).

The permanent check for this folder is
[`validation/windows_filetype/verify_filetype_association.ps1`](../../../validation/windows_filetype/verify_filetype_association.ps1).
It builds a throwaway install, registers it, asks Windows itself what it sees, and puts your
registry back exactly as it found it.
