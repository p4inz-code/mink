# Session 94 — Windows Real-World CLI / User Workflow Quality Audit

Date: 2026-09-04. Branch `main`, starting commit `2d841fd`. MINK v1.0.1.

## Scope

No Linux HTTP work, no C ABI / interop / package-manager / registry work. This
session audits and hardens the Windows user workflow: installing MINK,
checking/building/running real `.mink` files from a clean directory, version
aliases, error quality, path handling, and the npm package.

## Final classification

**WINDOWS CLI / USER WORKFLOW = STABLE** — verified end-to-end from a clean
directory outside the repository and from a fresh npm install of the packaged
artifact; all discovered issues fixed or explicitly classified below.

## Findings fixed

| # | Severity | Finding | Fix |
|---|----------|---------|-----|
| 1 | P1 | `mink build <directory>` failed **silently** (exit 1, no output): a directory argument made module discovery synthesize a "module file not found" error on a phantom `SourceId` that no `SourceMap` can render | New `BuildError::NotAFile`; `mink check/build/run <dir>` now reports `'<dir>' is a directory, not a MINK source file` |
| 2 | P2 | `mink check` lacked the per-variant error arms `build`/`run` have, so check-time failures printed only "N front-end error(s)" without the diagnostics | Added `FrontEnd`/`Backend` arms to the `check` command |
| 3 | P2 | `mink -v` reported "unknown command" (only `-V`/`--version`/`version` worked) | `-v` is now a version alias (node-style); all four forms agree (`mink 1.0.1`) |
| 4 | P3 | `mink help build` silently ignored its argument and printed generic help; `mink version extra` silently succeeded | Trailing arguments are now rejected for `help`/`version`/`explain` |
| 5 | P3 | `mink explain` with no code errored, yet the unknown-code hint said "run `mink explain` without arguments to see available codes" | `mink explain` with no code now lists all 35 documented error codes (exit 0); the hint is now truthful |
| 6 | P3 | Stale docs: `tests/filesystem_lib.rs` claimed FS wrappers "can cause crashes when called in sequence"; README claimed "All commands accept `--help` and `--version` flags" (subcommands reject options) | Doc comments and README corrected; wrapper-in-sequence behaviour locked by a new regression test |

## Verified working (evidence from clean temp dirs, no repo/build tools in play)

- Version: `mink version` / `-v` / `-V` / `--version` all → `mink 1.0.1`, exit 0.
- Help: `mink` (no args), `mink help`, `-h`, `--help` → usage, exit 0.
- Real files: check/build/run of `hello.mink` (prints + exit 0), `calc.mink`
  (arithmetic/conditionals/loops/functions), `stringprog.mink` (string build +
  compare under V1 move ownership), `fsproj/main.mink` (a real filesystem
  project: `mod filesystem` + `fs_write`/`fs_read` round-trip, byte-verified).
- Error handling: missing file → `failed to read '<file>'`; directory →
  clear message; empty file → check passes / build reports E-B08 no-main;
  malformed syntax → E-P03 with `--> file:line:col`; unknown command/option,
  missing args, extra args, unknown target → clear usage errors. All exit
  non-zero; nothing panics, hangs, or prints internal details.
- Paths: relative, `./`, absolute, `..`, backslash and forward-slash absolute,
  and a `spaces dir` path — all build and run correctly. Generated `hello.exe`
  runs directly and standalone.
- npm: fresh directory, `npm install <p4inz-code-mink-1.0.1.tgz>`; shims
  (`mink`, `mink.cmd`, `mink.ps1`) created; `mink --version`, check, build,
  run all work through the shim, through `cmd.exe` via `mink.cmd`, and by
  invoking the packaged exe directly. The bundled `bin/mink.exe` was refreshed
  from the current release build (it was stale: dated before Sessions 92-94)
  and the tarball re-packed.

## Intentional decisions / deferred

- `mink build --help` still reports "unknown option": options are parsed only
  where a command defines them; the documented path for help is `mink help`.
  Classified P3 (documented in README), not changed.
- `explain` catalog listing is flat (35 codes) rather than per-category; P3.
- npm global install (`npm install -g`) was not exercised (writes outside the
  sandbox); the local-install + shim + `mink.cmd` paths were, which exercise
  the same wrapper resolution.
- Windows env_get/env_has confirmed unwired to the Windows API (README's V1
  stub claim is accurate) — verified by probe.
- Linux HTTP POST etc. deliberately out of scope (Session 93 follow-ups).

## Quality gates

- `cargo fmt --check` — clean.
- `cargo clippy --all-targets --all-features` — no warnings in files touched
  this session; remaining warnings pre-existing in untouched files.
- `cargo test` — full suite **2520 passed / 0 failed** (2514 baseline + 6 new:
  5 CLI regression tests, 1 fs wrapper-sequence test).
- `cargo build` / `cargo build --release` — clean.

## Files changed

`src/cli.rs`, `src/driver.rs`, `tests/cli.rs`, `tests/filesystem_lib.rs`,
`README.md`, `npm/mink/bin/mink.exe` (refreshed release binary).
