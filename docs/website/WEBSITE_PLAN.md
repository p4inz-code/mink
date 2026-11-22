# Plan: the MINK website on GitHub Pages

**Status:** Plan. The owner has chosen the address and the look (2026-11-22). Nothing here is built yet.

## Goal

A fast, plain site that makes a visitor understand MINK in ten seconds and install it in thirty. The
model is the feel of nodejs.org: a big plain headline, two obvious download buttons, a real code sample,
and short sections. Same pattern, our own look.

## Look

- Taken from the MINK design system (`assets/brand/`): black and white with grey. The palette is Ink
  `#1C1D21`, White `#FFFFFF`, Fold `#3A3C44`, Edge `#45474F` and Paper `#F4F4F2`. No colour accent.
- The owner's direction: a little grey gradient, like liquid metal or glass. That goes on surfaces only:
  a soft brushed-metal sheen on cards and buttons, glass-style panels with a thin light edge, and a
  faint highlight that follows the pointer.
- The logo itself is never given gradients, outlines or shadows. The brand rules forbid it, so the logo
  stays flat, as supplied.
- The white MINK symbol sits behind the hero as a faint watermark, as in the setup window.
- Fonts: Atkinson Hyperlegible Next for text, Familjen Grotesk for headings, bundled with the site.
- Motion: slow and small, and off when the visitor asks for reduced motion.
- Works with no JavaScript for reading. No trackers, no cookies, no third-party requests.
- Readable at large text sizes and on a phone. Contrast checked against WCAG AA.

## Pages

| Page | What is on it |
|---|---|
| **Home** | Headline, one sentence, two buttons (Download for Windows, Read the docs), a real program with its output beside it, six short feature blocks, a line about the license and credit |
| **Download** | Setup file, portable zip and `npm i -g`, each with its SHA-256, a note that files are unsigned for now, and how to check a hash |
| **Learn** | Getting started in ten minutes, the language basics, the standard library reference, from the docs already in the repository |
| **Examples** | The twelve test apps once they exist, each with source, a screenshot or output, and run instructions |
| **Tools** | `mink fmt`, `mink lsp`, the VS Code extension, file association, `mink setup` |
| **Why MINK** | `docs/WHY_MINK.md`, including what MINK does not claim |
| **Releases** | The changelog |
| **About** | Atharva Patil, P4inz Interactive Labs, license, how to report a problem |

### Home page, top to bottom

1. Headline: what MINK is, in plain words. No "blazing", no "revolutionary".
2. Download buttons with the current version and Windows 10 and newer.
3. Code sample (the 55 program) beside the commands that build and run it.
4. Six blocks: one small `.exe`; clear errors; one format; editor support; Windows file support; a
   standard library with files, JSON, SQLite, HTTP and threads.
5. "Honest limits": Windows only, young, you manage memory, unsigned for now.
6. Footer: GitHub, releases, issues, license, credit.

## How it is built

- Static HTML and CSS, written by hand at first. The docs pages come from the Markdown in `docs/`.
- When the `sitegen` test app exists, the site is built with it, so MINK builds its own website.
- Lives in a `site/` folder on the main branch, published with a GitHub Actions workflow to GitHub
  Pages. The address is `https://p4inz-code.github.io/mink/` until there is a custom domain.
- Pages is not switched on for the repository yet. The repository owner turns it on in Settings, or we
  add the workflow and it deploys once enabled.
- Download links point at GitHub Releases. Hashes are copied from the release files by a script, never
  typed by hand.

## Content rules

- Every claim on the site is something we ran. No speed claims until `bench` has numbers.
- No stock phrases or hype. Short sentences, plain words.
- Text goes through the same plain-language check as the rest of the project.
- Say clearly that there is no browser or mobile target.

## Steps

| Step | Work | Done when |
|---|---|---|
| W1 | Design tokens, fonts, header, footer, home page | Home page looks right on a laptop and a phone |
| W2 | Download page with real hashes from the release | Every link works and every hash matches |
| W3 | Learn pages built from `docs/` | Navigation works with no JavaScript |
| W4 | Why MINK, Tools, Releases, About | All pages reviewed for accuracy |
| W5 | GitHub Actions deploy, search-engine and sharing tags, a social preview image | The site is live and shares well on Discord |
| W6 | Examples page | Filled in as test apps are finished |

## Decided

- Address: `https://p4inz-code.github.io/mink/` for now.
- Look: the MINK design system, black and white with a liquid-metal and glass feel on surfaces.

## Still open

1. English only?
2. Turn on GitHub Pages in the repository settings yourself, or should I try with the GitHub command line?
3. Should the setup window move to the same palette? It uses a teal accent today, which is not in the MINK palette.

## Phase 2: what makes the site worth sharing (2026-11-22)

The home page is done and built by a MINK program. These are the next items, in order. Each one has a
"done when" that can be checked.

| # | Item | Why | Done when |
|---|---|---|---|
| P1 | **Copy buttons** on code samples, commands and hashes | Expected on a developer site | Works with a keyboard and a touch screen; the page still works with JavaScript off |
| P2 | **Deploy workflow, WOFF2 fonts, lighter watermark, font preload** | Speed and a live address | Pages builds from `main`; home page transfers under 300 KB; Lighthouse performance 95 or more |
| P3 | **Hosted Learn pages**: getting started, language basics, standard library | The docs should not send visitors to GitHub Markdown | Built by `sitegen` from `docs/`; side navigation; every internal link checked |
| P4 | **Terminal demo**: a 20-second replay of a real session | The most memorable thing on a language site | Text taken from a real run; pause, replay and skip; no outside player; respects reduced motion |
| P5 | **What's new and roadmap strip** | Shows the project moves | Release notes pulled from `CHANGELOG.md`; roadmap lists only what is planned |
| P6 | **Real screenshots**: setup window, Explorer icon and right-click menu, VS Code | Proof beyond mockups | Taken after the clean-install test; no mockups left |
| P7 | **Examples page** | The twelve test apps as proof | One card per finished app with source, output and run steps |
| P8 | **Benchmarks page** | The first thing developers ask for | Real numbers from `bench`, losses included; method and machine stated |
| P9 | **"Why not Go, Rust or Zig?" FAQ** | People will ask | Honest, and says when to pick the other language |
| P10 | **Brand and press page** | Professional, and protects the credit | Logo downloads and the usage rules from `assets/brand/` |
| P11 | **Release feed (RSS), GitHub Discussions on** | Somewhere to follow and to ask | `feed.xml` builds from the changelog; Discussions enabled by the owner |
| P12 | **winget** in the install options | One command for Windows users | Manifest accepted by `winget-pkgs` |
| P13 | **Accessibility and no-JavaScript audit** | A top-tier site must work for everyone | Lighthouse accessibility 100; every page usable with JavaScript off; checked at phone, tablet and desktop widths |
| P14 | **Hash and size automation** | Numbers can never go stale | `tools/build_site.ps1` reads them from the release files |

Order: P2, P1, P14 (quick), then P3 and P4, then P5. P6, P7 and P8 wait for the clean install, the apps and
`bench`. P9 to P12 fill in around them.

Rules: nothing on the site that was not run; no speed claims before P8; plain language throughout.
