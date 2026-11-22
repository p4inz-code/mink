# Known limitations and bug register

What does not work yet, what behaves in a surprising way, and what looks like a bug but is the test
machine. Each line says whether it is fixed, planned or accepted.

## Language and compiler

| ID | What | Status |
|---|---|---|
| L1 | All modules are flattened into one namespace. Two modules that declare the same name collide (`E-S02`). | Accepted for 1.0.5. Real namespaces are planned. |
| L2 | `pub` is parsed but not enforced. Every name in a loaded module is visible. | Accepted for 1.0.5. Planned with namespaces. |
| L3 | A local `foo.mink` next to your program used to silently replace the standard-library `foo` module and break the program in confusing ways. | Fixed in 1.0.5. The compiler now says which module won and which one lost. |
| L4 | `module::name` calls work, but the module must be declared with `mod module;` in the same file. | Accepted. |
| L5 | No debug information in built programs, so a debugger shows no source lines. | Planned. Spec: [`specs/DOC_AND_DEBUG_SPEC.md`](specs/DOC_AND_DEBUG_SPEC.md). |
| L6 | No package registry. Packages install from local folders. | Planned. Spec: [`specs/PACKAGE_REGISTRY_SPEC.md`](specs/PACKAGE_REGISTRY_SPEC.md). |
| L7 | No `mink doc`. | Planned. Spec: [`specs/DOC_AND_DEBUG_SPEC.md`](specs/DOC_AND_DEBUG_SPEC.md). |

## Platform and distribution

| ID | What | Status |
|---|---|---|
| P1 | Windows 10 and newer only. Linux is frozen. | By design. |
| P2 | The installer and `mink.exe` are unsigned for now. Windows SmartScreen may warn on first run. Signing is coming. | Planned. |
| P3 | Setup uses a local window in Microsoft Edge. If Edge is missing it prints a local address to open instead. | Accepted. |

## Editor tooling

| ID | What | Status |
|---|---|---|
| E1 | The language server checks the whole program from the open file on every change. Very large programs may feel slow. | Accepted. |
| E2 | No semantic highlighting, code actions or workspace-wide symbol search. | Planned. |
| E3 | The VS Code extension is packaged but not published to the Marketplace. | Waiting on the owner. |
| E4 | Only VS Code has an extension. Other editors can use `mink lsp` directly. | Other editors come later. |

## The test machine, not MINK

These show up on the author's development PC and are not product bugs.

| ID | What | Workaround |
|---|---|---|
| T1 | `rustc` sometimes crashes with `STATUS_ACCESS_VIOLATION` or `STATUS_ILLEGAL_INSTRUCTION` during a build. | Run the build again with fewer jobs. |
| T2 | The environment variable `NoDefaultCurrentDirectoryInExePath` makes a few tests that start `mink` by name fail. | Run the tests with that variable unset. |
| T3 | A few tests call `file`, which is not on a plain Windows PATH. | Install Git for Windows' tools, or skip those tests. |
| T4 | Git Bash collapses backslashes inside here-documents. | Write files with an editor tool or PowerShell. |

## Rules learned the hard way

- A command that rewrites files must never default to the current folder. `mink fmt` needs an explicit
  path (`mink fmt .`) for that reason.
- After a full test run, check `git status` for files you did not mean to change.
