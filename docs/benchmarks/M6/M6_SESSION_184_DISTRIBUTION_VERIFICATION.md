# M6 Session 184 - published package and distribution integrity

**Date:** 2026-11-05
**Status:** **COMPLETE (audit)**. The published artifact is usable for
intrinsic-only programs but has **VERIFIED DISTRIBUTION DEFECTS** for the
standard library and Windows assets.
**Starting HEAD:** `40facceae9161666db5fcd68f8e8ba1c00c9fdc0`
**Ending code HEAD:** `b67fda4fb8044c0b3db1b06ba8230d3ad0a34cb1` (bundle sync;
this record is the following documentation commit)
**Branch:** `main`
**Upstream at start:** `origin/main` = `b860837a8a6039f304a503e3d85728a643ab470b`
**Expected/preserved worktree:** 63 entries (37 modified, 26 untracked); 63
after the bundle-sync commit.

**Measurement boundary:** the first package artifact (`tmp/s184/root.txt`,
Unix time `1793879534`) to the last evidence command (`1793879963`) is **429 s
(7 min 9 s)**. Baseline review and initial registry inspection preceded that
marker. This is a wall-clock execution window for the package work, not a claim
of human study time.

## 1. Published artifact

| Item | Observation |
| --- | --- |
| Registry name | `@p4inz-code/mink` |
| Published version / `latest` | `1.0.4` / `1.0.4` |
| Tarball | `https://registry.npmjs.org/@p4inz-code/mink/-/mink-1.0.4.tgz` |
| Tarball size | 1,348,748 bytes |
| SHA-1 (`dist.shasum`) | `fdea0ad977dea7c2b8734ac87a5ca970fd373132` |
| SHA-512 (`dist.integrity`) | `sha512-mFmIdnD8qYnrtmCWwKE38Cv5E063uw2vYOzFZ6tZuQHD/bg1GDnmpSG+tyMRj77t/+M55MdNR8Oi/fVO8zs21g==` |
| Downloaded SHA-256 | `af89f00ce640b91abf14efa750de3baa9d3c3bc3c69fd2f8337f4ff2d929d299` |
| Files | **28**: `package.json`, `bin/mink.exe`, 26 `stdlib/*.mink` |
| Manifest `files` | `["bin/mink.exe", "stdlib"]` |
| License / platform | `Apache-2.0`; `os: win32`; `cpu: x64`; Node `>=18` |

The downloaded tarball's SHA-1 and computed SHA-512 match the registry
metadata. The artifact is byte-faithful to the release preparation recorded in
`docs/implementation/SESSION_121_V1_0_4_RELEASE.md` and to tag `v1.0.4`:

- tag `v1.0.4` = `fac9fa966c59a25f7f2152db36a7fece92a869ba`;
- tag `npm/mink/package.json` has only `["bin/mink.exe", "stdlib"]`;
- tag `npm/mink/windows/` does not exist;
- tag `stdlib/strings.mink` and `environment.mink` hash to the published
  versions (`e8e486d7...` and `1da09748...`);
- tag `npm/mink/bin/mink.exe` hashes to `2bc97c92...`, equal to the published
  binary and the current repository npm copy.

**Conclusion:** this is not a corrupted tarball or a failed upload. The
published 1.0.4 artifact is the tag artifact; the repository's later 1.0.4
work was never published.

## 2. Clean installation

All installations used the downloaded tarball or the registry spec in a
disposable directory outside the repository. The global 1.0.2 install was not
used.

| Check | Command / condition | Result |
| --- | --- | --- |
| Isolated tarball install | `npm install --prefix "$ROOT/install" "$ROOT/p4inz-code-mink-1.0.4.tgz" --ignore-scripts --no-audit --no-fund` | PASS |
| Install path with spaces | prefix `.../install with space` | PASS |
| Registry-spec install | `npm install --prefix "$ROOT/registry-install" @p4inz-code/mink@1.0.4 ...` | PASS |
| Version | package `bin/mink.exe --version` | `mink 1.0.4` |
| Declared launcher | `.bin/mink.cmd` invokes `..\@p4inz-code\mink\bin\mink.exe` | PASS |
| First program | `hello.mink`, build then run | output `42`, exit 0 |
| Second independent project | `second project/two.mink` | output `99`, exit 0 |
| Project path with spaces | `project with space/hello.mink` | PASS |
| Foreign working directory | project under the OS temp root, not the repository | PASS |
| Package writes during build | full package tree hash before/after | unchanged |
| Standard-library program | `mod strings;` + `str_trim(rt_str_concat("  hi  ", ""))` | **exit 106, E-R06** |
| Environment module | `mod environment;` | **build failed, E-P01** |
| README complete fences | 5/5 compiled with the published compiler | PASS |

The compiler resolves its bundled `stdlib` beside the executable; no
repository `stdlib/` was present in the test working directory. Offline
operation was **not tested** with networking disabled. A global `npm install
-g` was **not tested**; prefix installs and the registry spec were.

## 3. Published versus repository

The working repository is at 1.0.4 in `package.json`, but its standard library
and Windows assets are ahead of tag `v1.0.4`.

| Asset | Published 1.0.4 | Repository HEAD / package copy | Classification |
| --- | --- | --- | --- |
| Compiler binary | `2bc97c9210d0...bf0c` | npm copy identical; `target/release/mink.exe` is `d8f61348...9f09` | Binary is the tag/package build; local rebuild differs (build identity, not a functional defect established here) |
| `stdlib/strings.mink` | `e8e486d7...b1b1c` | `79c27776...8dd5` | **Published artifact is pre-fix**; heap `str_trim` leaks |
| `stdlib/environment.mink` | `1da09748...9284` (old `extern fn`) | `588c72c2...54c0` (wrapper repair) | **Published artifact is pre-fix**; module does not load |
| `windows/` | absent | five tracked files in HEAD | **Published artifact omits the association assets** |
| `stdlib/httpserver.mink` | absent | present only as pre-existing untracked M3/M4 work | Not part of tag/HEAD 1.0.4; not a release regression |
| Other stdlib modules | 12 of 26 differ from HEAD; 14 are byte-identical | post-tag M3/M4 changes | Source/release divergence, not individually adjudicated here |
| Manifest | `["bin/mink.exe","stdlib"]` | `["bin/mink.exe","stdlib","windows"]` | **Published manifest predates Windows packaging** |

The current working-tree package contains **34 files** (`bin`, `package.json`,
27 stdlib modules including `httpserver`, and five `windows/` files). That is
what a new package from this checkout would contain; it is not what npm serves.

### Reproduction of the failures

```text
$ mink build stdlib.mink && ./stdlib.exe
hi
mink: runtime error[E-R06]: memory leak: live allocations remain at exit
exit 106

$ mink build env.mink
mink: error[E-P01]: expected a top-level declaration (`fn`, `let`, or `const`)
exit 1

$ python benchmarks/m6/session180/verify.py --compiler <published>/bin/mink.exe
FAIL: case normal failed: exit code expected 0, got 106
```

The verifier stops at its first case (`normal`), so the published package does
not complete the 12-case suite. The same binary with the repository
`strings.mink` and `environment.mink` substituted into a disposable package
copy passes **12/12** (`all invsync cases passed`), and the standard-library
and environment programs then print `hi` and `ok` with exit 0. The compiler
binary is therefore not the sole cause; the bundled data is.

A fresh pack of the working-tree npm directory (34 files, SHA-256
`9549a363b4b18a167727c77c021b31b07c9ebdcff2578effed04396a56fcfd78`)
also installs, prints `mink 1.0.4`, and passes **12/12**. This is repository
evidence for a future release, not evidence about the published artifact.

## 4. Repository-side fix

`cargo test --test release s107` failed before the fix:

```text
stdlib/environment.mink differs between stdlib/ and npm/mink/stdlib/
```

Session 182 repaired the root module but not the npm bundle. Commit
`b67fda4fb8044c0b3db1b06ba8230d3ad0a34cb1` copies the repaired module into
`npm/mink/stdlib/environment.mink`; the guard now reports:

```text
test s107_npm_stdlib_bundle_matches_repo_stdlib ... ok
test result: ok. 1 passed; 0 failed; 0 ignored
```

This fixes the repository bundle for a future pack. It cannot repair the
already-published 1.0.4 artifact.

## 5. Findings

| Finding | Classification |
| --- | --- |
| Registry serves 1.0.4 and tarball integrity matches | **VERIFIED PASS** |
| Tarball is the tag `v1.0.4` artifact, not a corrupted upload | **VERIFIED PASS** |
| Clean install, launcher, version, first intrinsic program | **VERIFIED PASS** |
| Install/project paths with spaces, foreign cwd, no package writes | **VERIFIED PASS** |
| Published standard library matches current 1.0.4 documentation | **VERIFIED DEFECT** - `str_trim` exit 106 |
| Published environment module loads | **VERIFIED DEFECT** - `E-P01` |
| Published package contains `windows/` association assets | **VERIFIED DEFECT** - directory absent |
| Repository npm bundle matches root stdlib | **FIXED** in `b67fda4` |
| Global `npm install -g` and offline operation | **NOT TESTED** |
| Full standard-library execution from published package | **BLOCKED** by the stale bundle |

## 6. Required future release action

Do **not** publish, re-tag or change the version in this execution. A corrected
release must be cut from a commit that includes the post-tag fixes, with a new
version (for example 1.0.5), using the 34-file npm directory; the release gate
must install that tarball and run the Session 180 verifier plus the stdlib
smoke tests before publication. The existing 1.0.4 artifact should be treated
as unsupported for standard-library and association claims.

## 7. Next task

Session 185: Windows installation, file-association safety and CLI first-run
audit, using the same published artifact and the repository association
harness. Do not start M7.
