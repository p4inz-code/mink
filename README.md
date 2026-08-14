# MINK Programming Language

Created by Atharva Patil / p4inz-code.
Stewarded by Northbyte Studios.

MINK is an ambitious general-purpose programming language designed around four pillars:

- Speed
- Less Errors
- Durability
- Flexibility

Source code is licensed under Apache License 2.0 (see [`LICENSE`](LICENSE)).

## What is MINK?

MINK is a compiled, general-purpose programming language being built from the
ground up: its own lexer, parser, type system, intermediate representations
(HIR/MIR), optimizer, native code generator, and runtime. It is designed for
systems programming, backend development, and application development — with
a strong emphasis on catching errors early and on durable, predictable
behavior.

## What problem does MINK solve?

MINK targets the tension between low-level performance and high-level safety:
a language fast enough for systems work while making whole classes of errors
hard to write. The project is building the compiler and runtime in the open,
from first principles, so the language model is validated by real
implementation rather than by marketing.

## Status

MINK is in the **implementation phase**. The compiler engineering foundation,
lexer and token system, parser and AST, semantic analysis, type inference,
HIR, MIR, an optimization pipeline, a first **native backend**, and a
**native runtime foundation** (deterministic heap, `rt_*` memory/print
intrinsics, structured runtime diagnostics) are established in a Rust-based
compiler workspace with source infrastructure, a working CLI entry point,
and a test suite (654 tests).

The compiler currently processes source through the following pipeline:

```
Source → Lexer → Parser → AST → Semantic Analysis → Type Analysis
    → HIR → MIR → Optimization → Backend → Runtime → Native Binary
```

The backend assembles a complete x86-64 Windows PE image with **no external
toolchain** (no C compiler, assembler, or linker), embedding the runtime
(process initialization, deterministic heap, structured `E-R01+`
diagnostics) directly into every executable.

## Current capabilities

- **Pipeline**: parsing, semantic analysis, type checking/inference, HIR and
  MIR lowering, deterministic optimization passes (boolean constant folding,
  copy propagation, CFG simplification, unreachable-block elimination,
  dead-code elimination), native code generation, and an embedded runtime.
- **Language subset**: integers, booleans, comparisons, logical and bitwise
  operators, `if`/`while`/`for`/`loop` control flow, direct function calls,
  module bindings, and integer results becoming process exit codes.
- **Runtime intrinsics**: `rt_alloc`, `rt_free`, `rt_mem_load`,
  `rt_mem_store` (validated against a bounded liveness table), `rt_exit`, and
  `rt_print_int` — backed by a deterministic bump/free-list heap that reports
  structured `E-R01+` diagnostics on invalid memory operations.
- **Target**: `x86_64-windows-pe` — a self-contained code generator and PE
  container builder producing runnable Windows executables.
- **Diagnostics**: everything outside the supported subset (floating point,
  strings, …) is rejected with structured `E-B01+` backend diagnostics
  instead of being miscompiled.

## Current limitations

- **Single target**: `x86_64-windows-pe` is the only implemented target;
  `x86_64-linux-elf` and `aarch64-linux-elf` are recognized but rejected
  (`E-B11`).
- **Fixed 1 MiB heap**: the runtime arena is a fixed size; exhaustion is a
  structured error (`E-R02`).
- **Single-threaded runtime**: no concurrency primitives yet.
- **No strings/structs/arrays**: `rt_mem_*` operate on raw 8-byte words; the
  memory-layout groundwork exists but no aggregate types consume it yet.
- **No ownership/borrow checking**: deliberately deferred — the memory model
  is established so later safety features have a stable foundation.
- **No garbage collector**: allocation is explicit and leak-checked on exit.
- **Limited native subset**: no floating point, strings, characters, `null`,
  member/index places, or function values in the native backend yet.
- **No stdlib, package manager, or tooling** (`mink run`/`test`/`fmt` are
  recognized but not yet implemented).

## Roadmap

The long-term plan is documented in
[`docs/roadmap/IMPLEMENTATION_ROADMAP.md`](docs/roadmap/IMPLEMENTATION_ROADMAP.md)
— memory/ownership, standard library, package/build system, developer
tooling, web/backend and desktop ecosystems, optimization, security
hardening, and release engineering. Future work is intentionally **not**
claimed as implemented; the sections above describe only what exists today.

## Repository Layout

- `docs/` — language and architecture specifications, plus engineering decisions
- `src/` — compiler implementation (Rust)
- `tests/` — compiler tests

## Documentation

- [`docs/implementation/`](docs/implementation/) — implementation records for
  every stage (lexer, parser, semantic analysis, type system and inference,
  HIR, MIR, optimization, native backend, runtime).
- [`docs/compiler/COMPILER_ARCHITECTURE.md`](docs/compiler/COMPILER_ARCHITECTURE.md)
  — compiler architecture and pipeline.
- [`docs/language/`](docs/language/) — language specifications; the frozen
  core grammar is in [`docs/language/CORE_GRAMMAR.md`](docs/language/CORE_GRAMMAR.md).
- [`docs/core/`](docs/core/) — master specification and design rules.
- [`docs/runtime/`](docs/runtime/) — runtime, memory, and concurrency model
  planning.
- [`docs/roadmap/`](docs/roadmap/) — implementation roadmap.

The native backend design and supported subset are in
[`docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`](docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md);
the native runtime foundation and memory model are in
[`docs/implementation/RUNTIME_IMPLEMENTATION.md`](docs/implementation/RUNTIME_IMPLEMENTATION.md).

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

See [`docs/implementation/ENGINEERING_FOUNDATION.md`](docs/implementation/ENGINEERING_FOUNDATION.md)
for engineering decisions and the compiler subsystem layout.
