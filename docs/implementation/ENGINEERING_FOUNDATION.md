# MINK — Engineering Foundation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 01 — Repository + Compiler Engineering Foundation

## 1. Implementation Language

**Decision:** the MINK compiler is implemented in **Rust**.

No specification document prescribes an implementation language, so the
choice follows the evaluation criteria in `docs/core/DESIGN_DECISION_RULES.md`
and `docs/core/MINK_MASTER_SPEC.md` §14:

| Criterion                  | Rust                                                       |
| -------------------------- | ---------------------------------------------------------- |
| Safety                     | Memory-safe by default; `unsafe_code` forbidden in sources |
| Compiler ecosystem quality | rustc/LLVM, rowan, codespan-reporting, lalrpop, etc.       |
| Performance                | Native speed; data-race-free parallelism for parallel compilation |
| Cross-platform support     | First-class Windows / macOS / Linux support                |
| Developer tooling          | cargo, rustfmt, clippy, cargo test, built-in doc generation |
| Long-term maintainability  | Strong typing, exhaustive enums, no GC; rustc itself proves the model |
| Build/release reliability  | Deterministic builds via committed `Cargo.lock`; reproducible releases |

The compiler requirements in `docs/compiler/COMPILER_ARCHITECTURE.md`
(incremental compilation, parallel compilation, HIR/MIR-style IRs,
structured diagnostics, LSP integration) align closely with rustc's own
architecture. Cargo was already anticipated by `.gitignore` (`/target/`).

Toolchain in use: Rust 1.97.1, edition 2024. Declared MSRV floor: 1.85
(the minimum required by edition 2024).

## 2. Dependency Policy

The compiler foundation has **zero third-party dependencies**. Dependencies
are introduced only when a subsystem genuinely needs them — for example, a
CLI argument parser when the CLI grows, or LLVM bindings when the backend is
chosen. This follows the anti-overengineering rule of
`docs/roadmap/IMPLEMENTATION_ROADMAP.md` §16 and keeps the build
deterministic and auditable.

## 3. Repository Layout

```
Cargo.toml            Package manifest (library + binary)
Cargo.lock            Committed for deterministic builds
README.md             Project overview and developer instructions
docs/                 Language and architecture specifications (planning baseline)
docs/implementation/  Engineering decisions (this document)
src/                  Compiler implementation
tests/                Integration and unit tests
```

## 4. Compiler Crate Structure

A single Cargo package `mink` with a library (`src/lib.rs`) and a thin
binary (`src/main.rs`). The library exposes the compiler as a reusable API;
the binary is only the process entry point. A single crate keeps the
foundation simple; subsystems can be split into workspace crates once their
interfaces stabilize.

| Module          | Role                                                              |
| --------------- | ----------------------------------------------------------------- |
| `cli`           | Argument parsing and command dispatch (build/check/run/test/fmt)  |
| `driver`        | Compiler pipeline orchestration (load + lexical check; parser pending) |
| `source`        | Source files, ids, spans, line/column mapping (implemented)       |
| `lexer`         | Tokenization with accurate spans + lexical diagnostics (implemented — see LEXER_IMPLEMENTATION.md) |
| `parser`        | Placeholder — syntax tree with error recovery                     |
| `ast`           | Placeholder — typed syntax representation                         |
| `semantics`     | Placeholder — name resolution, type checking, semantic rules      |
| `diagnostics`   | Placeholder — structured diagnostic engine (§10 of the compiler spec) |

## 5. Source Infrastructure Design

- **Positions** are byte offsets into UTF-8 source text (`u32`), the
  standard currency for compiler internals; no memory is wasted on per-byte
  metadata.
- **Lines and columns** are 1-based. Columns are byte-based (a multi-byte
  character occupies multiple columns); character-based columns can be
  layered onto `LineIndex` when the diagnostic system is implemented.
- **`SourceId`** values are assigned sequentially by `SourceMap` and remain
  stable for the lifetime of the map.
- **`Span`** is a half-open byte range `[start, end)` tied to one
  `SourceId`. An empty span marks a point location. `join` produces the
  covering range of two spans from the same file.
- **`LineIndex`** records line-start offsets at construction (O(n)) and
  answers line/column queries by binary search (O(log n)).

## 6. Development Commands

| Task                 | Command                                |
| -------------------- | -------------------------------------- |
| Build                | `cargo build`                          |
| Run the compiler     | `cargo run -- --version`               |
| Test                 | `cargo test`                           |
| Format               | `cargo fmt` / `cargo fmt --check`      |
| Lint                 | `cargo clippy --all-targets -- -D warnings` |

Compiler exit codes: `0` success, `1` usage or input error, `2` command not
yet implemented.

## 7. Deferred Decisions

- Toolchain pinning via `rust-toolchain.toml`
- CI configuration
- `LICENSE` file text (metadata declares Apache-2.0; `publish = false`)
- Character-based columns
- Workspace/crate split once subsystem interfaces stabilize
- Placement and format of AI-readable diagnostics once the diagnostic
  engine lands (per `docs/ai/AI_TOOLING_ARCHITECTURE.md`)
- Lexer-specific deferrals (Unicode identifiers, byte literals, fuzz
  harness, doc-comment metadata) are tracked in
  `docs/implementation/LEXER_IMPLEMENTATION.md` §8
