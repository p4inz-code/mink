# M6 Session 175 — installation and environment validation

**Date:** 2026-11-04
**Status:** **COMPLETE**
**Starting HEAD:** `4504073` (`docs: define M6 external usability experiment`)
**Scope:** follow the documented npm installation path in an isolated prefix,
verify compilation/execution, and record failure diagnostics. No documentation
or source fixes in this session.

## 1. Environment

| Item | Observed |
| --- | --- |
| OS | Windows (`win32`), x86_64 |
| Shell | Git Bash |
| Node.js | `v24.14.0` |
| npm | `11.9.0` |
| Install prefix | `tmp/m6_install_175` (project-local; no global install) |
| Registry package | `@p4inz-code/mink@1.0.4` (`npm view` -> `1.0.4`) |
| Installed compiler | `mink 1.0.4` |
| Package contents | `bin/`, `package.json`, `stdlib/` (26 modules) |
| Compiler binary | 3,899,392 bytes |

No global npm package, PATH entry, registry setting or unrelated toolchain was
modified.

## 2. Installation steps

```bash
npm view @p4inz-code/mink version
npm install --prefix tmp/m6_install_175 @p4inz-code/mink --no-audit --no-fund
tmp/m6_install_175/node_modules/@p4inz-code/mink/bin/mink.exe --version
PATH="$PWD/tmp/m6_install_175/node_modules/.bin:$PATH" mink --version
```

Observed:

```text
1.0.4
added 1 package in 316ms
mink 1.0.4
mink 1.0.4
```

The npm `bin` shim resolves correctly when its directory is on `PATH`.

## 3. First program

`tmp/m6_project_175/hello.mink`:

```mink
fn main() {
    let mut total = 0;
    for i in 1..=4 {
        total = total + i;
    }
    rt_print_int(total);
    return 0;
}
```

Built and run with the installed compiler, from the project directory (not the
repository root):

```text
mink: build: 'hello.mink' -> '...\hello.exe' (target: x86_64-windows-pe, 1 function(s), 0 binding(s))
BUILD_EXIT=0
10
RUN_EXIT=0
```

A second execution produced `10` again (exit 0).

## 4. Standard-library resolution

The packaged `stdlib/` resolves when the working directory has no local
`stdlib` directory:

```mink
mod filesystem;
fn main() -> Int {
    let rc = fs_write("m6_note.txt", "hello-m6");
    if rc < 0 { rt_print_str("write-failed"); return 1; }
    let s = fs_read("m6_note.txt");
    rt_print_str(s);
    rt_print_int(10);
    rt_str_free(s);
    return 0;
}
```

```text
BUILD_EXIT=0
hello-m6
10
RUN_EXIT=0
```

The written file was 8 bytes containing `hello-m6`.

`mink check hello.mink --json` also works:

```json
{
  "success": true,
  "files_checked": 1,
  "token_count": 34,
  "errors": [],
  "warnings": []
}
```

## 5. Failure scenarios

Captured without piping, so the exit status is the compiler's.

| Scenario | Command | Exit | Diagnostic |
| --- | --- | ---: | --- |
| Missing source | `mink build missing.mink` | 1 | `mink: error: failed to read 'missing.mink'` |
| Directory as source | `mink build .` | 1 | `mink: error: '.' is a directory, not a MINK source file` |
| Unknown target | `mink build hello.mink --target linux-x86_64` | 1 | `unknown target 'linux-x86_64' (supported targets: x86_64-windows-pe (implemented), x86_64-linux-elf (implemented), aarch64-linux-elf (not yet implemented))` |
| Compiler absent from PATH | `PATH=/c/Windows/System32 mink --version` | 127 | `mink: command not found` |
| Standard library missing | copied package, `stdlib` renamed, `mod strings;` | 1 | `mink: error[E-S01]: module file 'strings.mink' not found` at file:line |

The missing-module diagnostic names the missing file and the import location.
The unknown-target diagnostic exposes Linux targets even though Linux is
frozen; this is accurate about the compiler's target table but potentially
confusing to a new Windows developer.

## 6. Distribution audit

| Requirement | Evidence |
| --- | --- |
| Compiler present in package | `bin/mink.exe`, 3,899,392 bytes, reports 1.0.4 |
| Standard library bundled | 26 modules beside the binary |
| Node/npm requirement | `engines.node >= 18`; tested with Node 24 |
| Platform restriction | `os: win32`, `cpu: x64` |
| Runtime dependencies | none beyond Windows; nothing else installed by npm |
| PATH configuration | npm `bin` shim works once its directory is on PATH |
| Second invocation | Same output on rebuild/run |
| Update/uninstall | Not documented in README |
| Clean-machine install | Not tested (same machine, project prefix) |

## 7. Findings

| ID | Finding | Severity |
| --- | --- | --- |
| I1 | README says the registry serves 1.0.3 and 1.0.4 publishing is pending; the registry actually resolves `1.0.4`, and a fresh install reports 1.0.4. The install instructions are accurate, but the surrounding release note is stale. | High (documentation accuracy) |
| I2 | Installation is documented only as a global `npm install -g`; there is no project-local or isolated-prefix guidance. A cautious developer has to infer it. | Medium |
| I3 | No update, downgrade or uninstall instructions exist. | Low |
| I4 | Missing source, directory, unknown target and missing stdlib all fail with clear diagnostics and correct nonzero exits. | Positive |
| I5 | Unknown-target output lists two Linux targets as "implemented" while Linux is frozen and unsupported; the message is accurate but easy to misread. | Low |
| I6 | `mink check --json` provides machine-readable success/error data suitable for CI. | Positive |
| I7 | Build success reports an absolute output path; helpful locally, noisy in CI logs. | Informational |

## 8. Undocumented requirements observed

1. A developer must know to put the npm `bin` directory on `PATH` if not using
   a global install; the README only covers the global case.
2. The packaged `stdlib/` must remain beside the compiler binary; moving
   `mink.exe` alone produces `E-S01`. The README says the package bundles the
   library but does not state this layout requirement as an installation rule.
3. npm's `os`/`cpu` fields reject unsupported hosts, but the resulting npm
   error is not documented.

## 9. Limitations

- One machine and one Node/npm version; no clean VM or fresh Windows user.
- No global installation was attempted (deliberately, to avoid changing the
  user's environment). The README's documented command is global; the prefix
  form is equivalent npm behavior but not documented by MINK.
- No network-failure or offline-install test.
- No update/uninstall test.

## 10. Result

**COMPLETE.** Installation, PATH resolution, first compilation, execution,
second run, packaged-stdlib import and five failure scenarios were reproduced.
The evidence list contains every undocumented requirement observed. No fixes
were made in this session; documentation corrections are for Session 177.

## 11. Next session

Session 176 — first program and basic language learning: variables, functions,
conditionals, loops, output, edit/rebuild, plus a documented-surface audit.
