# M6 — Release Candidate Verification (M6-RC1, 1.0.5)

Operation `M6-RC1`. Machine: Windows x86_64. Date: 2026-11-05.

Scope: build, package and independently verify the release candidate prepared from the
recovered tree. **This candidate is not published.** The registry continues to serve
1.0.4 (verified: `npm view @p4inz-code/mink version` → `1.0.4`).

## 1. Version identity

| Location | Value |
| --- | --- |
| `Cargo.toml` `package.version` | `1.0.5` |
| `Cargo.lock` `name = "mink"` entry | `1.0.5` |
| `npm/mink/package.json` `version` | `1.0.5` |
| `target/release/mink.exe --version` | `mink 1.0.5` |
| installed `node_modules/.../bin/mink.exe --version` | `mink 1.0.5` |

1.0.4 was published on 2026-09-30, so the prepared candidate takes the next patch
version. No tag was created, moved or deleted for 1.0.5.

## 2. Compiler artifact

Built with `cargo build --release` (exit 0, 59 pre-existing warnings, no errors) from the
recovered source.

| Property | Value |
| --- | --- |
| Path | `target/release/mink.exe` |
| Size | 4,028,416 bytes |
| SHA-256 | `c288115d8489571df2384953170e67da9bbcd45c3bedbf569db65714a2479401` |

The copy installed into the package, `npm/mink/bin/mink.exe`, hashes identically. The
previously shipped 1.0.4 package binary (3,899,392 bytes, dated 2026-09-26) predated the
recovered source and was replaced.

## 3. Package manifest

`npm/mink/package.json` `files` changed from
`["bin/mink.exe", "stdlib", "windows"]` to
`["bin/mink.exe", "stdlib", "windows", "LICENSE"]`, and `npm/mink/LICENSE` was added
(byte-identical to the repository root `LICENSE`).

Packed with `npm pack ./npm/mink --pack-destination /tmp/mink_rc`:

| Property | Value |
| --- | --- |
| File | `p4inz-code-mink-1.0.5.tgz` |
| Size | 1,449,290 bytes |
| SHA-256 | `cfafc7a810d38e205475eb24a7009f1dfd0fc05feaad4beecfd24c50efc7df6f` |
| npm shasum | `7d7b8c277be23e2e358a18a5678b8cc7c8ed8e16` |
| Files | 35 |

Independent extraction of the tarball (not the packer's own listing) shows exactly:

- `LICENSE`
- `bin/mink.exe`
- `package.json`
- `stdlib/` — 27 modules
- `windows/` — `README.md`, `mink-app.ico`, `mink-file.ico`, `mink.rc`,
  `register-mink-filetype.ps1`

Checks against the repository:

- every one of the 27 packaged `stdlib/*.mink` files is byte-identical to `stdlib/`
  (`identical=27 differing=0`), including the corrected `environment.mink`
- packaged `bin/mink.exe` SHA-256 equals `target/release/mink.exe`
- packaged `package.json` reports `@p4inz-code/mink` at `1.0.5`

Compared with the published 1.0.4 tarball (28 files, 26 modules), the candidate adds one
stdlib module (`httpserver`) and the five `windows/` assets plus `LICENSE`.

## 4. Guard tests

Two permanent guards cover the packaging claims, both in the repository test suite:

| Guard | Location | Asserts |
| --- | --- | --- |
| `s107_npm_stdlib_bundle_matches_repo_stdlib` | `tests/release.rs` | packaged `stdlib/` is byte-identical to repository `stdlib/` |
| `m6_rc_package_manifest_is_complete` | `tests/release.rs` | manifest ships the binary, all 27 modules, the Windows assets and `LICENSE`; `files` lists `LICENSE`; versions agree |

Both guards were shown to fail when the condition is violated. Removing
`npm/mink/LICENSE` produced `test result: FAILED ... exit code 101`; restoring it returned
the suite to green.

## 5. Clean install verification

Installed from the tarball only, in directories outside the repository, with no repository
files on the path:

| Prefix | Result |
| --- | --- |
| `/tmp/mink_inst/with space/proj1` (space in path) | `added 1 package`, `mink 1.0.5` |
| `/tmp/mink_inst/ünïcode dir/proj2` (non-ASCII path) | `added 1 package`, `mink 1.0.5` |

Installed tree: 35 files, `bin/mink.exe` SHA-256
`c288115d…479401` — identical to the repository release build.

### Standard library resolves from the package, not the repository

`src/driver.rs::resolve_module_path` tries, in order: sibling file, sibling directory
package, `<exe>/../stdlib`, then `<cwd>/stdlib`. For an installed compiler the
`<exe>/../stdlib` candidate exists, so the repository copy is never consulted even when a
command happens to run with the repository as its working directory. Every functional test
below therefore exercised the packaged library.

### Building does not modify the installation

A recursive hash of the installed package tree taken before and after builds is unchanged
(`7034a1d6f00a2c15d9402ac7a02d13705459986376298ebcbaa2801c6d89bbfe`). Generated `.exe`
files and sources land only in the project directory.

## 6. Functional matrix

All runs use the installed compiler from a directory outside the repository.

| Case | Program | Result |
| --- | --- | --- |
| A — intrinsics only | `rt_print_str("intrinsic ok")` | build 0, run 0, stdout exactly `intrinsic ok\r\n` |
| B — strings module | `mod strings;`, `str_trim`, `rt_str_concat` | build 0, run 0, stdout `hi`, `hi`, `abcdef` |
| C — environment module | `mod environment;`, `env_cwd`/`env_has`/`env_set`/`env_get`/`env_remove` on heap strings | build 0, run 0, correct values |
| D — cold-start application | `invsync` via `benchmarks/m6/session180/verify.py --compiler <installed>` | 12/12 cases PASS |
| E — diagnostics | `1 + "a"` | exit 1, `error[E-T02]`, both operand types named, `--> d_typeerr.mink:2:13` |
| F — project isolation | second project in the non-ASCII prefix | independent build and run, `unicode project` |

Case B includes the exact expression that failed historically,
`str_trim(rt_str_concat("  hi  ", ""))`, which now returns `hi` with exit 0.

## 7. Defect found and fixed during verification

Case C initially failed with exit 106 and
`mink: runtime error[E-R06]: memory leak: live allocations remain at exit`.

Isolation showed every wrapper that takes a heap `Str` argument leaked: `env_has`,
`env_set`, `env_get` and `env_remove` all failed, while `env_cwd` (no argument) passed.
Reading `src/backend/emit/runtime.rs`, `emit_env_get`, `emit_env_set`, `emit_env_has` and
`emit_env_remove` copy the argument into a C string, call the Win32 API and release the
C-string copy — but never release the MINK `Str` argument. `stdlib/environment.mink`
documented its parameters as consumed and did not free them, unlike `stdlib/strings.mink`
(`str_trim` → `str_sub`) and `stdlib/filesystem.mink` (explicit `rt_str_free`), which
follow the consuming convention.

The defect was invisible to the existing tests because `tests/environment_lib.rs` passed
string literals, which are not heap objects.

Fix: each wrapper stores the result, frees the consumed parameters with `rt_str_free`, and
returns. The packaged copy in `npm/mink/stdlib/environment.mink` was re-synced, and the
tarball was rebuilt (the SHA-256 in §3 is the corrected artifact).

Permanent regression: `tests/environment_lib.rs::environment_wrappers_release_heap_string_arguments`
builds every argument at run time with `rt_str_concat` and requires exit 0 with empty
stderr. It was verified to fail against the unfixed module
(`exit=106 … E-R06`, `test result: FAILED`) and to pass against the fix.

## 8. Status

Candidate 1.0.5 is built, packaged and verified against its own tarball. It is **not
published**; publishing remains a maintainer action requiring a valid npm token.

Not covered here (see `M6_DEFERRED_SESSIONS.md`): the file-association harness
(`validation/windows_filetype/verify_filetype_association.ps1`), which needs registry
access and must not alter a real user's associations without authorization, and
`npm install -g`, which writes to the machine-wide global prefix.
