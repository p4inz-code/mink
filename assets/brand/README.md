# MINK brand assets

Official MINK identity artwork, integrated from the supplied `MINK UI SETS OFFICIAL`
design package (2026-10-25). These files are the repository's authoritative brand
sources. Nothing here is produced by the build, and nothing here is shipped to npm
users except the Windows file-type registration kit in
[`npm/mink/windows/`](../../npm/mink/windows/) — see
[`docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md`](../../docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md)
for the full integration and validation record.

## Layout

| Path | Contents | Class |
| --- | --- | --- |
| `masters/svg/mink-master.svg` | symbol + wordmark as named groups | source master |
| `masters/vector/mink-geometry.json` | raw paths and colours | source master |
| `logo/primary/` | stacked lockup, SVG + @2x PNG, three colourways | production |
| `logo/horizontal/` | horizontal lockup, SVG + @2x PNG, three colourways | production |
| `logo/compact/` | compact lockup, SVG + @2x PNG, three colourways | production |
| `logo/symbol/` | symbol outline, optical-small and silhouette variants, 1024 PNG and a 16–512 PNG ladder | production |
| `logo/wordmark/` | wordmark only, SVG + @2x PNG, three colourways | production |
| `on-background/` | primary logo on its intended background (dark, light, single colour) | production |
| `icon/application/` | `mink.exe` icon: master SVG, per-size SVGs 16–128, PNG frames 16–256 | production |
| `icon/source-file/` | `.mink` file icon: master, per-size SVGs 16–128, one-colour variant, PNG frames 16–1024 | production |
| `windows/ico/` | multi-size ICO used by Windows (`mink-app.ico`, `mink-file.ico`) | production |
| `web/github/` | README header and avatar, SVG + PNG | production (this repository) |
| `web/social/` | social preview, profile avatar, docs header | production (GitHub/web settings) |
| `previews/` | size ladders and an Explorer mock on light and dark | preview only |

## Colourways

- `-white` — for dark backgrounds
- `-ink` — `#1C1D21`, the default for light backgrounds
- `-black` — `#000000`, for print and one-colour production

Palette: Ink `#1C1D21` · White `#FFFFFF` · Fold `#3A3C44` · Edge `#45474F` ·
Paper `#F4F4F2`.

## Size rules

- Symbol outline: 128 px and up. Filled silhouette: 64 px and below. Optical
  (thickened) outline: 64–128 px — use `logo/symbol/mink-symbol-optical-small-*.svg`.
- The source-file icon shows the wordmark only at 128 and 256. At 16–32 the artwork is
  a pixel grid, so scale those SVGs by whole numbers only.
- Horizontal lockup minimum width: 96 px / 20 mm. Clear space is the wordmark
  cap-height on all sides and is already built into the SVG bounds.

## Rules

Do not stretch, rotate or recolour parts, add outlines, shadows, gradients or effects,
or set "MINK" in another font. The artwork in this tree is used as supplied.

## Where the artwork is used

- `README.md` uses `web/github/mink-readme-header-{light,dark}.png` through a
  `<picture>` element, so the header follows the reader's colour scheme.
- GitHub repository settings are manual and external: upload
  `web/social/mink-social-preview-dark-1280x640.png` as the social preview and
  `web/github/mink-avatar-dark-500.png` as the organisation/account avatar. These are
  not applied by any script in this repository.
- Windows uses `windows/ico/mink-file.ico` for the `.mink` file type; the shipped copy
  lives in `npm/mink/windows/`. `mink-app.ico` is the application icon.

## Not stored here

- **Rejected app-icon alternatives.** The design package also contains
  `extras/app-icon-alternatives/option-b` and `option-c` (a paper tile, and a tile with
  a prompt rule). They are not production artwork and are deliberately not in this
  tree; the chosen icon is `icon/application/mink-app-master.svg`.
- **Website favicons.** `web/favicon/` in the design package is for a MINK website.
  This repository has no website, so those files are not duplicated here; the favicon
  set is derived from the application icon and belongs to the website project.
- **Redundant exports.** The design package repeats several files across
  `logo/monochrome/`, `windows/png/` and `web/favicon/`. Each artwork is stored here
  once, in the directory that owns it: `logo/*` colourway files are the single-colour
  variants, `icon/*/png/` are the frames the ICOs were built from, and
  `windows/ico/` holds only the ICO containers. `mink-app-master.svg` is the 256
  artboard, so no separate `mink-app-256.svg` is kept.

## Regenerating

The SVGs are the source of truth. ICO and PNG frames were produced from the per-size
SVGs, and the master SVGs carry the geometry for the larger sizes. Regenerate raster
artwork from `masters/` and `icon/*/mink-*-master.svg` and re-run
`validation/brand_assets/verify_brand_assets.py`, which checks every SVG, PNG and ICO
in this tree for structural validity and expected dimensions.
