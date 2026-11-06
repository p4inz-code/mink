# M6-RC1 — Independent Re-verification

Operation `M6-RC1-RV`. Windows x86_64, 2026-11-06.
Scope: re-verify the already-committed M6-RC1 release candidate and the
pushed repository state from scratch, without relying on the previous
operation's own records for any result. This operation changed no tracked
file except the two records named in §9.

## 1. Starting state

| Item | Value |
| --- | --- |
| Branch | `main` |
| Starting HEAD | `c76f33c6a3eff91f2346e438345fc1e293688581` |
| `origin/main` | `c76f33c6a3eff91f2346e438345fc1e293688581` |
| Ahead/behind | `0  0` |
| Worktree | 4 untracked, 0 modified, 0 staged |
| Tags | `v1.0.0` … `v1.0.4` (no `v1.0.5`) |

The four untracked entries are the same unrelated files M6-RC1 preserved:
`MINK UI SETS OFFICIAL.zip`, `freebuff-chat-2026-10-01T19-08-00.600Z.md`,
`nl1.mink`, `nl1.exe`. They were left untouched and uncommitted again.

The historical baseline in the earlier reports (`f258210`) is 14 commits
behind the actual current HEAD. Nothing was reset towards it.

## 2. Version identity

| Location | Value |
| --- | --- |
| `Cargo.toml` | `1.0.5` |
| `Cargo.lock` (`name = "mink"`) | `1.0.5` |
| `npm/mink/package.json` | `1.0.5` |
| `target/release/mink.exe --version` | `mink 1.0.5` |
| `npm/mink/bin/mink.exe --version` | `mink 1.0.5` |
| installed `node_modules/.../bin/mink.exe --version` | `mink 1.0.5` |

## 3. Distribution content

| Property | Value |
| --- | --- |
| `stdlib/` modules | 27 |
| `npm/mink/stdlib/` modules | 27 |
| `diff -rq stdlib/ npm/mink/stdlib/` | no differences |
| `npm/mink/windows/` assets | 5 (`README.md`, `mink-app.ico`, `mink-file.ico`, `mink.rc`, `register-mink-filetype.ps1`) |
| `LICENSE` vs `npm/mink/LICENSE` | byte-identical, SHA-256 `81b7a27798d20aa586689b5b6e39b0737100bb9acb49c965ddd194d591f4647e` |
| Release binary | 4,028,416 bytes |
| Release binary SHA-256 | `c288115d8489571df2384953170e67da9bbcd45c3bedbf569db65714a2479401` |
| Packaged binary SHA-256 | same as release binary |

Tarball `p4inz-code-mink-1.0.5.tgz`, rebuilt and re-inspected independently
by extracting it rather than trusting the packer's listing:

| Property | Value |
| --- | --- |
| Size | 1,449,290 bytes |
| SHA-256 | `cfafc7a810d38e205475eb24a7009f1dfd0fc05feaad4beecfd24c50efc7df6f` |
| Files | 35 |
| `stdlib/*.mink` in tarball | 27 |
| `windows/` in tarball | 5 |
| Top level | `LICENSE`, `bin/mink.exe`, `package.json` |
| Extracted `package.json` version | `1.0.5` |
| Extracted `bin/mink.exe` SHA-256 | equals the release binary |

## 4. Full native regression

```bash
export PATH="/tmp/pyshim:$PATH"   # real Python 3.11, see M6_FULL_REGRESSION_AND_RELEASE_AUDIT.md §4
cargo test
```

| Measurement | Result |
| --- | --- |
| Process exit | 0 |
| Suites reporting `test result: ok.` | 78 |
| Suites reporting `test result: FAILED` | 0 |
| Passed | 3,176 |
| Failed | 0 |
| Ignored | 1 |

Counts obtained by summing the per-suite result lines out of the captured
log, not copied from the earlier record. The figure matches it exactly.

Focused suites, run separately:

| Suite | Result |
| --- | --- |
| `tests/release.rs` | 69 passed, 0 failed |
| `tests/environment_lib.rs` | 3 passed, 0 failed |

## 5. Guard sensitivity (deliberate-mismatch test)

The stdlib bundle guard was proven non-vacuous rather than assumed:

1. Baseline `npm/mink/stdlib/assert.mink` SHA-256
   `48e22d89b0fb332e1aad385a50da62c8f300de9a002300f04a6989d2ecd90025`.
2. Appended a comment line to that file.
3. `cargo test --test release s107_npm_stdlib_bundle_matches_repo_stdlib`
   → `test result: FAILED. 0 passed; 1 failed`, panicking with
   `assert.mink differs between stdlib/ and npm/mink/stdlib/ — re-copy the module`.
4. Restored the file from the committed blob (`git show HEAD:...`) and
   confirmed the SHA-256 returned to the baseline value.
5. Re-ran the guard → `1 passed; 0 failed`. Worktree returned to the same
   four untracked entries.

## 6. Clean installation and functional matrix

Installed from the local tarball only, into two isolated prefixes outside
the repository, with no repository file on the search path.

| Prefix | Install | `mink --version` |
| --- | --- | --- |
| `/tmp/mink_verify_rc/spaced proj` (space) | `added 1 package` | `mink 1.0.5` |
| `/tmp/mink_verify_rc/ünïcode proj2` (non-ASCII) | `added 1 package` | `mink 1.0.5` |

Installed package tree: 35 files in both prefixes, matching the tarball.
The only `.exe` inside the installed package is `bin/mink.exe`; build
products land in the project directory, not in the package.

| Case | Result |
| --- | --- |
| A — intrinsics only | build 0, run 0, stdout `intrinsic ok` |
| B — `mod strings;`, `str_trim(rt_str_concat("  hi  ", ""))`, concat | build 0, run 0, stdout `hi`, `abcdef` |
| C — `mod environment;` with heap-string arguments (`env_has`, `env_set`, `env_get`, `env_remove`, `env_cwd`) | build 0, run 0, `val-1` and the project cwd, empty stderr |
| D — `invsync` via `benchmarks/m6/session180/verify.py` | **12/12 PASS** against both the repository compiler and the installed compiler |
| E — diagnostics (`1 + "a"`) | exit 1, `mink: error[E-T02]: cannot apply operator '+' to types 'Int' and 'Str'`, `--> t_typeerr.mink:2:13` |
| F — second project in the non-ASCII prefix | build 0, run 0, stdout `unicode project` |

Case B note: the first version of the probe exited 106 with
`E-R06 memory leak`, because that probe itself never freed the two heap
strings it created (`str_trim` consumes its argument and returns a new
heap string; the concatenation result was also dropped unfreed). Rewriting
the probe to release both returned values gives exit 0. This is the leak
checker working as designed against a buggy client program, not a package
defect — MINK requires explicit release of owned heap values.

## 7. Windows file association

The harness was run against the **packaged** assets, which the earlier
operation had deferred:

```bash
powershell -NoProfile -ExecutionPolicy Bypass \
  -File validation/windows_filetype/verify_filetype_association.ps1
```

The script defaults `-KitDir` to `npm/mink/windows`, so this exercised the
files that ship in the tarball.

| Measurement | Result |
| --- | --- |
| Exit code | 0 |
| `PASS` lines | 56 |
| `FAIL` lines | 0 |
| Final line | `RESULT: PASS - 56 checks` |

Restoration, checked independently of the harness's own verdict:

| Key | After the run |
| --- | --- |
| `HKCU\Software\Classes\.mink` | absent |
| `HKCU\Software\Classes\MINK.SourceFile` | absent |
| `HKLM\Software\Classes\.mink` | absent (no machine-wide write) |
| `HKCU\...\Explorer\FileExts\.mink\OpenWithList` | present, `a = Microsoft.WindowsNotepad_8wekyb3d8bbwe!App`, `MRUList = a` |

The `OpenWithList` entry is Windows Explorer's own open-with MRU, written
when the harness opened a `.mink` file with Notepad in its step 7. It
carries no MINK ProgID and does not associate anything; Session 185 already
recorded this subkey as pre-existing and not a MINK entry.

**Apparatus limitation, stated rather than hidden:** the pre-run snapshot of
these three keys was taken with `reg query <path> /s`, and this `reg.exe`
rejected those dot-prefixed paths with `ERROR: Invalid syntax` instead of
reading them. The saved "before" files are therefore *not* valid absence
evidence, and this operation cannot prove byte-for-byte that the
`OpenWithList` key was unchanged by the run. What is proven independently
is that every MINK-owned entry is gone and that nothing was written to
`HKLM`. The key was deliberately left in place rather than deleted, because
deleting it would itself be an unrequested change to user registry state.

## 8. Documentation and registry checks

| Check | Result |
| --- | --- |
| Relative links in `README.md` | 13 distinct targets, all resolve on disk |
| `README.md` code fences | 46 fence markers (balanced) |
| `docs/language/*.md` code fences | all balanced |
| Platform statement | README states Windows x86_64 only, Linux frozen, macOS unsupported |
| Release-state statement | README distinguishes published 1.0.4 from the local, unpublished 1.0.5 candidate |
| `npm view @p4inz-code/mink version` | `1.0.4` |
| `git diff --check` | clean (exit 0) |
| Local vs remote `main` | identical, `0  0` |
| `v1.0.5` tag, local or remote | none |

## 9. Changes made by this operation

Two files, documentation only:

- this record (new);
- `M6_DEFERRED_SESSIONS.md` — the association-harness item is updated from
  "not run" to "run and green against the packaged assets".

No source, test, manifest, binary or package file was modified. The 1.0.5
candidate tarball was not rebuilt; its hash was re-derived and re-checked
against the records.

## 10. Status

| Claim | Status |
| --- | --- |
| Version metadata consistent at 1.0.5 | verified |
| Packaged stdlib equals repository stdlib (27/27) | verified, and the guard proven to fail on mismatch |
| Tarball complete, matches the release binary | verified |
| Clean install works in space and non-ASCII paths | verified |
| `str_trim` and environment-module regressions | verified from the installed package |
| `invsync` 12/12 | verified |
| Full suite 3,176 / 0 / 1 | verified on this HEAD |
| Association harness against packaged assets | **verified, 56/56** |
| Registry restored of all MINK-owned entries, no `HKLM` write | verified |
| Byte-exact preservation of the pre-run Explorer MRU key | **not provable — invalid baseline capture (§7)** |
| Local and remote `main` equal | verified |
| npm published | **no** — registry still serves 1.0.4 |
| `npm install -g` | **not exercised** — writes the machine-wide prefix |
| Tag / GitHub release created | **no** |

Operation status: **COMPLETE** for verification. The only previously
deferred gate that could be closed without touching machine-wide state —
the association harness against the packaged assets — is now closed.

## 11. Exact next action

Publish the verified 1.0.5 candidate from commit
`c76f33c6a3eff91f2346e438345fc1e293688581` with a maintainer npm token
(`npm publish ./npm/mink --access public`), then tag `v1.0.5` and record the
publication. Publishing is a maintainer action and was not performed here.
