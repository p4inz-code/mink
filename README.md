<div align="center">

# MINK

**A general-purpose programming language — built from first principles.**

*Fast. Fewer errors. Durable. Flexible.*

![License: Apache 2.0](https://img.shields.io/badge/license-Apache%202.0-blue)
![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange)
![Status](https://img.shields.io/badge/status-implementation-yellow)
![Target](https://img.shields.io/badge/target-x86_64--windows--pe-lightgrey)

Created by [Atharva Patil / p4inz-code](https://github.com/p4inz-code). Stewarded by Northbyte Studios.

</div>

---

## What is MINK?

MINK is a compiled, general-purpose programming language being built **from
the ground up** — its own lexer, parser, type system, intermediate
representations (HIR/MIR), optimizer, native code generator, and runtime.
Nothing is borrowed from another language's toolchain: the compiler
assembles a complete native executable with **no external toolchain** (no C
compiler, assembler, or linker).

It is designed for systems programming, backend development, and application
development — with a strong emphasis on catching errors early and on durable,
predictable behavior.

## Why MINK?

| Pillar        | What it means                                                                 |
| ------------- | ----------------------------------------------------------------------------- |
| ⚡ **Speed**      | Native performance, no garbage collector, small and deterministic runtime. |
| 🛡️ **Less Errors** | Invalid memory operations are detected and reported with structured diagnostics instead of silently corrupting memory. |
| 🏗️ **Durability**  | A stable, documented memory model and architecture designed so safety features can be layered on later. |
| 🧩 **Flexibility** | A general-purpose language for systems, backend, and application work. |

The project is built **in the open** — the language model is validated by a
real, working compiler and runtime, not by marketing.

## A taste of MINK

```mink
fn main() {
    let mut total = 0;
    for i in 1..=10 {
        total = total + i;
    }
    rt_print_int(total);  // prints: 55
    return total % 2;     // exit code: 1
}
```

```console
$ mink build demo.mink
mink: build: 'demo.mink' -> 'demo.exe' (target: x86_64-windows-pe, 1 function(s), 0 binding(s))

$ ./demo.exe
55
$ echo $?
1
```

MINK ships strings and typed pointers on top of the deterministic,
leak-checked runtime heap:

```mink
fn main() {
    let p = rt_alloc(24);          // zero-initialized heap block
    rt_mem_store(p, 7);
    rt_mem_store(p + 8, 35);
    let x = rt_mem_load(p);
    let y = rt_mem_load(p + 8);
    rt_print_int(x * 10 + y);      // prints: 105
    rt_free(p);
    return 0;
}
```

```mink
fn main() {
    let s = rt_str_alloc(5);
    rt_str_set_byte(s, 0, 104);
    rt_str_set_byte(s, 1, 105);
    rt_str_set_byte(s, 2, 33);
    rt_print_str(s);              // prints: hi!
    rt_str_free(s);
    rt_print_str("done");          // string literals are immutable byte data
    return 0;
}
```

Access a freed or never-allocated block and the runtime traps with a
structured `E-R05` diagnostic; index past a string's end and it traps with
`E-R09`; index past an array's end and it traps with `E-R10` — no silent
corruption, no segfault guessing games.

## What works today

- **Complete pipeline** — parsing → semantic analysis → type checking and
  inference → HIR → MIR → deterministic optimization (boolean constant
  folding, copy propagation, CFG simplification, unreachable-block
  elimination, dead-code elimination) → native code generation → embedded
  runtime.
- **Language subset** — integers, booleans, **strings** (`Str`),
  **typed pointers** (`Ptr<Int>`), **structs** (`struct P { x: Int }` with
  `P { x: 1 }` literals and `p.x` access), **fixed-size arrays**
  (`[1, 2, 3]`, `a[i]`, with compile-time constant-index and runtime
  bounds checks), **enums** (`enum D { A, B }` with `D::A` variant paths,
  nominal enum typing, and single-word discriminant values), **sum
  types** (data-carrying variants `enum Shape { Circle(Int), Nothing }`
  with `E::V(expr)` construction, `E::V(x)` payload patterns, and
  tagged-union layout), **explicit discriminants** (`enum E { A = 5, B }`
  with implicit continuation and duplicate/overflow rejection `E-T31`/
  `E-T32`), **pattern matching** (`match` over `Int`, `Bool`,
  and enums with literal, variant, binding, and `_` wildcard patterns,
  compile-time exhaustiveness `E-T24` and unreachable-arm `E-T25`
  rejection, recursive payload coverage), comparisons, logical and
  bitwise operators,
  `if`/`while`/`for`/`loop` control flow, direct function calls, module
  bindings, and integer results becoming process exit codes.
- **Ownership & borrow checking** — compile-time move semantics for
  heap-owning values (`Str`, structs/arrays containing them): owned
  values move on transfer (use-after-move is `E-S10`), string literals
  copy freely, immutable strings reject mutation (`E-S11`), and
  compile-time borrow checking on top of it: shared (`&`) and exclusive
  (`&mut`) borrows, conflicting-borrow rejection (`E-S12`), and
  dangling-reference rejection (`E-S14`) — invalid programs fail before
  code generation, with no runtime cost (see
  [`OWNERSHIP_IMPLEMENTATION.md`](docs/implementation/OWNERSHIP_IMPLEMENTATION.md)
  and
  [`REFERENCES_BORROWING_IMPLEMENTATION.md`](docs/implementation/REFERENCES_BORROWING_IMPLEMENTATION.md)).
- **Runtime intrinsics** — `rt_alloc`, `rt_free`, `rt_mem_load`,
  `rt_mem_store` (validated against a bounded liveness table), and the
  string intrinsics `rt_str_alloc`/`rt_str_free`/`rt_str_len`/
  `rt_str_byte`/`rt_str_set_byte`/`rt_print_str` (bounds-checked, `E-R09`),
  plus `rt_exit` and `rt_print_int`, backed by a deterministic
  bump/free-list heap with structured `E-R01+` diagnostics.
- **Native target** — `x86_64-windows-pe`: a self-contained code generator
  and PE container builder producing runnable Windows executables with no
  external toolchain.
- **Honest errors** — everything outside the supported subset (floating
  point, characters, …) is rejected with structured diagnostics instead of
  being miscompiled.

## Quick start

Requirements: **Rust 1.85+** (developed against 1.97).

```console
$ git clone https://github.com/p4inz-code/mink.git
$ cd mink
$ cargo build --release
```

Write a program, then validate and build it (the compiler binary lives at
`target/release/mink` after the build above):

```console
$ ./target/release/mink check demo.mink   # parse, analyze, type check, lower, optimize
$ ./target/release/mink build demo.mink   # compile the optimized MIR into demo.exe
$ ./demo.exe
```

| Task                 | Command                                          |
| -------------------- | ------------------------------------------------ |
| Build the compiler   | `cargo build`                                    |
| Run the compiler     | `cargo run -- --version`                         |
| Test                 | `cargo test`                                     |
| Format               | `cargo fmt` / `cargo fmt --check`                |
| Lint                 | `cargo clippy --all-targets -- -D warnings`      |

### CLI

- `mink check <path>` — runs the full front end: lexical, syntactic,
  semantic, type, HIR, and MIR analysis plus optimization. Exit 0 when the
  program validates through MIR; exit 1 with diagnostics otherwise.
- `mink build <path> [--target <triple>]` — compiles the optimized MIR into
  a native executable (default `x86_64-windows-pe`).
- `mink run`, `mink test`, `mink fmt` — recognized but not yet implemented
  (exit 2).
- `mink version` / `mink help` — version and usage.

## Current limitations

Honest status, because durable engineering starts with accurate claims:

- **Single target** — `x86_64-windows-pe` is implemented;
  `x86_64-linux-elf` and `aarch64-linux-elf` are recognized but rejected
  (`E-B11`).
- **Fixed 1 MiB heap** — exhaustion is a structured error (`E-R02`).
- **Single-threaded runtime** — no concurrency primitives yet.
- **Aggregate limits** — structs, arrays, and tagged-union enums are
  values with deterministic C-style layout; they can be returned from
  functions and stored at module scope through a caller-allocated return
  slot and constant-evaluated data images (session 22), and booleans
  packed at any byte offset coexist correctly with the integer fields
  that follow them (session 23). `main` still cannot return an aggregate
  (its result is the exit code, `E-B09`). Tagged-union enums cannot be
  compared with `==`/`!=` (`E-T30`); there
  is no enum-to-`Int` conversion; pattern matching covers
  `Int`/`Bool`/enum scrutinees only (no struct/array destructuring,
  ranges, or or-patterns yet), and there are no tuples or generics.
- **Strings are byte sequences** — literals are immutable, there is no
  concatenation, and UTF-8 well-formedness is not validated at runtime.
- **Borrowing is lexical, not non-lexical** — explicit references
  (`&T` / `&mut T`), borrows (`&place` / `&mut place`), and derefs (`*r`)
  are implemented (session 16) with compile-time borrow checking, but
  lifetimes are lexical (a borrow lives until its binding dies), there is
  no reborrowing, disjoint-field borrows are conservatively rejected,
  enums are not borrowable (`&enum` is `E-T19`), and only whole-value
  deref assignment (`*r = v`) is supported — member/element assignment
  through a deref (`(*r).x = v`) is `E-T33` (see
  [`REFERENCES_BORROWING_IMPLEMENTATION.md`](docs/implementation/REFERENCES_BORROWING_IMPLEMENTATION.md)).
- **No garbage collector** — allocation is explicit and leak-checked on exit.
- **Limited native subset** — no floating point, characters, `null`, or
  function values in the native backend yet.
- **No stdlib or package manager yet** — and no IDE tooling beyond the CLI.

## Roadmap

The long-term plan lives in
[`docs/roadmap/IMPLEMENTATION_ROADMAP.md`](docs/roadmap/IMPLEMENTATION_ROADMAP.md):
memory/ownership, the standard library, package/build system, developer
tooling, web/backend and desktop ecosystems, optimization, security
hardening, and release engineering. Future work is intentionally **not**
claimed as implemented — the "What works today" section is the only status
that matters.

## Documentation

- [`docs/implementation/`](docs/implementation/) — implementation records
  for every stage: lexer, parser, semantic analysis, type system and
  inference, HIR, MIR, optimization, native backend, runtime, the string +
  memory type foundation
  ([`STRING_MEMORY_IMPLEMENTATION.md`](docs/implementation/STRING_MEMORY_IMPLEMENTATION.md)),
  the aggregate (struct/array) foundation
  ([`AGGREGATE_TYPES_IMPLEMENTATION.md`](docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md)),
  the reference/borrowing foundation
  ([`REFERENCES_BORROWING_IMPLEMENTATION.md`](docs/implementation/REFERENCES_BORROWING_IMPLEMENTATION.md)),
  the enum foundation
  ([`ENUM_TYPES_IMPLEMENTATION.md`](docs/implementation/ENUM_TYPES_IMPLEMENTATION.md)),
  the pattern-matching foundation
  ([`PATTERN_MATCHING_IMPLEMENTATION.md`](docs/implementation/PATTERN_MATCHING_IMPLEMENTATION.md)),
  the sum-types foundation
  ([`SUM_TYPES_IMPLEMENTATION.md`](docs/implementation/SUM_TYPES_IMPLEMENTATION.md)),
  and the explicit-discriminants foundation
  ([`DISCRIMINANTS_IMPLEMENTATION.md`](docs/implementation/DISCRIMINANTS_IMPLEMENTATION.md)).
- [`docs/compiler/COMPILER_ARCHITECTURE.md`](docs/compiler/COMPILER_ARCHITECTURE.md)
  — compiler architecture and pipeline.
- [`docs/language/`](docs/language/) — language specifications; the frozen
  core grammar is in
  [`docs/language/CORE_GRAMMAR.md`](docs/language/CORE_GRAMMAR.md).
- [`docs/core/`](docs/core/) — master specification and design rules.
- [`docs/runtime/`](docs/runtime/) — runtime, memory, and concurrency model
  planning.

The native backend design is in
[`docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`](docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md)
and the runtime/memory model in
[`docs/implementation/RUNTIME_IMPLEMENTATION.md`](docs/implementation/RUNTIME_IMPLEMENTATION.md).

## Repository layout

```
├── docs/       Language & architecture specifications + implementation records
├── src/        The compiler (Rust) — lexer, parser, typecheck, hir, mir, backend, runtime
├── tests/      Compiler tests (1121, all passing)
├── Cargo.toml  Package manifest
└── LICENSE     Apache License 2.0
```

## Contributing

MINK is early and moving fast. Good first contributions:

- **Tests** — more coverage of the parser, type checker, backend, and
  runtime invariants.
- **Documentation** — the planning docs under `docs/` are the long-term
  spec; the implementation records under `docs/implementation/` describe
  what is actually built. Keep the two honest.
- **The next milestone** — see
  [`docs/roadmap/IMPLEMENTATION_ROADMAP.md`](docs/roadmap/IMPLEMENTATION_ROADMAP.md).

The project enforces quality gates on every change:

```console
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

## License

Apache License 2.0 — see [`LICENSE`](LICENSE).
