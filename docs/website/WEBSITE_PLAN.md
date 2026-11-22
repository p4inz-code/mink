# Plan: the MINK website on GitHub Pages

**Status:** Plan, for the owner to approve. Nothing here is built yet.

## Goal

A fast, plain site that makes a visitor understand MINK in ten seconds and install it in thirty. The
model is the feel of nodejs.org: a big plain headline, two obvious download buttons, a real code sample,
and short sections. Same pattern, our own look.

## Look

- Black background with the white MINK symbol as a faint watermark, as in the setup window.
- Fonts: Atkinson Hyperlegible Next for text, Familjen Grotesk for headings, bundled with the site.
- One accent, teal `#4fd1d6`. 8px corners, hairline borders, no gradients, no animation beyond hover.
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

## Decisions needed from the owner

1. Address: GitHub Pages address for now, or a custom domain?
2. Keep the teal accent, or use another colour?
3. Is the site content in English only?
4. Turn on GitHub Pages for the repository yourself, or should I try with the GitHub command line?
