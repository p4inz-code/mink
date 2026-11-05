# Session 139 — Brand assets, Windows file identity and distribution integration

**Scope:** integrate the supplied professional MINK artwork package into the repository and
the actual distribution/tooling layer: authoritative asset tree, Windows application and
`.mink` source-file icons, per-user `.mink` file association (register/unregister),
editor/web asset handling, packaging and documentation — with real validation.

**Out of scope and untouched:** compiler semantics, runtime, standard library, version
number (stays **1.0.4**), releases, tags, publishing, Linux, M3, and every unrelated
uncommitted change already in the working tree.

## 1. Starting state

| Item | Value |
| --- | --- |
| HEAD | `bb3ff1e3caafdb00004223ff4a5a204ecdf7724d` |
| Branch | `main` |
| Version | **1.0.4** (unchanged) |
| Working tree | 24 modified tracked files and the untracked Session 128–137 records, `docs/roadmap/archive/`, `stdlib/httpserver.mink`, `npm/mink/stdlib/httpserver.mink`, `tests/httpserver_lib.rs`, `nl1.exe`, `nl1.mink`, a pasted transcript, and the artwork ZIP |
| Artwork package | `MINK UI SETS OFFICIAL.zip`, 2,331,842 bytes, 192 entries under a top-level `MINK/`, SHA-256 `d65f57306995a4a595440a548c346be197242686db0849b4992c66fb54e30884` |

The pre-existing work was inspected before any edit (`git status`, `git log`) and left
untouched: nothing was reset, discarded or reformatted, and no unrelated file is part of
the commit that carries this record.

## 2. What the repository already had (answers to the integration questions)

| Question | Finding |
| --- | --- |
| Compiler binary / output mechanism | `cargo build` → `target/release/mink.exe`; zero crate dependencies, **no build script**, no resource script anywhere |
| npm distribution | `npm/mink/` with `bin/mink.exe`, 27 `stdlib/*.mink`, `package.json` (`files: ["bin/mink.exe","stdlib"]`) |
| Windows installer | **none** — no `.iss`, no setup/bootstrap script, no `windows/` directory anywhere in the repository |
| Existing Windows assets | **no `.ico` file existed in the repository at all** |
| Compiler icon embedded? | **no** — `target/release/mink.exe`, `target/debug/mink.exe` and `npm/mink/bin/mink.exe` all report PE data directory 2 (resources) = RVA 0, size 0 |
| Existing editor tooling | none; no VS Code extension, no `package.json` with `contributes.languages` |
| Existing website | none in this repository (`docs/web/WEB_BACKEND_ARCHITECTURE.md` is a specification for MINK programs, `mink.dev` appears once as a hypothetical example in an audit) |
| Release/package tests | `tests/release.rs`, including `s107_npm_stdlib_bundle_matches_repo_stdlib` (bundled `stdlib/` must stay byte-identical to `stdlib/`) |

## 3. Asset integration

The package was extracted to a temporary (git-ignored) directory and validated before
anything was copied. A content check found **27 groups of byte-identical files**:
`logo/monochrome/` repeats eleven files from the lockup directories, all fourteen
`windows/png/` frames repeat `icon/*/png/`, the `web/favicon/` PNG ladder repeats the
application-icon frames, and `icon/application/mink-app-256.svg` is byte-identical to
`mink-app-master.svg`.

Integrated into [`assets/brand/`](../../assets/brand/) — **116 files, 2.7 MB**:

| Directory | Files | Class |
| --- | --- | --- |
| `masters/` | master SVG (symbol + wordmark groups), `mink-geometry.json` | source |
| `logo/{primary,horizontal,compact,symbol,wordmark}/` | SVG + @2x PNG in `-ink`/`-white`/`-black`, symbol optical-small and silhouette variants, 16–512 PNG ladder | production |
| `on-background/` | primary logo on its intended dark, light and single-colour background | production |
| `icon/application/`, `icon/source-file/` | master SVG, per-size SVGs 16–128, PNG frames 16–256 (source-file also 1024 and a one-colour variant) | production |
| `windows/ico/` | `mink-app.ico`, `mink-file.ico` | production (Windows) |
| `web/github/`, `web/social/` | README header pair, avatars, social preview, profile avatar, docs header | production (GitHub/web) |
| `previews/` | size ladders and Explorer mock, light and dark | preview only |

Duplicate contents were collapsed (each artwork is stored once, in the directory that owns
it): `logo/monochrome/`, `windows/png/`, `web/favicon/` and the duplicate
`mink-app-256.svg` are not in the tree, and the design package's README/snippet files were
replaced by `assets/brand/README.md` and
[`docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md`](../brand/BRAND_AND_WINDOWS_INTEGRATION.md).
The rejected alternatives (`extras/app-icon-alternatives/option-b`, `option-c`) are **not**
integrated and the validator fails if they reappear. No artwork was modified: SVGs, PNGs
and ICOs are byte-identical to the supplied files.

## 4. Windows integration: what was built

`npm/mink/windows/` — the only copy that ships, co-located with `bin/mink.exe` so the
registration can resolve its own location:

| File | Purpose |
| --- | --- |
| `register-mink-filetype.ps1` | register / unregister the per-user file association |
| `mink-file.ico`, `mink-app.ico` | shipped copies, byte-identical to `assets/brand/windows/ico/` |
| `mink.rc` | resource script for embedding (icon paths corrected to sit beside it) |
| `README.md` | short user-facing usage |

`npm/mink/package.json` gained `"windows"` in `files` (the only packaging change; version
and registry metadata untouched).

### Identity and registry layout

`.mink` → `MINK.SourceFile` → `MINK Source File` → `mink-file.ico`, content type
`text/x-mink`, perceived type `text`, all per-user under `HKCU`. Full table in
[`docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md`](../brand/BRAND_AND_WINDOWS_INTEGRATION.md) §5.

### Open behaviour

No `open` verb is registered by default, so Windows can never execute a `.mink` file:
the shell's normal "Open with" flow runs, the user's answer is stored by Windows in the
user-owned `FileExts\.mink\UserChoice`, and `-Editor <editor.exe>` binds a specific editor
explicitly (`"<editor.exe>" "%1"`). The supplied `.reg` reference (Notepad, hard-coded
`%LOCALAPPDATA%\Programs\MINK\bin`) is therefore **not** shipped, and the Inno Setup
snippet is not shipped because there is no installer to carry it — the PowerShell script is
the single authoritative mechanism.

### Safety properties implemented

- refuses to replace a foreign `.mink` owner without `-Force`; `-Force` records the previous
  owner and `-Uninstall` restores it;
- never deletes another application's values or the class-level `OpenWithProgids` list it
  may have created;
- removes only MINK-owned values, prunes an extension key only when it is completely empty,
  and never touches `UserChoice`;
- idempotent: repeated runs produce identical values and subkeys, and a stale
  `-Editor` command is removed when a later run omits `-Editor`;
- `-InstallDir`/`-MinkExe`/`-IconPath`/`-Editor` override every path assumption; otherwise
  the installation is resolved from the script's own location and then from `mink.exe` on
  PATH — no developer-machine path is hard-coded anywhere;
- shell refresh via `SHChangeNotify(SHCNE_ASSOCCHANGED)` plus `ie4uinit.exe -show`; Explorer
  is never restarted or killed;
- all registry access uses the .NET API (see §7 for why).

### Application icon

`mink-app.ico` is integrated as the official application icon asset (7 frames, 32-bpp, each
frame byte-identical to its PNG export) and shipped in the package, but it is **not embedded
in `mink.exe`**: the compiler build has no build script, no external toolchain and a
recorded release-artifact hash, and adding `rc.exe`/`windres` to the link would change both.
This is the documented limitation of
[`docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md`](../brand/BRAND_AND_WINDOWS_INTEGRATION.md) §3,
with `mink.rc` provided for a local embed. Explorer, shortcut and installer identity for
`.mink` files comes from the file-type icon, which is integrated and verified.

## 5. Editor, web and social decisions

- **VS Code:** no extension exists, and none was invented. The supplied snippet was inspected
  and its guidance recorded in the brand document §7 together with the exact asset sources a
  future extension needs (`icon/source-file/mink-file-master.svg` → `./icons/mink-file.svg`,
  `icon/application/png/mink-app-128.png` → `./icon.png`).
- **Web/favicon:** this repository has no website, so the favicon set is not duplicated here;
  the document states that it belongs to the website project and is derivable from the
  application icon.
- **GitHub/social:** the README header pair is integrated and used by `README.md`; the avatar
  and social-preview uploads are documented as manual GitHub settings and no script touches
  GitHub configuration.

## 6. Validation performed

All numbers below are from actual runs on 2026-10-27 (Windows x86_64, Windows PowerShell 5.1).

**Assets — `python validation/brand_assets/verify_brand_assets.py` → PASS, 9 checks**

| Check | Result |
| --- | --- |
| Inventory (73 required files) | PASS |
| Rejected/duplicate artwork classes absent | PASS |
| Volume (49 SVG, 63 PNG present) | PASS |
| SVG structure — 49 files: XML, `<svg>` root, width/height/viewBox, no external reference, every `url(#id)`/`href="#id"` resolves | PASS |
| PNG integrity and sizes — 63 files: signature, chunk CRCs, IDAT length, RGBA alpha channel, file-name/dimension agreement | PASS |
| ICO structure — 2 files: 7 frames (16/24/32/48/64/128/256), 32-bpp, dimensions, byte-identical to the PNG exports | PASS |
| Shipped Windows kit — 5 files present, both ICOs byte-identical to the brand tree, `files[]` still publishes `windows` | PASS |
| No byte-identical duplicates in the tree | PASS |
| `mink.exe` resource directory | INFO — absent (documented limitation) |

Negative control: removing `"windows"` from `package.json` `files` makes the kit check FAIL
with exit 1; restoring it returns to PASS.

**Windows association — `powershell -File validation/windows_filetype/verify_filetype_association.ps1`
→ PASS, 56 checks, 11 steps, three consecutive runs.** The test builds a temporary install
tree whose path contains a space, snapshots the keys it touches, and asks Windows (not the
registry) what it resolved:

| Step | Verified |
| --- | --- |
| 3 | foreign owner: refusal without `-Force`, exits 1, message names the owner, nothing written; with `-Force`: extension re-pointed, foreign value and subkey preserved, previous owner recorded |
| 4 | uninstall restores the previous owner, removes MINK content type, keeps the foreign value, subkey and extension key |
| 5 | from a clean extension, registration through the package layout alone: ProgID, content type, perceived type, display name, quoted `DefaultIcon`, no open-command key, OpenWithProgids entry; `SHGetFileInfo` type name = `MINK Source File`; `AssocQueryString` friendly name, default icon (`"…\mink-file.ico",0`) and ProgID; shell icon handle resolved; open command contains no `mink.exe` |
| 6 | idempotency: identical values/subkeys after a second run, unchanged shell type name, two runs in one PowerShell session without an `Add-Type` conflict |
| 7 | with `-Editor`: command is `"<probe-editor.exe>" "%1"`; `Start-Process hello.mink` reached the probe with the file path (cwd noted), and **no `hello.exe` was produced — the source was not compiled or executed** |
| 8 | re-registering without `-Editor` removes the stale command and prunes `shell\open` / `shell` |
| 9 | from a Unicode path (`mink-…-…\package with space\...`) with no arguments: registration exits 0 and the Unicode icon path round-trips, confirmed by `AssocQueryString` |
| 10 | uninstall from a clean extension leaves no `MINK.SourceFile`, no empty `.mink` key, no OpenWithProgids value |
| 11 | recursive snapshot comparison: the registry is byte-for-byte back to its starting state |

Negative control: the same test against a copy of the kit with a different ProgID and content
type reported **9 failures and exit code 1** — the harness detects the defect it exists for.

**Repository checks**

| Check | Result |
| --- | --- |
| `cargo check --all-targets` | exit 0 (pre-existing warnings only; no source file of the compiler was touched) |
| `cargo test --test release s107` (npm stdlib bundle guard) | 1 passed, 0 failed |
| `npm pack --dry-run` (no publish) | 34 entries: `bin/mink.exe`, `package.json`, 27 `stdlib/` modules, `windows/README.md`, `windows/mink.rc`, `windows/register-mink-filetype.ps1`, `windows/mink-app.ico`, `windows/mink-file.ico` — 54,852 bytes added to the package |
| `git diff --check` | clean |

The full Rust suite was **not** re-run: no Rust source, stdlib module or test was modified in
this session, and `cargo check --all-targets` plus the packaging-adjacent release test cover
the change surface.

## 7. Defects found during validation and their fixes

All of these were in the integration artifacts written for this session, discovered by running
the validation rather than by inspection; no MINK source file was involved.

1. **`New-Item -Force` silently wiped an existing registry key.** Registering with `-Force`
   against an extension owned by another application deleted that application's values and
   subkeys (reproduced directly: a key with 1 value and 1 subkey became empty). Fixed by
   never using `-Force` on an existing key, and finally by moving all registry access to the
   .NET API (`CreateSubKey` opens without clearing).
2. **The registry provider cannot create missing parent keys** (`New-Item …\shell\open\command`
   fails with "The registry key at the specified path does not exist"), so `-Editor` binding
   failed on a clean extension. Fixed by the same .NET switch.
3. **`Remove-ItemProperty -Name '(default)'` cannot delete the unnamed default value** in
   PowerShell 5.1 — it reports "Property (default) does not exist" even when it is set, which
   aborted `-Uninstall` half-way. Fixed with `RegistryKey.DeleteValue("", $false)`.
4. **The test's assertion counters were function-local**, so the HTML-style summary reported
   "PASS — 0 checks" and a broken run could have exited 0. Fixed by `$script:`-scoped counters,
   plus the negative control above, which now proves failures fail the run.
5. **A BOM-less `.ps1` with non-ASCII text is read in the ANSI code page by PowerShell 5.1**,
   which corrupted the Unicode test path; both scripts are now pure ASCII (the test builds the
   Unicode name from code points).

An earlier observation was *not* a defect and is documented as expected Windows behaviour: a
foreign class-level `OpenWithProgids` list makes the shell treat the extension as "open with"
and ask the user to confirm the application once, which is why that list is left alone.

## 8. Limitations (real, remaining)

1. `mink.exe` still has no embedded icon resource (§4); `mink.rc` and the package ICO are the
   distribution-layer answer.
2. There is no installer, so association registration is an explicit user step
   (`README.md` → "Optional: give `.mink` files their own Windows identity").
3. No MINK editor extension exists yet; the integration path is documented, not built.
4. `cmd.exe`'s `assoc`/`ftype` cannot see per-user associations, so they are not usable as
   evidence; the test uses the shell APIs Explorer itself uses.
5. A previous manual `.mink` default-app choice (`UserChoice`) keeps governing until the user
   changes it in Windows settings; MINK never writes or removes it.
6. Favicon and GitHub-avatar application remain external steps.

## 9. Files

**Added:** `assets/brand/` (116 files + `README.md`), `npm/mink/windows/` (5 files),
`docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md`,
`validation/brand_assets/verify_brand_assets.py`,
`validation/windows_filetype/verify_filetype_association.ps1`, this record.

**Changed:** `npm/mink/package.json` (one line: `files` gains `"windows"`), `README.md`
(official header artwork instead of the plain `# MINK` heading, the file-association
install/uninstall section, and two documentation links).

**Intentionally not integrated:** the rejected app-icon alternatives, the website favicon
set, the redundant duplicate artwork listed in §3, the `.reg` and Inno Setup reference
snippets, and the artwork ZIP itself (design source, kept out of the repository).

## 10. Final state and verification

- Compiler, runtime, standard library, `Cargo.toml` and version **1.0.4**: unchanged; the
  binary in `npm/mink/bin/mink.exe` was not rebuilt or replaced.
- Nothing was published, tagged, version-bumped or pushed; no GitHub setting was changed.
- The registry state of this machine was left exactly as it was found (the association test
  snapshots and restores the three keys it touches, verified by recursive comparison, and the
  machine currently has no MINK-owned `.mink` entries).
- Validation: brand assets 9/9 checks PASS, Windows association 56/56 checks PASS (three
  runs) with a negative control that fails as intended, `cargo check --all-targets` clean,
  `cargo test --test release s107` passing, `npm pack --dry-run` showing the five new entries,
  `git diff --check` clean.
- Pre-existing uncommitted work in the tree (Sessions 128–138 records, `stdlib/`, `src/`,
  `tests/` and documentation edits) was neither modified nor removed; the commit carrying this
  record contains only the branding integration, with `README.md` staged for the three hunks
  this session wrote so the unrelated pending README edits stay in the working tree.

## 11. Closure and handoff

**Session 139 is complete.** Brand asset integration, the `.mink` Windows file identity,
Windows registration/unregistration, package inclusion and their validation are done:

- official artwork integrated in `assets/brand/` (116 files, byte-identical to the supplied
  package) and shipped kit in `npm/mink/windows/` (published through `files[]`);
- compiler, runtime and standard library unchanged; **version stays 1.0.4**;
- nothing published to npm, no tag, no release, no push, no version bump;
- commit `3b93bdbb834ba2faeb63faf9e22825f279d93416` —
  *brand: integrate the official MINK artwork and the .mink Windows file identity* — is HEAD
  and contains only this work (127 files, +2145/−2).

**Two external branding actions remain, both in the GitHub web UI** (GitHub exposes no REST or
GraphQL endpoint for either; `gh` is authenticated as `p4inz-code` and still cannot do them):

| Action | File to upload | Where |
| --- | --- | --- |
| Repository social preview | `assets/brand/web/social/mink-social-preview-dark-1280x640.png` (1280x640, SHA-256 `6d34dbd268b0e7b0996c1730acc2fcc7e184c9efee9ce74310c73b8de6dae26e`) | `github.com/p4inz-code/mink` → *Settings → General → Social preview → Upload an image* |
| Account avatar | `assets/brand/web/github/mink-avatar-dark-500.png` (500x500, SHA-256 `2957a60e771cb6aa5d525b441f651538441993feaea2637af3e231fa7957223e`) | `github.com/settings/profile` → *Profile picture* (the account `p4inz-code` is a user account; an organisation would take the same file under its own settings) |

Both files are the official supplied artwork (byte-identical to their entries in
`MINK UI SETS OFFICIAL.zip`), are not previews and are not the rejected app-icon
alternatives; `-light` variants exist for light backgrounds but the design package designates
the `-dark` files for these two slots. The repository stays public; no other setting is
involved.

**Repository state at handoff:** branch `main`, HEAD `3b93bdb`, working tree carrying only
the pre-existing uncommitted work (plus this handoff note, deliberately uncommitted since no
new commit was authorised). `git diff --check` clean. `bb3ff1e` and `3b93bdb` are not on the
remote, and a `git fetch --dry-run` shows the remote has moved on independently
(`b860837..8b00083  main -> origin/main`); no fetch, merge, rebase or push was performed, so
reconciling local `main` with `origin/main` (and the duplicated "Northbyte Studios" rename in
the working tree versus remote commit `b860837`) is a decision for the maintainer.

**No new MINK roadmap milestone** (M3, feature, ecosystem, packaging or platform work) is to
be started as part of this integration.
