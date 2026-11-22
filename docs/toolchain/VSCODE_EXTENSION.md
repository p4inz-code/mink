# MINK for VS Code

The extension lives in `editors/vscode`. It adds colours, snippets and commands, and starts the
MINK language server (`mink lsp`) for everything else.

## Install from the file

```
cd editors/vscode
npm install --omit=dev
npx @vscode/vsce package
code --install-extension mink-lang-1.0.5.vsix
```

MINK itself must be installed and on your PATH, or `mink.path` must point to `mink.exe`.

## What you get

- **Colours** for keywords, types, strings, numbers, comments, doc comments and `rt_` runtime functions.
- **Snippets**: `fn`, `main`, `if`, `ife`, `while`, `for`, `match`, `struct`, `enum`, `let`, `letm`, `print`, `mod`.
- **Commands**: MINK: Run this file (Ctrl+F5), Check this file, Build this file, Restart the language
  server, Show the language server log. Run, Check and Build save the file and use a terminal called MINK.
- **Language server**: errors as you type, completion, hover, go to definition, find references, rename,
  outline, signature help and formatting. See [LANGUAGE_SERVER.md](LANGUAGE_SERVER.md).
- **Settings**: `mink.path`, `mink.formatOnSave` (off by default), `mink.trace.server`.

## Safety

In a folder you have not trusted, the extension does not start `mink` and will not run files. `mink.path`
cannot be set by a workspace file in an untrusted folder.

## Checks

`npm test` in `editors/vscode` checks that every JSON file parses, every command has a handler, every
grammar expression compiles, and every language keyword is highlighted. The language server itself is
tested in the compiler's test suite, including a run over real stdin and stdout.

## Not done yet

- Publishing to the Visual Studio Marketplace. The package builds; it is waiting for your go-ahead.
- Opening the extension in a real VS Code window was not part of this check.
