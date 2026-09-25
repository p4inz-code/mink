MINK v1.0.2 Release
====================

Release Identity
-----------------
- Tag: v1.0.2
- Branch: main
- Platform: x86-64 Windows PE (Windows x86_64 is the frozen baseline)
- npm: `@p4inz-code/mink@1.0.2` — published and independently verified
- npm tarball: 28 files (`bin/mink.exe`, `package.json`, and all 26 `stdlib/*.mink`
  modules)
- npm tarball sha1 (registry `dist.shasum`): `3f347dd42b6583db8c75f684ec1b7971ba6d2b5b`
- npm tarball integrity: `sha512-v4BS3iilXZY2QrbqgCAy1EtxbSTnKMff+MRUmM4OflvAhjGtYtrZdsmXITCdccaqVTx255foNwocul0ppHBnrA==`
- Compiler binary SHA-256: `6b33b2f6e2c8ac5a2524af3c240683fd78f6e1aa1bbd70170e599fb4a2447076`

Starting commit: `ca503cc` · ending commit: the commit carrying this record.

This is the first release published after the Windows completion program. 86 commits
landed between the `v1.0.1` tag (`3e6a7a6`) and this release, and the published package
is the corrected one rather than a pre-correction build.

What Changed Since v1.0.1
--------------------------
1. **Corrected runtime ownership.** A map replace (`rt_map_set` over an existing key) and
   a set insert (`rt_set_add` of a duplicate element) that discard a stored key or
   element now release the replaced allocation instead of leaking it.

2. **Hashing fixed on owned buffers** (`stdlib/hashing.mink`). Two defects were fixed:
   an `E-R05` workspace overrun for inputs of 256 bytes or more, and an unavoidable
   `E-R06` leak when hashing an owned buffer. Five permanent regressions were added; the
   deterministic regression baseline is now **2957 passing · 0 failed · 3 ignored**
   (2952 before this work).

3. **Four defects found by the public-facing acceptance audit:** `mink init` in a
   directory whose name is not a legal package name; `mink test` and `mink repl <file>`
   module resolution; a malformed missing-module diagnostic; a garbled no-candidate
   resolver message.

4. **Updated bundled compiler.** `npm/mink/bin/mink.exe` is rebuilt from this source and
   reports `mink 1.0.2`. The published tarball's compiler binary and standard library are
   **byte-identical** to the copies in `npm/mink/`.

Additions Since v1.0.1
-----------------------
Delivered by the Windows completion program and present in this release:

- **26 standard-library modules**, including `tls` (TLS 1.2/1.3 client and HTTPS GET via
  Windows Schannel), `sqlite`, `zip`, `zlib`, `re`, `threads`, `tasks`, `csv`, `logging`
  and `assert`. All are bundled beside the compiler, so `mod`/`use` of a standard-library
  module works in any project with no configuration.
- **Package manager** — manifest, resolver, lockfile (`mink.lock`) and isolated
  environments (`mink init` / `add` / `remove` / `install` / `update` / `env`).
- **`mink repl`** — interactive compile-eval session, and **`mink test`** — a test runner
  that discovers and runs `fn test_*` functions.
- Threads and locks, and async tasks with `async fn` / `await`.

Published Package Verification
-------------------------------
- `npm view @p4inz-code/mink version` → `1.0.2`
- `npm view @p4inz-code/mink@1.0.2 version` → `1.0.2`
- `npm view @p4inz-code/mink dist-tags` → `{ latest: '1.0.2' }`
- Fresh install of the published tarball into a clean external directory:
  - `mink --version` / `-v` / `-V` / `version` → `mink 1.0.2`
  - `mink help`, `mink check`, `mink init` → pass
  - `mink run` → `42`; `mink build` → standalone `hello.exe` printing `42`
  - `mink test` → 2 passed / 0 failed, importing the **bundled** `assert` module
  - `mink repl` → interactive session starts and evaluates
  - `mink explain E-T01` → typed diagnostic

Known Limitations (Unchanged)
------------------------------
- x86-64 Windows PE only. Linux and macOS are not supported.
- No Unicode database: `Str` is a byte buffer with a UTF-8 code-point layer, but there are
  no non-ASCII case operations, categories, normalization or collation.
- stdin is read-all only (no line-by-line streaming).
- Array/`Vec` slicing is function-form only (no slice views or step).
- macOS: future / TBD.

Platform Status
----------------
- Windows x86_64: FROZEN (complete, stable; only critical maintenance, security fixes or
  genuine regressions may reopen it).
- Linux: FROZEN. A native ELF backend exists in source, but Linux development is paused
  and Linux is not a supported platform.
- Public npm: `1.0.2` PUBLISHED AND VERIFIED.

Public Positioning and Launch Copy
-----------------------------------

Central positioning statement:

> MINK is a systems-oriented native programming language for Windows x86_64.

GitHub repository description:

> Systems-oriented native programming language for Windows x86_64. Native compilation,
> explicit ownership, standalone executables, and a bundled 26-module standard library.

Project tagline:

> A systems-oriented native programming language for Windows x86_64.

Release announcement (v1.0.2):

> **MINK 1.0.2** is available on npm: `npm install -g @p4inz-code/mink`
>
> MINK is a systems-oriented native programming language for Windows x86_64. It compiles to
> standalone executables that import only Windows system libraries — no CRT DLL, no
> interpreter, no external toolchain.
>
> This release:
>
> - Is the corrected public npm package (`@p4inz-code/mink@1.0.2`, now the registry's
>   `latest`). The published compiler binary and standard library are byte-identical to the
>   copies in this repository.
> - Fixes runtime ownership: a map replace or a set insert that discards an existing key or
>   element now releases the replaced value instead of leaking it.
> - Fixes hashing on owned buffers in `stdlib/hashing.mink`: no more `E-R05` workspace
>   overrun for inputs of 256 bytes or more, and no more `E-R06` leak when hashing an owned
>   buffer.
> - Ships 26 standard-library modules, including `tls`, `sqlite`, `zip`, `zlib`, `re`,
>   `threads` and `tasks`.
> - Keeps the standalone-executable workflow: `mink build` produces a single `.exe` that
>   runs on any Windows 10+ x86_64 machine with no MINK installation.
>
> MINK 1.0.2 is **Windows x86_64 only**. Linux and macOS are not supported; a native Linux
> ELF backend exists in source but Linux is frozen.
>
> Verify: `mink --version` → `mink 1.0.2`

Showcase material: the repository contains no screenshots or terminal captures, and none
were generated. The primary proof asset is the real source and the executable it produces —
[`examples/sysinfo/main.mink`](../../examples/sysinfo/main.mink), built with the public CLI
into a standalone 189,952-byte PE32+ executable whose only import is `kernel32.dll`.
