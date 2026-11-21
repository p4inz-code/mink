# Session 286 — Brand and Windows File-Type Integration Closure

Windows x86_64, 2026-11-22. Work performed in `F:\Works\Mink`; baseline commit
`d2eeb6169908c5330ca85706f148928e81e450e6`, with local `main` equal to
`origin/main` before work began. No MINK language, compiler, runtime or
standard-library source changed. 1.0.5 remains unpublished, M6 remains
**BLOCKED / PENDING EXTERNAL EXECUTION**, M7 Phases A–E stay complete with the
ecosystem half demand-gated, M8 stays gated on M6 evidence, M9 is conditional on
M8, and Linux stays frozen.

## 1. Objective

Finish the existing MINK file-type branding and Windows association work rather
than extend it: enumerate every MINK file type, verify the icon each supported
type receives, remove the dependence of the registered icon on a development
path where the repository allows it, extend the permanent validators, refresh
the documentation and roadmap, and deliver the result. Language and ecosystem
development stayed on hold.

## 2. Baseline

| Item | Value |
| --- | --- |
| Branch | `main`, tracking `origin/main` |
| HEAD / `origin/main` | `d2eeb6169908c5330ca85706f148928e81e450e6` (synchronised) |
| Worktree | clean except the four user-owned untracked files: `MINK UI SETS OFFICIAL.zip`, `freebuff-chat-2026-10-01T19-08-00.600Z.md`, `nl1.exe`, `nl1.mink` — untouched by this session |
| Live association | `HKCU\Software\Classes\.mink` → `MINK.SourceFile`, `text/x-mink`, `PerceivedType` `text`, `DefaultIcon` = `"F:\Works\Mink\npm\mink\windows\mink-file.ico",0`, no `open` verb |
| Installed npm package | `@p4inz-code/mink@1.0.2` at `C:\Users\Admin\AppData\Roaming\npm\node_modules\@p4inz-code\mink` — `bin/` and `stdlib/` only, no `windows/` |
| Brand validator | PASS, 9 checks |
| Association harness | PASS, 56 checks |

## 3. File-type inventory

The complete set, established by searching the compiler, package manager,
`stdlib/`, `examples/`, `validation/`, `benchmarks/`, the npm manifests and the
documentation:

| Extension | Purpose | Status | Windows association | Icon |
| --- | --- | --- | --- | --- |
| `.mink` | source: programs, modules, the 27 stdlib modules, tests | supported | `MINK.SourceFile`, `text/x-mink`, per-user | `mink-file.ico` (7 frames) |
| `mink.toml` | package manifest (`src/package/manifest.rs`) | supported | **none, by design** — the extension is `.toml`, shared with unrelated applications | none defined |
| `mink.lock` | generated lockfile (`src/package/lock.rs`) | supported, generated | **none, by design** | none defined |
| `.mink/` | project state directory (`src/package/env.rs`) | internal | not applicable | not applicable |
| `.exe` | `mink build` output | generated | Windows' own executable type | default executable icon |

Exactly one user-facing MINK document type exists, so exactly one association is
correct. Registering `.toml` or `.lock` would hijack every TOML file and
lockfile on a machine, and the brand kit defines no manifest/lock artwork, so no
new identity was invented. `.mink` files, `mink.toml` and `mink.lock` all open
in whatever editor the user prefers; MINK registers no `open` command for its
source.

## 4. Audit findings

1. **Portability of the icon path was the only real weakness in the shipped
   script.** Resolution was already anchored on `$PSScriptRoot` and an optional
   `-InstallDir`/`-MinkExe`, with no hard-coded drive, no current-directory
   assumption and no temporary file. What it did not do was check that the file
   it found was an ICO: any existing path was accepted, so a truncated or
   replaced file would register a `DefaultIcon` the shell cannot draw. Fixed.
2. **The validators did not cover it.** 56 checks proved registration, takeover
   safety, Unicode and space handling, idempotency and uninstall, but nothing
   covered the real package layout, a source checkout with no installation, a
   missing/empty/malformed icon, or shipped-package completeness.
3. **The live association still depended on this checkout** — not because the
   script hard-codes anything, but because the machine's only installation is
   `1.0.2`, which ships no `windows/`. That is a release dependency
   (OWNER ACTION), not something a code change can close.
4. **Explorer had only been read in its default details view.** The glyph was
   now verified visually at two view sizes and the shell bitmap was compared
   with the official artwork.

Nothing else in the branding tree was defective: both ICOs remain 7-frame
32-bpp PNG icons, byte-identical between `assets/brand/windows/ico/` and
`npm/mink/windows/`, and the 9-check brand validator was unchanged and green.
No artwork was regenerated.

## 5. Changes

| File | Change |
| --- | --- |
| `npm/mink/windows/register-mink-filetype.ps1` | `Test-IconFile` validates the ICONDIR header (reserved 0, type 1, frame count > 0) before any registry write; `Resolve-FileIcon` applies it to `-IconPath` and to both discovered candidates and distinguishes "no icon found" from "found a file that is not a valid ICO", listing the paths searched. Resolution order and all registry behaviour are unchanged. |
| `validation/windows_filetype/verify_filetype_association.ps1` | Steps 11–14 added (restore is now step 15): npm package layout (icon only in `windows\`), source checkout with no installation, four refusal cases with a from-scratch registry-signature comparison, and shipped-kit inventory (manifest ships `windows/` and `bin/mink.exe`, kit contains both ICOs + `mink.rc` + `README.md`, 7 frames each, exactly one declared extension, no `.toml`/`.lock` claim, no drive path in code, assets resolved from `$PSScriptRoot`). Also asserts the registered icon is a valid 7-frame ICO. 56 → 88 checks; no existing assertion weakened. |
| `docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md` | New §5.1 file-type inventory, §5.2 portability and icon validation, §10 validator counts and new coverage, §10.2 closure evidence, §11 limitations renumbered and extended. |
| `docs/language/WINDOWS_INSTALLATION.md` | One-extension statement, 88/88 counts, icon-resolution and stale-path notes, troubleshooting rows for a missing kit and a generic icon. |
| `npm/mink/windows/README.md` | Inventory note, icon-validation and absolute-path caveats, validator reference. |
| `docs/roadmap/IMPLEMENTATION_ROADMAP.md` | §8 dated status for this session; milestone gates unchanged. |

No compiler, language, runtime, stdlib or package-version change; no
publication, tag or release.

## 6. Verification

| Check | Result |
| --- | --- |
| `python validation/brand_assets/verify_brand_assets.py` | PASS, 9 checks (unchanged) |
| `powershell -File validation/windows_filetype/verify_filetype_association.ps1` | PASS, **88 checks / 15 steps**; machine state restored (step 15) |
| Live re-registration from the checkout, twice | exit 0 both times, identical `DefaultIcon` and extension values |
| Live `-Uninstall` | exit 0; 4 MINK-owned values removed; `.mink`, `MINK.SourceFile` and the `FileExts` entry absent; no empty key |
| Live re-registration after uninstall | exit 0; state identical to before; `shell\open` still absent |
| Explorer, details view | Type column **MINK Source File**, MINK document glyph next to a generic `.txt` page |
| Explorer, large-icon view | MINK sheet with the wordmark, no artefacts |
| Shell bitmap vs official PNG exports | 10/1024 px (32) and 5/256 px (16) differ, all at antialiased edges; 696/1024 differ from the generic text icon |
| `npm pack --dry-run` (no publish) | 1.0.5, 35 files, 5 under `windows/` |

Not verified: icon resolution from a **published installed package** on this
machine (only `1.0.2` is installed and 1.0.5 is unpublished — the harness's
temporary package tree covers that layout instead); Explorer at sizes other than
details and large icons; persistence across an MINK upgrade or uninstall
(no installer exists).

## 7. Remaining work

- **COMPLETE AND VERIFIED:** inventory; `.mink` association, icon, uninstall,
  idempotency; portable resolution logic; validators; documentation.
- **OWNER ACTION REQUIRED:** publish `@p4inz-code/mink@1.0.5`, then
  `npm install -g` and re-run the registration script so the icon resolves from
  an installed package rather than this checkout. Sending the M6 invitation
  remains the other owner action, unchanged by this session.
- **BLOCKED BY ENVIRONMENT:** real-package resolution and cross-size Explorer
  inspection on this machine.
- **DEFERRED:** an installer that registers the association automatically; an
  embedded icon resource in `mink.exe`; any editor extension; language and
  ecosystem work, which stays on hold.

## 8. Delivery

Committed on `main` as one commit on top of `d2eeb6169908c5330ca85706f148928e81e450e6`
with an explicit file list (no `git add .`/`-A`, no clean, reset or force-push),
then pushed to `origin/main` with local `HEAD` and `origin/main` verified equal.
The four user-owned untracked files were not staged, and no npm publish, tag or
release was created.
