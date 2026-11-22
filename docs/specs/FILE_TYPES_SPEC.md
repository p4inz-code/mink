# MINK file types: specification

**Status:** Specification only. Nothing here is implemented in 1.0.5. Extensions are proposals and
the owner decides the final names.

MINK has one file type today: the source file, `.mink`. Three more are planned.

| Kind | Proposed extension | What it holds | Who writes it |
|---|---|---|---|
| Source file | `.mink` | Program text | You |
| Interface file | `.minki` | The public names, types and doc comments of a module, with no bodies | `mink build` |
| Compiled module | `.minkm` | A checked, ready-to-link module, so unchanged modules are not compiled again | `mink build` |
| Package archive | `.minkpkg` | A package: `mink.toml`, sources, interface files and a checksum list, in one zip | `mink pack` |

## Source file (exists)

Plain UTF-8 text. Icon, right-click verbs, New menu and preview are registered by Setup.

## Interface file

- Text, human readable, one declaration per line, so it diffs well and editors can index it.
- Starts with a header: `mink-interface 1`, the module name and the compiler version that wrote it.
- Lets the language server and `mink doc` read a package without its source.
- Never executable. Windows registers it with the MINK icon and the Edit verb only.

## Compiled module

- Binary, with a header of magic bytes, a format version, the compiler version and a SHA-256 of the
  source it came from.
- The compiler uses it only when the stored hash matches the source and the compiler version is
  identical. Otherwise it recompiles. A stale file can never change a program's meaning.
- Cache first: modules are looked up in `.mink/cache` before they are compiled, as agreed in planning.
- Double-click does nothing. There is no Run verb, since it is not a program.

## Package archive

- A zip with fixed layout: `mink.toml`, `src/`, `interface/`, `CHECKSUMS.sha256`, `NOTICE`, `LICENSE`.
- `mink install <file.minkpkg>` verifies every checksum before it unpacks anything, and refuses an
  archive with paths that escape the package folder.
- The archive must carry the package's `NOTICE` and `LICENSE`, so credit travels with the code.
- Double-click opens an "Install this package?" prompt. It never installs silently.

## Windows registration (when implemented)

Each type gets its own ProgID under `HKCU\Software\Classes`, its own icon from the MINK icon set and
only the verbs listed above. Setup registers them the same way it registers `.mink`, keeps existing
choices and never takes an extension another program already owns.

## Open questions

- Final extension names.
- Whether interface files are checked into version control or always generated.
