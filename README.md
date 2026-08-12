# MINK Programming Language

Created by Atharva Patil / p4inz-code.
Stewarded by Northbyte Studios.

MINK is an ambitious general-purpose programming language designed around four pillars:

- Speed
- Less Errors
- Durability
- Flexibility

Source code is licensed under Apache License 2.0 (see [`LICENSE`](LICENSE)).

## Status

MINK is in the **implementation phase**. The compiler engineering foundation,
lexer and token system, parser and AST, semantic analysis, type inference,
HIR, and MIR are established in a Rust-based compiler workspace with source
infrastructure, a working CLI entry point, and a test suite (537 tests).

The compiler currently validates source through the following pipeline:

```
Source → Lexer → Parser → AST → Semantic Analysis → Type Analysis
    → HIR → MIR
```

There is **no code generation, runtime, or executable output yet**:
`mink check` validates and lowers programs through MIR, and `mink build` is
not yet implemented. The language and architecture specifications live in
[`docs/`](docs/); the frozen core grammar is in
[`docs/language/CORE_GRAMMAR.md`](docs/language/CORE_GRAMMAR.md).

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

### CLI usage

- `mink check <path>` — loads a MINK source file and runs lexical, syntactic,
  semantic, type, HIR, and MIR analysis. Exit 0 when the program validates
  through MIR, exit 1 with diagnostics otherwise.
- `mink build <path>` — loads a MINK source file and reports that the build
  pipeline is not yet implemented (exit 2).
- `mink run`, `mink test`, `mink fmt` — recognized but not yet implemented
  (exit 2).
- `mink version` / `mink help` — version and usage.

See [`docs/implementation/ENGINEERING_FOUNDATION.md`](docs/implementation/ENGINEERING_FOUNDATION.md)
for engineering decisions and the compiler subsystem layout, and
[`docs/compiler/COMPILER_ARCHITECTURE.md`](docs/compiler/COMPILER_ARCHITECTURE.md)
for the compiler architecture.
