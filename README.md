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
HIR, MIR, an optimization pipeline, and a first **native backend** are
established in a Rust-based compiler workspace with source infrastructure, a
working CLI entry point, and a test suite (622 tests).

The compiler currently processes source through the following pipeline:

```
Source → Lexer → Parser → AST → Semantic Analysis → Type Analysis
    → HIR → MIR → Optimization → Native Backend → Executable
```

`mink check` validates, lowers, and optimizes programs through MIR, and
`mink build` compiles the optimized MIR into a native executable for the
first target: `x86_64-windows-pe` (a self-contained x86-64 code generator
that assembles a complete PE image with no external toolchain). The first
native subset covers integers, booleans, comparisons, logical and bitwise
operators, `if`/`while`/`for`/`loop` control flow, direct function calls,
module bindings, and integer results becoming process exit codes. Everything
outside the subset (floating point, strings, …) is rejected with structured
`E-B01+` diagnostics instead of being miscompiled. The language and
architecture specifications live in [`docs/`](docs/); the frozen core grammar
is in [`docs/language/CORE_GRAMMAR.md`](docs/language/CORE_GRAMMAR.md).

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
  semantic, type, HIR, MIR, and MIR-optimization analysis. Exit 0 when the
  program validates and optimizes through MIR, exit 1 with diagnostics
  otherwise.
- `mink build <path> [--target <triple>]` — loads a MINK source file and
  compiles the optimized MIR into a native executable for the selected
  target (default `x86_64-windows-pe`). Exit 0 with a success message when
  the executable is written; exit 1 with diagnostics for front-end errors,
  structured `E-B01+` backend errors, an unrecognized target, or an
  output-write failure.
- `mink run`, `mink test`, `mink fmt` — recognized but not yet implemented
  (exit 2).
- `mink version` / `mink help` — version and usage.

See [`docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`](docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md)
for the native backend design and the supported subset.

See [`docs/implementation/ENGINEERING_FOUNDATION.md`](docs/implementation/ENGINEERING_FOUNDATION.md)
for engineering decisions and the compiler subsystem layout, and
[`docs/compiler/COMPILER_ARCHITECTURE.md`](docs/compiler/COMPILER_ARCHITECTURE.md)
for the compiler architecture.
