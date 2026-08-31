<div align="center">

# MINK

**A compiled general-purpose programming language — from lexer to native executable.**

![License](https://img.shields.io/badge/license-Apache%202.0-blue)
![Version](https://img.shields.io/badge/version-1.0.1-brightgreen)
![Platform](https://img.shields.io/badge/platform-Windows%20x64-lightgrey)

Created by [Atharva Patil / p4inz-code](https://github.com/p4inz-code). Stewarded by Northbyte Studios.

</div>

---

## What is MINK?

MINK is a compiled, general-purpose programming language built from the ground up — its own lexer, parser, type system, intermediate representations, optimizer, native code generator, and runtime. No external toolchain required: the compiler produces standalone Windows executables with zero dependencies.

MINK is designed for systems programming and application development, with a focus on catching errors early and providing predictable, deterministic behavior.

## Install MINK

**MINK requires Windows x64.** Linux and macOS are not currently supported.

MINK is distributed through npm. You need **Node.js 18 or newer** (which includes npm).

### Already have Node.js / npm?

```bash
npm install -g @p4inz-code/mink
```

Verify:

```bash
mink --version
```

Expected output:

```text
mink 1.0.1
```

### Don't have Node.js / npm?

1. Install Node.js 18+ from the [official Node.js website](https://nodejs.org/). npm is included — no separate install needed.
2. Open a **new terminal** (or restart your current one).
3. Verify:

```bash
node --version
npm --version
```

4. Install MINK:

```bash
npm install -g @p4inz-code/mink
```

5. Verify:

```bash
mink --version
```

> **Do I need Rust?** No. The npm package ships a pre-built compiler binary. You do not need Rust, Cargo, or any build tools.
>
> **Do I need to download `mink.exe` manually?** No. `npm install` handles everything.

## Quick Start

### First Program

Create a file called `hello.mink`:

```mink
fn main() {
    rt_print_int(42);
    return 0;
}
```

Run it:

```bash
mink run hello.mink
```

Expected output:

```text
42
```

That's it — no build step, no configuration, no runtime installation on the target machine.

### Build and Run

```bash
mink build hello.mink    # creates hello.exe
./hello.exe               # runs the executable
```

The generated `.exe` is a standalone Windows executable. Copy it to any Windows 10+ x86_64 machine and run it — no MINK compiler needed on the target.

## Commands

| Command | Description |
| --- | --- |
| `mink run <file>` | Compile and execute a MINK source file |
| `mink build <file>` | Compile to a native Windows executable |
| `mink check <file>` | Analyze source for errors without producing output |
| `mink explain <code>` | Explain an error code (e.g., `mink explain E-T01`) |
| `mink version` | Print the compiler version |
| `mink help` | Show usage information |

All commands accept `--help` and `--version` flags.

### Examples

```bash
mink check hello.mink          # check for errors
mink build hello.mink          # compile to hello.exe
mink run hello.mink            # compile and run
mink explain E-T01             # explain a type mismatch error
```

## What's New

### MINK 1.0.1

- **npm distribution** — install with `npm install -g @p4inz-code/mink`, no manual download needed
- **`mink run`** — compile and execute in one step
- **Static CRT** — generated executables have no external DLL dependencies
- **Filesystem library** — path operations, file read/write/copy/move, directory operations
- **Process execution** — spawn and manage external processes
- **Networking** — TCP/UDP sockets via Winsock2
- **HTTP client** — HTTP/1.1 request/response, GET/POST, header parsing
- **Crypto** — HMAC-SHA256, HKDF-SHA256, secure random via Windows BCrypt
- **Hashing** — FNV-1a, DJB2, SHA-256, hex encoding
- **JSON** — parse and serialize JSON data
- **Collections** — dynamic vectors with push/pop/search/transform
- **Strings** — concatenation, comparison, integer/boolean conversion
- **Math** — abs, min, max, clamp, pow, sqrt, div/mod, sign
- **Encoding** — Base64, hex, URL encoding/decoding
- **Time** — current time, formatting, epoch, monotonic clock
- **Environment** — get/set environment variables

## What's Next

MINK currently targets **Windows x64**. The next major platform expansion is **Linux**.

| Area | Status |
| --- | --- |
| Windows x64 | Available |
| npm distribution | Available |
| Linux | Next major platform target |
| macOS | Future / TBD |
| Package manager | Future |
| Concurrency / threading | Future |

The project will continue expanding platform support, ecosystem libraries, and production capabilities. See the [Implementation Roadmap](docs/roadmap/IMPLEMENTATION_ROADMAP.md) for the full plan.

## Standard Library

MINK ships with a growing standard library covering common development needs:

| Library | Capabilities |
| --- | --- |
| `strings` | Concatenation, comparison, integer/boolean conversion, length |
| `collections` | Dynamic vectors: push, pop, search, transform |
| `math` | abs, min, max, clamp, pow, sqrt, div/mod, sign |
| `filesystem` | Path ops, file read/write/copy/move/remove, directories |
| `process` | Spawn processes, capture output, manage exit codes |
| `network` | TCP/UDP sockets, connect, bind, listen, send, receive |
| `http` | HTTP/1.1 client, GET/POST, headers, response parsing |
| `json` | Parse and serialize JSON |
| `crypto` | HMAC-SHA256, HKDF-SHA256, secure random |
| `hashing` | FNV-1a, DJB2, SHA-256, hex encoding |
| `encoding` | Base64, hex, URL encoding/decoding |
| `time` | Current time, formatting, epoch, monotonic clock |
| `random` | Random integers, bytes, boolean |
| `environment` | Get/set environment variables |

Standard library files live in the `stdlib/` directory and are imported with `use`.

## Language Features

MINK supports a rich feature set for a language built from scratch:

- **Types** — `Int`, `Bool`, `Str`, `Float`, `Char`, `Null`, `Ptr<T>`
- **Structs** — `struct Point { x: Int, y: Int }` with field access and destructuring
- **Enums** — unit variants, data-carrying variants, sum types, explicit discriminants
- **Generics** — `fn id<T>(x: T) -> T` with monomorphization
- **Pattern matching** — `match` with literals, variants, bindings, wildcards, or-patterns, range patterns, guards
- **Control flow** — `if`/`else`, `while`, `for` ranges, `loop` with `break`
- **Closures** — `|x: Int| x + 1` with capture semantics
- **Ownership** — move semantics, `&`/`&mut` borrows, compile-time borrow checking
- **Modules** — `mod`/`use`/`pub` with multi-file compilation
- **Tuples** — `(Int, Bool)`, field access, destructuring
- **Arrays** — fixed-size with bounds checking

## Examples

### Hello World

```mink
fn main() {
    rt_print_int(42);
    return 0;
}
```

```bash
$ mink run hello.mink
42
```

### Sum 1 to 10

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

```bash
$ mink run sum.mink
55
$ echo $?
1
```

### Strings and Heap

```mink
fn main() {
    let s = rt_str_alloc(3);
    rt_str_set_byte(s, 0, 104);
    rt_str_set_byte(s, 1, 105);
    rt_str_set_byte(s, 2, 33);
    rt_print_str(s);              // prints: hi!
    rt_str_free(s);
    rt_print_str("done");          // string literals are immutable byte data
    return 0;
}
```

### Structs and Enums

```mink
struct Point { x: Int, y: Int }

enum Shape {
    Circle(Int),
    Square(Int),
    Empty,
}

fn main() {
    let p = Point { x: 3, y: 4 };
    let c = Shape::Circle(5);

    match c {
        Shape::Circle(radius) => { rt_print_int(radius); },
        Shape::Square(side) => { rt_print_int(side); },
        Shape::Empty => { rt_print_int(0); },
    }

    return 0;
}
```

## Documentation

- [`docs/implementation/`](docs/implementation/) — implementation records for every compiler stage
- [`docs/compiler/COMPILER_ARCHITECTURE.md`](docs/compiler/COMPILER_ARCHITECTURE.md) — compiler architecture and pipeline
- [`docs/language/`](docs/language/) — language specifications
- [`docs/language/CORE_GRAMMAR.md`](docs/language/CORE_GRAMMAR.md) — core grammar
- [`docs/core/`](docs/core/) — master specification and design rules
- [`docs/runtime/`](docs/runtime/) — runtime, memory, and concurrency model
- [`docs/roadmap/IMPLEMENTATION_ROADMAP.md`](docs/roadmap/IMPLEMENTATION_ROADMAP.md) — implementation roadmap

## Development

Building from source requires **Rust 1.85+**.

```bash
git clone https://github.com/p4inz-code/mink.git
cd mink
cargo build --release
```

Quality gates:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build
```

## Repository Layout

```
├── docs/       Language & architecture specifications + implementation records
├── src/        The compiler (Rust) — lexer, parser, typecheck, hir, mir, backend, runtime
├── stdlib/     Standard library (MINK source)
├── tests/      Compiler tests
├── npm/        npm distribution package
├── Cargo.toml  Package manifest
└── LICENSE     Apache License 2.0
```

## License

Apache License 2.0 — see [`LICENSE`](LICENSE).

Copyright (c) 2026 Atharva Patil.
