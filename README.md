# MINK Programming Language

Created by Atharva Patil / p4inz-code.
Stewarded by Northbyte Studios.

MINK is an ambitious general-purpose programming language designed around four pillars:

- Speed
- Less Errors
- Durability
- Flexibility

Source code is licensed under Apache License 2.0.

## Status

MINK is in the **implementation phase**. The compiler engineering foundation
(session 01) is established: a Rust-based compiler workspace with source
infrastructure, a working CLI entry point, and a test suite. Language and
architecture specifications live in [`docs/`](docs/).

## Repository Layout

- `docs/` — language and architecture specifications, plus engineering decisions
- `src/` — compiler implementation (Rust)
- `tests/` — compiler tests

## Developer

Requirements: Rust 1.85 or newer (developed against 1.97).

| Task                 | Command                                          |
| -------------------- | ------------------------------------------------ |
| Build                | `cargo build`                                    |
| Run the compiler     | `cargo run -- --version`                         |
| Test                 | `cargo test`                                     |
| Format               | `cargo fmt` / `cargo fmt --check`                |
| Lint                 | `cargo clippy --all-targets -- -D warnings`      |

Examples:
- `cargo run -- check path/to/program.mink` loads a MINK source file and runs
  lexical analysis (exit 0 when the file lexes cleanly, exit 1 with
diagnostics otherwise).
- `cargo run -- build path/to/program.mink` loads a MINK source file and runs
the (not yet implemented) build pipeline.

See [`docs/implementation/ENGINEERING_FOUNDATION.md`](docs/implementation/ENGINEERING_FOUNDATION.md)
for engineering decisions and the compiler subsystem layout.
