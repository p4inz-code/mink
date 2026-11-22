# Changelog

All notable changes to MINK. The newest release is first.

## 1.0.6 (next)

### Fixed
- Calls into Windows with no arguments (`GetTickCount64`, `GetCurrentProcessId`, `GetLastError` and others) through `rt_sys_call` always returned 0. They now make the call. A function that could not be found returns 0 instead of crashing.

- SQL `UPDATE` and `DELETE` no longer change every row when they meet text they do not understand (`SET n = n + 1`, a misspelt `WHERE`). They now return an error and leave the data alone. A closing `;` is no longer stored inside a value.

- `base64_decode_alloc` and `base64_url_decode_alloc` decoded lowercase letters as uppercase ones, so most decoded text was wrong. They now decode correctly.

### Added
- Four examples: `json_config`, `web_loopback`, `winapi_info`, `hash_and_compress`, and a field check of what MINK can build in `docs/language/WHAT_YOU_CAN_BUILD.md`.

## 1.0.5

### Added
- **Windows file association.** `.mink` files get an icon and a name, right-click Run, Check, Build and
  Edit, a New > MINK Source File entry, a preview pane and search support, and a Settings > Default apps
  entry. Double-click opens an editor and never runs a file. Registration only touches your own account.
- **Icons and version information in built programs.** `mink build --icon file.ico` and
  `--no-icon`. `mink resource` adds them to an existing program.
- **`mink setup`.** A step-by-step setup in the terminal with numbered choices and y/n questions.
  `--yes` takes the defaults, `--status` shows the state, `--remove` undoes it. npm installs run it
  automatically.
- **A setup file and a portable zip.** `mink-Setup-<version>.exe` installs for your account without
  administrator rights, shows the license and credit, and removes cleanly from Apps & features. The
  setup window has larger text, light and dark colours, back navigation, About and Help pages.
- **Side-by-side versions.** `mink versions` lists them and `mink use <version>` picks one.
- **`mink upgrade`.** Checks GitHub only when you run it and verifies the download's SHA-256.
- **`mink fmt`.** One layout, no settings, with a check that its output matches the input token for
  token.
- **`mink lsp`.** A language server: errors as you type, completion, hover, go to definition, references,
  rename, outline, signature help and formatting.
- **A VS Code extension** in `editors/vscode`: colours, snippets, Run / Check / Build commands and a
  language-server client.
- **`module::name` calls** for functions in loaded modules.
- **A NOTICE file** that carries the creator credit with every copy.

### Fixed
- A local module with the same name as a standard-library module silently replaced it and caused
  confusing errors. The compiler now says which one was used and which one was ignored.

### Known limits
See [`docs/KNOWN_LIMITATIONS.md`](docs/KNOWN_LIMITATIONS.md).

## 1.0.4
The last release on npm before this sprint.
