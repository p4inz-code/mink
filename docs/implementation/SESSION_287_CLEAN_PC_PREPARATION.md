# Session 287 — Clean-PC Preparation and Public-Package Truth Audit

Windows x86_64, 2026-11-22. Repository `F:\Works\Mink`, baseline commit
`ce330331a3b99c48c117effe32402a9e9cafd0de`, branch `main`, local `main` equal to
`origin/main` before work began. No compiler, runtime, standard-library or
package-version change. 1.0.5 remains unpublished, M6 remains **BLOCKED /
PENDING EXTERNAL EXECUTION**, M7 Phases A–E stay complete with the ecosystem half
demand-gated, M8 stays gated on M6, M9 is conditional on M8, and Linux stays
frozen.

## 1. Objective

Prepare the machine for an installation test the owner performs by hand, and make
the documentation tell the truth about the package the registry actually serves.
The session did **not** install MINK, publish anything, register the file type as
a lasting change, or run the owner's test.

## 2. The public stable package (verified against the registry)

| Item | Value |
| --- | --- |
| `latest` dist-tag | `1.0.4` (`npm view @p4inz-code/mink dist-tags`) |
| Published versions | `1.0.1`, `1.0.2`, `1.0.3`, `1.0.4` |
| 1.0.4 publish time | 2026-09-30T15:34:10.031Z |
| Tarball | `https://registry.npmjs.org/@p4inz-code/mink/-/mink-1.0.4.tgz` |
| Integrity | `sha512-mFmIdnD8qYnrtmCWwKE38Cv5E063uw2vYOzFZ6tZuQHD/bg1GDnmpSG+tyMRj77t/+M55MdNR8Oi/fVO8zs21g==` |
| `shasum` | `fdea0ad977dea7c2b8734ac87a5ca970fd373132` |
| File count / unpacked size | 28 files, 4,332,882 bytes |
| `bin` / `engines` | `mink` → `bin/mink.exe` / `node >=18` |

The tarball was downloaded and unpacked (not installed). Its complete contents:

```text
package/bin/mink.exe   package/package.json
package/stdlib/*.mink  (26 modules)
```

**There is no `windows/` directory, no ICO, no registration script and no `LICENSE`
in the published 1.0.4 tarball.** A clean public installation therefore cannot
register the `.mink` file type or show the MINK icon; that part of the test is
unavailable on the public track and is recorded as such rather than approximated.

The locally prepared candidate was also verified without publishing:

| Item | Value |
| --- | --- |
| Version | `1.0.5`, `npm pack` → 35 files |
| Path | `tmp/cleanpc/p4inz-code-mink-1.0.5.tgz` (git-ignored build artifact) |
| SHA-256 | `890bf1fb769a551f0036b59250de65747f76c24738ec11cd419f7c76e5c56476` |
| Windows assets | `windows/` with `mink-app.ico`, `mink-file.ico`, `mink.rc`, `register-mink-filetype.ps1`, `README.md` |
| Binary | prints `mink 1.0.5` |

## 3. Documentation audit

297 tracked Markdown files were inventoried. Historical records (`docs/benchmarks/`,
`docs/implementation/`) were left untouched by design: their version statements
describe the state at their session, and rewriting them would destroy evidence.
User-facing documents were checked against the registry metadata, both real
tarballs, the extracted binaries and the registration script.

Corrected, because they stated something the artifacts contradict:

| Document | Defect | Correction |
| --- | --- | --- |
| `README.md` | badge advertised `1.0.5`, the unpublished version | badge now names the published `1.0.4`; the package-state note still explains the candidate |
| `README.md` | the optional file-type section told the reader to run `windows\register-mink-filetype.ps1` from `npm root -g`, which 1.0.4 cannot provide | checkout command first, package command only "once a version that ships the kit is installed" |
| `README.md` | the 1.0.4 standard-library leak was not mentioned anywhere a first-time user would see it | added to Limitations with the exact symptom, exit code and affected calls |
| `docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md` | §6 opened with the package command, which cannot work today | states the tarball finding (28 files, no `windows/`) and leads with the checkout |
| `docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md` | §5.2 described the missing `windows/` without naming the verified evidence | records the 2026-11-22 download and inspection |
| `npm/mink/windows/README.md` | same package-command assumption | states that the kit exists only in packages that ship it, with both invocations |

Already accurate, left alone: `docs/language/WINDOWS_INSTALLATION.md` (it already
pins 1.0.4, reports 28 files and no `windows/`, and records the `str_trim` exit 106
and `mod environment;` `E-P01` defects), the roadmap's release state,
`docs/benchmarks/M6/*` (historical), and the brand document's registry section.

The audit confirmed the file-type inventory: `.mink` is the only registered
extension; `mink.toml` and `mink.lock` are identified by file name and deliberately
not associated; `.mink/` is a state directory; `.exe` uses Windows' own type.

## 4. Machine state

Captured before and after by `tmp/cleanpc/capture.ps1`, with the MINK-owned keys
exported to `tmp/cleanpc/before/registry/*.reg` first.

| Item | Before | After |
| --- | --- | --- |
| Global package | `@p4inz-code/mink@1.0.2` at `C:\Users\Admin\AppData\Roaming\npm\node_modules\@p4inz-code\mink` | not installed |
| `mink` command | `...\npm\mink`, `mink.cmd`, `mink.ps1`; `mink 1.0.2` | `where.exe mink` → not found; not on `PATH` |
| Shim files in prefix | `mink`, `mink.cmd`, `mink.ps1` | none |
| `HKCU\Software\Classes\.mink` | `MINK.SourceFile`, `text/x-mink`, `PerceivedType=text` | absent |
| `HKCU\Software\Classes\MINK.SourceFile` | friendly name + `DefaultIcon` | absent (both keys) |
| DefaultIcon | `"F:\Works\Mink\npm\mink\windows\mink-file.ico",0` | absent |
| `HKLM` classes | absent | absent |
| Explorer `FileExts\.mink` | `OpenWithProgids\MINK.SourceFile`, `OpenWithList` → Notepad | MINK value and its subkey removed; the user's Notepad entry kept |
| Other extensions | 502 Explorer `FileExts` entries | identical digest; `.txt`, `.md`, `.js`, `.rs`, `.json` samples unchanged |
| PATH | only the npm prefix (no MINK entry) | unchanged |
| Other installs | none in `Program Files`, `ProgramData`, `LocalAppData`, `~\.mink` | unchanged; `@p4inz-code/repo-map` untouched |

Registry removal used the project's own `register-mink-filetype.ps1 -Uninstall`,
not hand-deleted keys. npm used `npm uninstall -g @p4inz-code/mink`. No wildcard
deletion, no icon-cache clear, no PATH edit, no npm cache purge.

## 5. Finding: the published 1.0.4 stdlib leaks on string transforms

Discovered while executing the documented commands against the extracted 1.0.4
binary, and reproduced on both extracted artifacts:

| Program | 1.0.4 | 1.0.5 |
| --- | --- | --- |
| intrinsics only (`rt_str_concat` + `rt_str_free`) | exit 0 | exit 0 |
| `str_to_upper(rt_str_concat(...))`, result freed by the caller | **exit 106, `E-R06: memory leak`** | exit 0 |
| `str_trim` + `str_to_lower` + `str_reverse`, results freed | **exit 106** | exit 0 |
| `mink test` with `mod assert;` | exit 0, `1 passed` | exit 0 |

The packaged `stdlib/strings.mink` in 1.0.4 contains 6 `rt_str_free` calls; the
repository version contains 31. The input handed to a transformation is consumed
and never released, so the caller cannot free it — the documented ownership model
leaves the allocation live and the exit-time leak check fires. This is the same
defect class as the `environment` leak recorded in the 1.0.5 change list, and it is
reachable from ordinary, correctly written user code.

It is a defect of the published artifact, not of the checklist, and it is now
documented in the README, the Windows installation guide and the clean-install
checklist. Fixing it on the registry requires publishing 1.0.5, which this session
did not do.

## 6. Verification performed

| Check | Result |
| --- | --- |
| Registry `latest` / versions / publish time | `1.0.4`; four versions; 2026-09-30 |
| Published tarball download and unpack | 28 files, no `windows/`, binary reports `mink 1.0.4` |
| Local candidate pack and unpack | 35 files, `windows/` present, binary reports `mink 1.0.5` |
| CLI commands from the README, run against 1.0.4 | `help`, `--version`, `check`, `run`, `build`, `run` of the built `.exe`, `test`, piped `repl` — outputs matched |
| `mink build --help` rejection (documented) | reproduced exactly |
| Space-containing working directory | `run` and `build` succeeded there on both artifacts |
| Pre/post machine capture | MINK state removed, unrelated state identical |
| `verify_brand_assets.py` | PASS, 9 checks |
| `verify_filetype_association.ps1` | PASS, 88 checks / 15 steps, machine restored |

The association harness registers the type temporarily while it runs and restores
the pre-run state in its final step; the machine was verified clean again
afterwards (`.mink` and `MINK.SourceFile` absent).

## 7. Owner actions

1. Perform the clean installation test in
   [`../validation/CLEAN_INSTALL_CHECKLIST.md`](../validation/CLEAN_INSTALL_CHECKLIST.md),
   track A first. Record the A5 exit code as the published-artifact finding.
2. Decide whether to publish 1.0.5. Publishing fixes the stdlib leak, ships the
   Windows kit and is the only way the association and icon can be tested from an
   installed package. It is not authorized here and was not done.
3. Send the M6 invitation; M6 remains outstanding and this session adds no
   independent evidence.

## 8. Delivery

One commit on top of `ce330331a3b99c48c117effe32402a9e9cafd0de` containing the
documentation corrections, this record and the checklist, staged by explicit path.
The four user-owned untracked files were not staged, and no tarball, package
publication, tag or release was created.
