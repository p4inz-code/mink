<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/brand/web/github/mink-readme-header-dark.png">
  <img alt="MINK" src="assets/brand/web/github/mink-readme-header-light.png" width="640">
</picture>

**A systems-oriented native programming language for Windows x86_64.**

![License](https://img.shields.io/badge/license-Apache%202.0-blue)
![Version](https://img.shields.io/badge/version-1.0.5-brightgreen)
![Platform](https://img.shields.io/badge/platform-Windows%20x64-lightgrey)

Created by [Atharva Patil / p4inz-code](https://github.com/p4inz-code). Stewarded by P4inz Interactive Labs.

</div>

---

## What is MINK?

MINK is a systems-oriented native programming language for **Windows x86_64**. It compiles
straight to standalone `.exe` files — no interpreter, no virtual machine, no external
toolchain — and reaches the operating system directly for files, processes, sockets,
threads and time.

MINK is for programmers writing native Windows tools: utilities that read and write real
files, spawn and supervise child processes, talk TCP or UDP, run work on OS threads, and
ship as a single executable with no runtime to install on the target machine.

What MINK provides:

- **Natively compiled, end to end.** Its own lexer, parser, type system, HIR, MIR,
  optimizer, x86-64 code generator and runtime. The compiler has zero Rust crate
  dependencies.
- **Explicit ownership.** Move semantics, `&`/`&mut` borrows and compile-time borrow
  checking, over a heap that is tracked and leak-checked at exit.
- **Standalone output.** A generated executable imports only Windows system libraries —
  no CRT DLL and no third-party runtime. Copy it to any Windows 10+ x86_64 machine.
- **A real standard library.** 27 modules bundled beside the compiler, including
  filesystem, process, network, TLS, SQLite, ZIP/DEFLATE, threads and async.

> **Platform:** Windows x86_64 only. Linux and macOS are not supported yet — see
> [Limitations](#limitations).

## Install MINK

**Requires Windows x64** and **Node.js 18 or newer** (which includes npm). MINK is
distributed through npm.

> **Published package state.** The registry serves **1.0.4** (`npm view
> @p4inz-code/mink version` → `1.0.4`, `latest: 1.0.4`, published 2026-09-30). The release
> candidate prepared in this repository is **1.0.5** — the defect closure described in
> [What's New](#whats-new). Its tarball is built and verified here (`npm pack` → 35 files:
> the compiler binary, 27 `stdlib/` modules, 5 `windows/` assets, `package.json` and
> `LICENSE`, with the packaged `stdlib/` byte-identical to the repository copy), and proven
> by fresh isolated installs of exactly that tarball — including directories whose paths
> contain spaces and non-ASCII characters — where `mink --version` reports `mink 1.0.5`,
> the standard library resolves from the installed package rather than the repository, and
> the Session 180 `invsync` verifier passes all 12 cases. **1.0.5 is not published.** Until
> a maintainer runs `npm publish ./npm/mink --access public` with a valid token, the
> registry keeps serving 1.0.4, so `npm install -g @p4inz-code/mink` installs 1.0.4 and
> `mink --version` prints `mink 1.0.4`.

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
mink 1.0.4
```

(1.0.4 is what the registry currently serves. The 1.0.5 candidate in this repository prints
`mink 1.0.5`; see the published package state note above.)

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

### Optional: give `.mink` files their own Windows identity

The package ships the official MINK source-file icon and a per-user registration script that
makes `.mink` a real Windows file type — **MINK Source File**, content type `text/x-mink`, with
the MINK source-file icon. It applies to your account only, needs no administrator rights, and
double-clicking a `.mink` file never compiles or runs it: Windows asks once which editor to use
and remembers that answer.

```powershell
$package = Join-Path (npm root -g) '@p4inz-code\mink'
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1"
# remove it again:
powershell -ExecutionPolicy Bypass -File "$package\windows\register-mink-filetype.ps1" -Uninstall
```

Bind one editor immediately with `-Editor "C:\Program Files\Microsoft VS Code\Code.exe"`, or
take over an existing `.mink` association with `-Force` (the previous owner is restored by
`-Uninstall`). Registry layout, open behaviour and limitations:
[`docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md`](docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md).

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

```bat
mink build hello.mink    :: creates hello.exe
.\hello.exe              :: runs the executable
```

Program arguments go to the built executable, not through `mink run` (which
takes only the source file):

```bat
mink build program.mink    :: creates program.exe
.\program.exe input.txt    :: rt_argc() == 1, rt_argv(0) == "input.txt"
```

The generated `.exe` is a standalone Windows executable. Copy it to any Windows 10+ x86_64 machine and run it — no MINK compiler needed on the target. Its only imports are Windows system libraries (kernel32, ws2_32 and bcrypt families); it inherits no C-runtime DLL dependency.

## Systems Proof

[`examples/sysinfo/main.mink`](examples/sysinfo/main.mink) is a ~500-line native Windows
utility written in MINK and built with the public CLI into a standalone `.exe`. In a single
process it exercises:

| Capability | What the utility does |
| --- | --- |
| Command-line arguments | `rt_argc` / `rt_argv` |
| Environment | get, set, has, remove, then read back |
| Filesystem | create dir, write, read, size, copy, move, remove, enumerate |
| Process control | create a child process and capture its stdout |
| Binary data | a 4096-byte binary workload |
| Hashing | SHA-256, FNV-1a and CRC-32 over that buffer |
| Compression | zlib compress / decompress round trip, byte-verified |
| Threads and locks | four OS threads accumulating under a lock, checked against the closed form |
| TCP loopback | listen → dial → accept → send → receive → echo → close |
| Time | `rt_time_millis`, `rt_time_now`, `time_strftime` |

The compiled image is a PE32+ x86-64 console executable whose **only import is
`kernel32.dll`**. Copied into an unrelated directory and run with `PATH` restricted to
`C:\Windows\System32` — no compiler, no Rust, no repository files on the path — it prints
the full report and exits 0.

Build it yourself:

```bat
mink build examples\sysinfo\main.mink
```

More complete programs live in [`examples/`](examples/) — a TCP client, an HTTP client, a
compression utility, a threaded-work demo, an async-task demo, a SQLite inventory app and
a regex tool, among others.

## Capability Snapshot

| Area | Capability |
| --- | --- |
| Native compilation | x86-64 PE output, standalone `.exe`, no external toolchain, no CRT DLL |
| Ownership | Move semantics, `&`/`&mut` borrows, compile-time borrow checking |
| Memory | Explicit heap allocation, runtime-validated, exit-time leak check |
| Filesystem | Paths, read/write/copy/move/remove, directories, enumeration |
| Processes | Spawn, capture stdout/stderr, exit codes |
| Environment | get / set / has / remove |
| Networking | TCP and UDP sockets (Winsock2), non-blocking I/O and select |
| Threads | Real OS threads and locks |
| Async | `async fn` / `await` over a task event loop |
| Binary data | Byte buffers, hashing, compression |
| Compression | DEFLATE, zlib and gzip; PKZIP (`.zip`) read and write |
| Hashing | SHA-256, FNV-1a, DJB2, CRC-32; HMAC-SHA256 / HKDF-SHA256 |
| TLS / HTTPS | TLS 1.2/1.3 client and HTTPS GET (Windows Schannel) |
| SQLite | Embedded relational database: statements, results, transactions |
| Project tooling | `init` / `add` / `remove` / `install` / `update`, environments, lockfile |
| Developer CLI | `run`, `build`, `check`, `test`, `repl`, `explain`, JSON diagnostics |

Every row is backed by the repository's own test suite and session records. See
[`docs/audits/SYSTEMS_READINESS_AUDIT.md`](docs/audits/SYSTEMS_READINESS_AUDIT.md) for the
executed evidence and the exact claim boundary.

## Commands

| Command | Description |
| --- | --- |
| `mink run <file>` | Compile and execute a MINK source file |
| `mink build <file>` | Compile to a native Windows executable |
| `mink check <file>` | Analyze source for errors without producing output |
| `mink test <file>` | Discover and run `fn test_*` functions in a source file |
| `mink repl [file]` | Start an interactive compile-eval session (ends at EOF — the end of piped input, or `Ctrl-Z` then `Enter` at a Windows console — or with `:quit`) |
| `mink explain <code>` | Explain an error code (e.g., `mink explain E-T01`) |
| `mink version` | Print the compiler version |
| `mink help` | Show usage information |
| `mink explain` | With no code, list all documented error codes |

### Project and package commands

`mink init` creates the manifest; the other commands run in a project directory (one
containing a `mink.toml`):

| Command | Description |
| --- | --- |
| `mink init [path]` | Write a starter `mink.toml` |
| `mink add <name> [--version <req> \| --path <dir>]` | Declare a dependency and install it |
| `mink remove <name>` | Remove a declared dependency and uninstall what is no longer required |
| `mink install [path] [--check]` | Resolve dependencies, install them into the project, and write `mink.lock` |
| `mink update [path]` | Re-resolve from the manifest, ignoring `mink.lock` |
| `mink env new <name>` | Create and activate an isolated environment |
| `mink env use <name>` | Activate an existing environment |
| `mink env list` | List the project's environments |
| `mink env remove <name>` | Remove an environment and its packages |

Version and help are also available as global flags anywhere the command
name would go: `mink --version`, `mink -V`, and `mink -v` all print the
version; `mink --help` and `mink -h` print usage. Options such as `--help`
are not accepted after a subcommand name (for example, `mink build --help`
reports an unknown option — use `mink help`).

### Examples

```bash
mink check hello.mink          # check for errors
mink build hello.mink          # compile to hello.exe
mink run hello.mink            # compile and run
mink explain E-T01             # explain a type mismatch error
```

## What's New

### MINK 1.0.5

**Release candidate — built and verified locally; not published** (see the note in
[Install MINK](#install-mink)). The registry still serves 1.0.4. This is a defect-closure
release: it restores work that had been verified but never committed, and fixes an
ownership defect found while verifying the package.

Changed in this release (a defect closure; no new features):

- **Recovered work is back in the repository** — the Sessions 140–149 implementation,
  tests and standard-library changes are committed as one coherent unit, with the suite run
  against it rather than against a partial tree
- **`mod environment;` no longer leaks its arguments** — every wrapper consumed its `Str`
  parameters but released only the temporary C-string copies, so any call given a heap
  string (`rt_str_concat` and friends) leaked one allocation and aborted at exit with
  `E-R06: memory leak: live allocations remain at exit`. The defect was invisible to the
  existing tests because they passed string literals, which are not heap objects. Each
  wrapper now frees what it consumes
- **Permanent regressions for both defects** — `tests/environment_lib.rs` builds every
  argument at run time and requires a balanced allocator at exit, and `tests/release.rs`
  asserts the package ships the compiler binary, all 27 `stdlib/` modules, the 5 Windows
  assets and `LICENSE`, with packaged `stdlib/` byte-identical to the repository copy
- **Windows file identity assets are packaged** — `windows/` (icons, `.rc` resource and the
  registration script) and `LICENSE` now ship in the npm tarball, which grows from 28 to
  35 files
- **Full sweep is green** — **3176 passed · 0 failed · 1 ignored** across 78 test targets;
  the single ignored test needs public internet access

### MINK 1.0.4

**Published release (superseded by the prepared 1.0.5).** This is a defect-closure
release: it fixes the public `mod collections;` build failure reported against 1.0.3. A
program that imported the bundled `collections` module but never called `vec_min` or
`vec_max` passed `mink check` and then failed `mink build` with four `E-B07` diagnostics
("invalid backend IR: binary operand type does not match the operator") pointing at
`stdlib/collections.mink`.

Root cause: an untyped function parameter is an inference variable, and the checker's
comparison rule left two unconstrained operands linked but unpinned. With no call site to
unify them, the operands stayed unresolved; the backend classified the unresolved type as
`unit`; and the backend verifier — which requires `Int` operands for `<`, `<=`, `>`, `>=`
— rejected a function the program never called. The fix pins operator operands that the
whole program left unconstrained to `Int` at the end of type checking, so unresolved
operands can no longer reach the backend as `unit`. The standard library is unchanged; the
monomorphizer and the backend are untouched.

Changed in this release (a defect closure; no new features):

- **`mod collections;` builds again** — importing the module is enough; its aggregate
  functions no longer depend on a call site to resolve their comparison operands
- **Runtime values verified, not just exit codes** — permanent native-executing regressions
  in `tests/collections_lib.rs` assert `vec_max`/`vec_min` on `[5,2,9]` → `9, 2`, `vec_sum`
  → `60`, and an untyped `smaller(a, b)` helper → `2, -7`
- **No behaviour change elsewhere** — `tests/typecheck.rs`'s contract test was updated to
  the corrected rule (`arithmetic_on_unconstrained_operands_resolves_to_int`), and the full
  sweep is green: **2973 passed · 0 failed · 1 ignored** (2852 integration passes across all
  71 test targets, plus the 121 unit tests; the single ignored test needs public internet
  access). On this machine `tests/process_lib.rs` additionally requires
  `NoDefaultCurrentDirectoryInExePath` to be unset — an environment property, not compiler
  behaviour: the same 8 tests fail identically under the shipped 1.0.3 binary
- **Bundled compiler refreshed** — `npm/mink/bin/mink.exe` is rebuilt from this source and
  reports `mink 1.0.4`; the packaged `stdlib/` is byte-identical to the repository copy
- **Verified from the exact tarball** — a fresh isolated `npm install` of
  `p4inz-code-mink-1.0.4.tgz` (28 files, 26 `stdlib/` modules) builds and runs the original
  repro correctly and passes the CLI workflow (`init`, `check`, `test`, `build`, `run`,
  `repl`); the mink-scout showcase rebuilds from that install and its fixture verifier
  still reports `VERIFY: PASS` with byte-identical output

### MINK 1.0.3

**Older release (superseded by 1.0.4, which the registry serves).** 1.0.2 was cut from an
earlier commit, so its compiler binary predates the monomorphization and capture fixes
below; do not pin it.

Changed in this release (a release-distribution correction; no new features):

- **Corrected compiler** — the four monomorphization and capture fixes below are what an
  installed user receives with 1.0.3. The 1.0.2 registry package was published from the
  pre-fix commit and did not contain them; do not pin 1.0.2.
- **Corrected runtime ownership** — a map replace or set insert that discards an existing
  key or element now releases the replaced value instead of leaking it
- **Hashing on owned buffers fixed** — `stdlib/hashing.mink` no longer overruns its
  workspace for inputs of 256 bytes or more, and no longer leaks when hashing an owned
  buffer
- **Refreshed bundled compiler** — `npm/mink/bin/mink.exe` is rebuilt from this source and
  reports `mink 1.0.3`; the published tarball's standard library is byte-identical to the
  copy in `npm/mink/stdlib/`
- **Monomorphization and capture repairs (maintenance pass)** — synthetic spans no longer
  collide with real declaration spans, so a capturing closure compiles regardless of the
  file's leading blank lines; literal spans survive monomorphization, so integer and
  string literals inside a generic function's body keep their values; capture order is
  preserved, so multi-capture closures receive the right values; a call whose callee is
  an `rt_*` intrinsic no longer captures the intrinsic's name
- **26 standard-library modules** — `tls`, `sqlite`, `zip`, `zlib`, `re`, `threads`,
  `tasks`, `csv`, `logging`, `assert` and the rest, all bundled beside the compiler
- **Windows x86_64** — self-contained compiler, standalone PE output, no external toolchain

Capabilities:

- **npm distribution** — install with `npm install -g @p4inz-code/mink`, no manual download needed
- **`mink run`** — compile and execute in one step
- **Static CRT** — generated executables link the C runtime statically: no external CRT DLL, only Windows system libraries
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
- **Time** — current time, epoch, calendar fields, `strftime`/`strptime` formatting and parsing, monotonic ticks/frequency for durations
- **Environment** — get/set/has/remove environment variables, wired to the Windows API

## Limitations

MINK is explicit about its edges:

- **Windows x86_64 is the only supported platform.** A native ELF backend exists in source
  for Linux, but Linux is frozen and unsupported; macOS is not supported.
- **Unicode is partial.** `Str` is a byte buffer with a UTF-8 code-point layer, but there
  is no Unicode database — no non-ASCII case operations, categories, normalization or
  collation. Command-line arguments and filesystem entries use the ANSI Windows APIs, so
  non-ASCII arguments and paths are limited.
- **Runtime resources are deliberately bounded.** The heap arena is a 1 GiB
  no-coalesce region, so a single request above that ceiling is rejected with `E-R02`
  rather than served; at most 1,024 heap allocations may be live at once (`E-R03`), which
  bounds workloads such as one live string per split line to roughly 800–1,000 lines; and
  the JSON parser works inside a caller-sized arena. These limits fail loudly and are
  documented, not silent.
- **Some platform surface is missing.** Paths and arguments use the ANSI Windows APIs, so
  non-ASCII paths are limited; long paths (`MAX_PATH`) are unsupported; and there is no
  console control handler, so Ctrl+C cannot be trapped.
- **Tooling is CLI-only.** There is no integrated debugger or profiler.
- **Some ecosystem capabilities remain partial.** There is no package registry or
  publishing workflow (dependencies are declared requirements or local paths), no
  library-version model, and stdin is read-all rather than line-streamed.
- **Soak evidence is bounded, not long-running.** A 300-second mixed soak (1,440 program
  launches, 72 compiler builds, 216 double-free runs, two full TLS suites) and bounded
  per-suite soaks are green with flat handle counts; there is no multi-hour or overnight
  soak record.
- **No superiority claim.** M4 measured developer experience and M5 delivered three frozen
  applications, but the results come from one machine and one run period. MINK is allowed
  to lose comparisons, and no benchmark in this repository demonstrates broad advantage
  over Python, Rust, Go or C#.

## What's Next

Planning is governed by the **canonical locked roadmap** ([`docs/roadmap/IMPLEMENTATION_ROADMAP.md`](docs/roadmap/IMPLEMENTATION_ROADMAP.md)).
**M1 through M5 are complete**: M1 validated 10 real applications with 228 executable cases
and caught one real semantic defect; M2 ran the practical stress validation and closed in
Session 138; M3 closed the reliability and capability gaps it exposed; M4 measured the
developer experience across MINK, Python, Rust, Go and C#; and M5 delivered three frozen
Windows applications with 163 executable checks per run. Native compilation and ownership
are treated as **capabilities that must be tested, not as proven differentiators.**

**Release-mode verification passes.** A clean optimized build compiles in 30.78 s (4,048,384
bytes, `mink 1.0.5`) and the full release suite reports **85 suites, 3,262 passed, 0 failed,
9 ignored** — the same counts as debug. The Sessions 205–210 `rustc` crash was a transient
host fault that no longer reproduces; Session 212 records the evidence and Session 213 the
verification. Ten validation applications rebuilt with the release compiler match their
Python references on **228/228 cases**.

One item is open. **M6 external developer validation is outstanding** — it requires a
developer who did not build MINK. M7 tooling and ecosystem work remains gated on
demonstrated demand. No publication, tag or release was created: 1.0.5 remains unpublished.

MINK targets **Windows x64**, the completed baseline and the only active platform for
practical validation. Linux and any additional platform remain **FROZEN until the M8
strategic gate is passed**; a native ELF backend exists in source but is unmaintained.

| Area | Status |
| --- | --- |
| Windows x64 | Available — the only active platform |
| npm distribution | Available |
| Package manager | Available (manifest, resolver, lockfile, environments) |
| Concurrency / threading | Available (threads, locks, async/await) |
| Linux | **Frozen until M8** — native ELF backend exists in source, unmaintained |
| macOS | Not supported — future / to be decided after M8 |

Ecosystem and tooling work is **gated by the roadmap (M7)**: it begins only after actual
demand is demonstrated, not speculatively.

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
| `time` | Current time, epoch, calendar fields, `strftime`/`strptime`, monotonic ticks/frequency |
| `random` | Random integers, bytes, boolean |
| `environment` | Get/set/has/remove environment variables |
| `tls` | TLS 1.2/1.3 client and HTTPS GET (Windows Schannel) |
| `zip` | PKZIP (`.zip`) reader and writer |
| `zlib` | DEFLATE, zlib and gzip compression/decompression |
| `sqlite` | Embedded relational database: statements, results, transactions |
| `threads` | Threads and locks |
| `tasks` | Async tasks and the event loop |
| `re` | Regular expressions |
| `csv` | RFC 4180 CSV reading and writing |
| `logging` | Leveled logging to stderr |
| `option` | `Option<T>` — a value that may be absent |
| `result` | `Result<T, E>` — a value or an error |
| `assert` | Test assertions: `assert_true`, `assert_false`, and typed `assert_eq_*` / `assert_ne_*` helpers (`_int`, `_str`, `_float`) |

Standard library files live in the `stdlib/` directory and are loaded with `mod`.
They are also bundled next to the compiler in the npm package, so `mod`/`use` of
a standard-library module works in any project without configuration.

## Language Features

MINK supports a rich feature set for a language built from scratch:

- **Types** — `Int`, `Bool`, `Str`, `Float`, `Char`, `Null`, `Ptr<T>`, `Vec<T>`, `Map<K, V>`, `Set<T>`
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
- [`docs/roadmap/IMPLEMENTATION_ROADMAP.md`](docs/roadmap/IMPLEMENTATION_ROADMAP.md) — the canonical locked roadmap (M0–M9)
- [`docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md`](docs/brand/BRAND_AND_WINDOWS_INTEGRATION.md) — brand assets, the `.mink` Windows file type and icon integration
- [`assets/brand/`](assets/brand/) — official MINK logo, icon and web artwork

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
cargo clippy --all-targets
cargo test
cargo build
```

`cargo fmt --check` is clean. `cargo clippy --all-targets` reports a non-zero warning
baseline (dead code and missing documentation in the native backend), so it does not yet
pass `-D warnings`; the gate is that no change adds a new warning.

## Repository Layout

```
├── docs/       Language & architecture specifications + implementation records
├── src/        The compiler (Rust) — lexer, parser, typecheck, hir, mir, backend, runtime
├── stdlib/     Standard library (MINK source)
├── examples/   Complete example programs (one directory each)
├── tests/      Compiler tests
├── npm/        npm distribution package
├── Cargo.toml  Package manifest
└── LICENSE     Apache License 2.0
```

## License

Apache License 2.0 — see [`LICENSE`](LICENSE).

Copyright (c) 2026 Atharva Patil.

---

<!-- SUPPORT-BLOCK:START -->
<div align="center">

### Support my work

I make these tools on my own and keep them free. If one helped you, you can chip in by UPI. Any amount.

<a href="https://p4inz-code.github.io/donate/"><img src="https://raw.githubusercontent.com/p4inz-code/donate/main/qr.svg" alt="UPI QR code. Scan it with any UPI app." width="200"></a>

`9321614988@jio`

On your phone? [Open the donation page](https://p4inz-code.github.io/donate/).

Outside India, or prefer a card?

<a href="https://buymeacoffee.com/p4inz"><img src="https://img.shields.io/badge/Buy%20me%20a%20coffee-p4inz-FFDD00?style=for-the-badge&logo=buymeacoffee&logoColor=black" alt="Buy me a coffee"></a>

</div>
<!-- SUPPORT-BLOCK:END -->
