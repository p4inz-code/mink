# MINK package registry: specification

**Status:** Specification only. Not implemented in 1.0.5.

Today packages install from local folders into `.mink/packages`. A registry would let
`mink install name` find one online.

## Goals

- Install a package by name and version with one command.
- Every download is checked, and every package keeps its creator's credit.
- Nothing runs on install. No install scripts.

## Shape

- A static index served over HTTPS. Each package version has a page with its metadata and a download
  of one `.minkpkg` archive (see [FILE_TYPES_SPEC.md](FILE_TYPES_SPEC.md)).
- Names are lowercase letters, digits and hyphens. A name is first come, first served, and cannot be
  reused after removal.
- Versions follow `major.minor.patch`. A published version cannot be changed. It can be yanked, which
  hides it from new installs but keeps it available to anyone who already depends on it.

## Commands

| Command | What it does |
|---|---|
| `mink install <name>[@version]` | Downloads, verifies and installs a package |
| `mink publish` | Packs and uploads the current package (needs a login) |
| `mink search <text>` | Searches names and descriptions |
| `mink yank <name>@<version>` | Hides a version from new installs |

## Safety rules

1. Every archive's SHA-256 must match the index before it is opened.
2. A lock file (`mink.lock`) records the exact version and hash of every dependency, so a build is
   repeatable.
3. Archives with paths outside the package folder are refused.
4. The registry stores the license of each package and `mink install` shows it before installing.
5. Publishing needs a signed-in account. Tokens are stored in the user's profile and never printed.

## Not decided

- Who hosts it, and whether the first version is a GitHub-backed index.
- Account and token design.
- How to handle abandoned or malicious packages beyond yanking.
