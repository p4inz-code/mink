# MINK POST-V1 ECOSYSTEM FOUNDATION AUDIT

**Date:** August 25, 2026
**Version:** MINK 1.0.0 (tag: v1.0.0, commit aaf8866)
**Scope:** Architecture/foundation audit for post-V1 ecosystem construction
**Status:** Read-only audit — no code changes

---

## 1. Executive Summary

MINK 1.0.0 is shipped. The repository is clean, tests pass (163 unit tests, all green), the tag `v1.0.0` is committed, and the working tree is clean.

**What MINK is today:** A single-target (x86_64-windows-pe), single-threaded, dependency-free compiler written in Rust. It implements a complete pipeline from source through native executable with ownership/borrow checking, monomorphized generics, closures, pattern matching, sum types, tuples, structs, enums, and a deterministic heap runtime. The compiler is a single Cargo crate with zero third-party dependencies.

**What MINK needs for the ecosystem:** The V1 architecture is a strong foundation for a *language* but has architectural constraints that block ecosystem construction. The critical gaps are:

1. **No module/package system** — modules are file-based (`mod name;`) but flatten into a single compilation unit. No packages, no dependencies, no registry.
2. **No standard library** — only `rt_*` intrinsics and two MINK source files (`Option<T>`, `Result<T,E>`).
3. **Single target** — x86_64-windows-pe only. No cross-platform foundation.
4. **Single-threaded runtime** — no concurrency primitives.
5. **Fixed 1 MiB heap** — no dynamic heap, no OS-level allocation.
6. **No FFI boundary** — no stable C ABI, no calling convention for external code.
7. **No package manager, no registry** — no build system beyond `mink build <file>`.

**Go/No-Go Decision: CONDITIONAL GO.** MINK's V1 architecture is sufficient to *begin* ecosystem construction IF the following Category-A changes are made first:

- **A-1:** Define a stable C ABI as the FFI foundation (can be designed now, implemented incrementally).
- **A-2:** Design the package/module architecture (design freeze before implementation).
- **A-3:** Design the cross-platform target model (design freeze before implementation).

These are *design* decisions, not rewrites. The V1 compiler pipeline, type system, ownership model, and runtime are architecturally sound and do not need modification.

---

## 2. Current Architecture Map

```
Source (.mink files)
    │
    ▼
Lexer (src/lexer/) — tokenization, spans, lexical diagnostics (E-L01..E-L03)
    │
    ▼
Parser (src/parser/) — recursive descent, error recovery, AST construction
    │
    ▼
AST (src/ast/) — typed syntax representation, all language constructs
    │
    ▼
Monomorphizer (src/monomorphize/) — generic → concrete instantiation, closure desugaring
    │
    ▼
Semantic Analysis (src/semantics/) — name resolution, scope analysis, symbol table
    │
    ▼
Type Checking (src/typecheck/) — unification-based, inference variables, nominal structs/enums
    │
    ▼
Ownership Analysis (src/ownership/) — move semantics, borrow checking (lexical), E-S10..E-S14
    │
    ▼
HIR Lowering (src/hir/) — typed, symbol-resolved, owned tree
    │
    ▼
MIR Lowering (src/mir/) — control-flow IR, basic blocks, statements, terminators
    │
    ▼
MIR Optimization (src/mir/optimize.rs) — boolean constant folding, copy propagation,
    │                                     CFG simplification, unreachable/dead-code elimination
    ▼
Backend Lowering (src/backend/lower.rs) — portable backend IR (BProgram)
    │
    ▼
Backend Verification (src/backend/verify.rs) — structural integrity checks
    │
    ▼
x86_64 Code Generation (src/backend/emit/x86_64.rs) — instruction selection, register allocation
    │
    ▼
PE Container Builder (src/backend/emit/pe.rs) — Windows executable assembly
    │
    ▼
Runtime Embedding (src/backend/emit/runtime.rs) — machine-code runtime services
    │
    ▼
Native Executable (.exe)
```

### Key subsystem responsibilities:

| Subsystem | Owns | Stable API |
|-----------|------|------------|
| **Lexer** | Tokenization, spans, lexical errors | `LexError`, `Token`, `Span` |
| **Parser** | AST construction, error recovery | `ParseOutput`, `ParseError`, `Ast` |
| **AST** | All language syntax nodes | Public struct/enum fields |
| **Monomorphizer** | Generic → concrete, closure desugaring | `monomorphize(&mut Ast)` |
| **Semantic Analysis** | Name resolution, scope, symbol table | `SemanticResult`, `SymbolTable` |
| **Type Checking** | Type inference, unification, type validation | `TypeResult`, `TypeTable`, `TypeId` |
| **Ownership** | Move semantics, borrow checking | `OwnershipResult` |
| **HIR** | Typed, resolved tree | `HirProgram` |
| **MIR** | Control-flow IR | `MirProgram` |
| **Backend** | Code generation, image assembly | `BProgram`, `Target`, `Image` |
| **Runtime** | ABI, heap, intrinsics | `RuntimeLayout`, `Intrinsic`, `MemoryLayout` |
| **Module** | Multi-file compilation (V1) | `ModuleTree`, `ModuleRegistry` |
| **Source** | File loading, spans, line indexing | `SourceMap`, `SourceFile`, `Span` |
| **Diagnostics** | Placeholder (structured engine deferred) | None |

---

## 3. Current Strengths

1. **Clean pipeline architecture.** Every stage has a clear input/output contract. Stages are ordered and gated: ownership errors suppress HIR, type errors suppress ownership, etc.

2. **Zero dependencies.** The compiler is entirely self-contained. No external crate for parsing, CLI, or code generation. This is unusual and valuable for a language compiler.

3. **Sound ownership foundation.** Move semantics with compile-time use-after-move detection (E-S10), immutable-mutation rejection (E-S11), borrow checking (E-S12/E-S13/E-S14), and lexical lifetimes. Conservative by design.

4. **Deterministic compilation.** Identical source → identical output. No HashMap iteration order leakage, no non-deterministic passes.

5. **Structured diagnostics.** Every error has a stable code (E-L01, E-T05, E-S10, E-R05, E-B07, etc.), source spans, and related locations. The diagnostic model is already machine-readable.

6. **Three-tier IR.** AST → HIR → MIR → Backend IR. Each tier has appropriate abstraction: AST for syntax, HIR for types/symbols, MIR for control flow, Backend IR for machine operations.

7. **Self-contained code generation.** No external toolchain (no C compiler, assembler, or linker). The backend emits complete PE executables directly.

8. **Monomorphization works.** Generics, closures (desugared to named functions), and module flattening all work correctly.

9. **Ownership through the full pipeline.** Ownership analysis runs early, gates HIR lowering, and the backend preserves ownership semantics (moves are compile-time fiction; runtime is safety backstop).

10. **163 tests, all green.** The test suite covers lexer, parser, semantics, typecheck, HIR, MIR, backend, ownership, references, modules, generics, closures, pattern matching, tuples, aggregates, strings, and end-to-end compilation.

---

## 4. Current Architectural Constraints

### Windows-specific assumptions:
- Target is `x86_64-windows-pe` only. PE container builder, Windows API calls (GetStdHandle, WriteFile), calling convention (Microsoft x64 ABI).
- Runtime services use Windows-specific intrinsics (WriteFile, GetStdHandle).
- Executable output is `.exe` only.

### x86_64-specific assumptions:
- All code generation is x86-64. Register allocation, instruction selection, and ABI are x86-64 specific.
- No aarch64, no 32-bit targets.

### Single-threaded assumptions:
- No concurrency primitives in the runtime or language.
- No thread-local storage, no atomics, no synchronization.

### Fixed-heap assumptions:
- 1 MiB arena in `.bss`, zero-initialized by loader.
- Max 256 live allocations. Liveness table is fixed-size.
- No OS-level memory allocation (mmap/VirtualAlloc).

### Library/package blocking assumptions:
- No package identity (no `mink.toml`, no package name).
- No dependency declaration or resolution.
- No module boundaries (modules flatten into one compilation unit).
- No import/export across packages.
- No version model.

### FFI blocking assumptions:
- No stable ABI for calling external code.
- No `extern` syntax or function declaration.
- No type representation that crosses language boundaries.

### Cross-platform blocking assumptions:
- Backend emits Windows PE only. No ELF, no Mach-O.
- Runtime uses Windows APIs directly in machine code.
- No platform abstraction layer.

---

## 5. Ecosystem Layer Model

After evaluating the current architecture and the requirements for a cross-platform, interoperable ecosystem, here is the recommended layering:

| Layer | Name | Scope |
|-------|------|-------|
| **L0** | Language | Syntax, semantics, type system, ownership model |
| **L1** | Compiler | Frontend (lexer → parser → AST → semantic → typecheck → ownership → HIR → MIR), optimizer, backend |
| **L2** | Runtime | ABI, heap, intrinsics, platform abstraction, startup/shutdown |
| **L3** | Core Library | `Option<T>`, `Result<T,E>`, primitive operations, error types — compiler-provided |
| **L4** | Standard Library | Collections, strings, I/O, networking, concurrency, serialization, time |
| **L5** | Module System | Package identity, module declarations, visibility, imports/exports |
| **L6** | Package Manager | Dependency resolution, lockfile, build, test, run, publish |
| **L7** | Registry | Package hosting, versioning, integrity, signing, discovery |
| **L8** | FFI/Interop | C ABI, language bridges (Python, C++, C#, Rust) |
| **L9** | Developer Tooling | LSP, formatter, linter, debugger, AI diagnostics |
| **L10** | Domain Libraries | Web, database, crypto, AI/ML, gaming |
| **L11** | AI Integration | Machine-readable metadata, deterministic builds, structured errors |

**Key design principle:** Layers must not create circular dependencies. Domain libraries (L10) must not depend on compiler internals (L1). FFI (L8) must not depend on the standard library (L4) — it must work with L2 (runtime) alone.

---

## 6. Standard Library Architecture

### What belongs in the compiler (cannot be written in MINK):
- `Option<T>`, `Result<T,E>` — these are already compiler-provided (session 39, monomorphized before semantic analysis)
- Primitive operations (arithmetic, comparisons, boolean logic)
- Memory management (allocation, deallocation, bounds checking)
- Type representation (struct layout, enum discriminants)
- Pattern matching dispatch

### What belongs in the runtime:
- Heap management (bump/free-list allocator)
- Process lifecycle (init, exit, leak check)
- Intrinsics (`rt_alloc`, `rt_free`, `rt_mem_*`, `rt_str_*`, `rt_vec_*`, `rt_print_*`)
- Platform abstraction (syscall wrappers)
- Error reporting infrastructure

### What belongs in the core library (MINK source, compiler-provided):
- `Option<T>` and `Result<T,E>` (already done)
- `Option` and `Result` method implementations (`.unwrap()`, `.map()`, `.and_then()`)
- Error trait/type
- Unit type support
- Range utilities

### What belongs in the standard library (MINK source packages):
- **Core:** `collections` (Vec, HashMap, HashSet, etc.), `strings` (UTF-8, formatting), `math`, `time`, `encoding`
- **System:** `fs`, `process`, `env`, `thread`, `sync`
- **Network:** `net` (TCP, UDP), `http`, `dns`, `tls`
- **Data:** `json`, `toml`, `csv`, `serialization`
- **Dev:** `testing`, `logging`, `cli`

### What belongs in external packages:
- Database drivers, web frameworks, game engines, ML inference, specialized crypto

### Naming convention recommendation:
```
use core::collections::Vec;
use std::fs;
use std::net::TcpListener;
use mink_json;  // external package
```

---

## 7. Cross-Platform Architecture

### Target triple format:
```
{arch}-{vendor}-{os}-{abi}
```
Examples:
- `x86_64-pc-windows-msvc` (current)
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `aarch64-apple-darwin`

### Platform detection:
The compiler should support conditional compilation:
```mink
#[cfg(target_os = "windows")]
fn platform_impl() { ... }

#[cfg(target_os = "linux")]
fn platform_impl() { ... }
```

### Platform abstraction layers needed:

| Abstraction | Current state | Needed |
|-------------|--------------|--------|
| Syscalls | Windows API in machine code | Abstract over Windows/Linux/macOS |
| File I/O | Not implemented | Platform-agnostic file operations |
| Networking | Not implemented | BSD socket abstraction |
| Threads | Not implemented | pthreads/Win32 threads |
| Dynamic loading | Not implemented | dlopen/LoadLibrary |
| Memory mapping | Not implemented | mmap/VirtualAlloc |

### Approach decision:

**Approach A: Runtime-linked platform abstraction**
- Platform-specific code in separate source files
- Linker selects the right implementation
- Pros: Clean separation, no runtime overhead
- Cons: Requires linker/platform selection at build time

**Approach B: Runtime-vtable platform abstraction**
- Single runtime with function pointers selected at init
- Pros: Single binary, runtime detection
- Cons: Indirection overhead, more complex init

**CHOSEN: Approach A (Runtime-linked).** MINK compiles to native code with no external toolchain. Platform selection should happen at compile time, matching the current model. The runtime is embedded in the binary, so platform-specific runtime code is selected during compilation, not at runtime.

### Minimum viable platform targets (in priority order):
1. `x86_64-pc-windows-msvc` (current, rename from `x86_64-windows-pe`)
2. `x86_64-unknown-linux-gnu`
3. `aarch64-unknown-linux-gnu`
4. `aarch64-apple-darwin`

---

## 8. Module Architecture

### Current state:
- `mod name;` loads `name.mink` from the same directory
- `use mod_name;` or `use mod_name::Item;` imports items
- `pub` makes items visible across modules
- Modules are flattened into a single compilation unit (no separate compilation)

### What the module system needs:

**Module identity:**
- Each module has a unique path-based identity
- Modules are identified by `package::module::item` paths

**Visibility:**
- `pub` (public), `pub(crate)`, `pub(super)` (private by default)
- Already partially implemented

**Import system:**
- `use package::module::Item;`
- `use package::module::*;`
- `use package::module::{Item1, Item2};`
- Currently: `use mod_name;` (flat import)

### Approach decision:

**Approach A: Rust-style module system**
- File-based modules (`mod name;` → `name.mink`)
- `use` for imports, `pub` for visibility
- Module tree maps to directory structure
- Pros: Familiar to Rust developers, proven model
- Cons: Requires separate compilation support

**Approach B: Python-style module system**
- Package directories with `__init__.mink` (or equivalent)
- `import package.module`
- Pros: Simple for small projects
- Cons: Runtime resolution, less compile-time safety

**CHOSEN: Approach A (Rust-style).** MINK is a compiled language with compile-time type checking. Rust's module model maps naturally to MINK's existing `mod`/`use`/`pub` syntax and provides strong compile-time guarantees. The existing V1 module system is already 80% of this — it needs separate compilation, not redesign.

---

## 9. Package Architecture

### Package identity:
```toml
# mink.toml
[package]
name = "my-project"
version = "0.1.0"
edition = "2026"
license = "Apache-2.0"

[dependencies]
mink-std = "1.0.0"
mink-json = { version = "0.3.0", features = ["serde"] }
```

### Package layout:
```
my-project/
├── mink.toml          # package manifest
├── mink.lock          # lockfile
├── src/
│   ├── main.mink      # entry point (binary)
│   └── lib.mink       # library root
├── tests/
│   └── integration.mink
├── examples/
└── docs/
```

### Dependency declaration:
- Semver ranges: `"1.0.0"`, `"^1.0.0"`, `"~1.2.3"`, `">=1.0, <2.0"`
- Features: optional compilation flags
- Platform conditions: `[target.'cfg(target_os = "windows")'.dependencies]`
- Dev dependencies: `[dev-dependencies]`
- Build dependencies: `[build-dependencies]`

### Dependency resolution:
- Deterministic resolution given same inputs
- Lockfile records exact versions, hashes, and dependency graph
- Conflict detection (diamond problem)
- Cycle detection

### Approach decision:

**Approach A: Cargo-inspired package model**
- `mink.toml` manifest (like `Cargo.toml`)
- Semver-based resolution
- Lockfile (`mink.lock`)
- Features system
- Pros: Proven at scale, familiar to Rust developers
- Cons: Complexity of feature resolution

**Approach B: Go-module-inspired package model**
- Minimal manifest, go.sum-style integrity
- Module proxy for caching
- Simpler feature model
- Pros: Simpler, faster builds
- Cons: Less flexible, less ecosystem tooling

**CHOSEN: Approach A (Cargo-inspired).** MINK is a systems language. Cargo's package model is the most mature systems-language package system. It handles the complexity MINK needs (features, platform deps, build scripts, workspaces) while being well-understood. The zero-dependency compiler can adopt this incrementally.

---

## 10. Package Manager Architecture

### CLI commands (minimal coherent set):
```
mink init <name>          # create new project
mink build                # build the project
mink run                  # build and run
mink test                 # run tests
mink check                # type-check without building
mink fmt                  # format source
mink lint                 # lint source
mink add <pkg>            # add dependency
mink remove <pkg>         # remove dependency
mink update               # update dependencies
mink publish              # publish to registry
mink deps                 # show dependency tree
mink search <query>       # search registry
mink explain <error-code> # explain diagnostic
```

### Core architecture:
- **Dependency resolver:** SAT-solver-based semver resolution
- **Lockfile:** `mink.lock` with exact versions and content hashes
- **Cache:** `~/.mink/cache/` with integrity verification
- **Build system:** Delegates to compiler with dependency context
- **Registry client:** HTTP/HTTPS with retry, caching, integrity checks

### Security model:
- All packages verified by content hash before use
- Lockfile is authoritative for reproducible builds
- Build scripts run in sandboxed environment (phase 2)
- Package signing supported but not required initially

---

## 11. Registry Architecture

### Package naming:
- `lowercase-name` (like npm/crates.io)
- Optional namespace: `@org/package-name`
- No uppercase, no special characters (hyphens and underscores allowed)

### Versioning:
- Semantic versioning (MAJOR.MINOR.PATCH)
- Immutable versions (yanking allowed, no replacement)
- Pre-release versions: `1.0.0-alpha.1`

### API design:
- REST API for package metadata, download, search
- GraphQL optional for complex queries
- Content-addressable storage for package archives

### Key features:
- Package checksums (SHA-256)
- Dependency metadata
- Platform-specific artifacts
- Source distributions
- Vulnerability advisories
- License metadata

---

## 12. FFI Architecture

### Critical decision: Stable C ABI as universal bridge

MINK should expose a **stable C ABI** as the lowest-level FFI boundary. This is the most important architectural decision for the ecosystem.

**Why C ABI:**
- Every major language (Python, C++, C#, Rust, Go, Java) has C FFI support
- C ABI is the de facto standard for cross-language interop
- Simple to implement (no name mangling, no complex type representations)
- Well-understood calling conventions per platform

### FFI model:

```mink
// MINK source declaring an external C function
extern "C" fn printf(format: Ptr<Char>, ...) -> Int;

// MINK source exporting a function for C consumption
#[export]
fn mink_add(a: Int, b: Int) -> Int {
    return a + b;
}
```

### Type mapping (MINK → C):

| MINK Type | C Type | Notes |
|-----------|--------|-------|
| `Int` | `int64_t` | 64-bit signed |
| `Bool` | `uint8_t` | 0 or 1 |
| `Float` | `double` | IEEE 754 |
| `Char` | `uint8_t` | Single byte |
| `Str` | `struct { uint64_t len; const uint8_t* ptr; }` | Fat pointer |
| `Ptr<T>` | `T*` | Raw pointer |
| `&T` | `T*` | Borrowed pointer |
| `&mut T` | `T*` | Exclusive pointer |
| `()` | `void` | No return |
| `Struct` | C struct (field-by-field) | Blittable only |
| `Enum` | `int64_t` (discriminant) | Tag value |

### Approach decision:

**Approach A: C ABI as primary FFI boundary**
- All foreign calls go through C ABI
- Simple, proven, every language supports it
- Cons: Some overhead for complex types (C++ templates, Rust enums)

**Approach B: Native ABI with language-specific bridges**
- Direct calling convention for each target language
- Zero-overhead for language-specific interop
- Cons: N implementations for N languages, maintenance burden

**CHOSEN: Approach A (C ABI as primary).** The complexity of maintaining native ABI bridges for every language is not justified at this stage. C ABI covers 95% of use cases and can be optimized later. MINK's strategy is to work *with* existing stacks, not replace them — C ABI is the universal adapter.

---

## 13. Python Integration Architecture

### Integration points:
1. **MINK → Python:** MINK compiled library callable from Python
2. **Python → MINK:** Python calling MINK functions (via C ABI)
3. **Buffer sharing:** NumPy arrays, byte buffers passed between languages

### Model: CPython extension module

```
mink_python/
├── src/
│   ├── lib.rs          # Rust/FFI bridge
│   └── mink_module.c   # CPython module definition
├── mink.toml           # MINK package
└── setup.py            # Python build
```

### Zero-copy considerations:
- NumPy arrays: share memory buffer via pointer (zero-copy)
- Byte buffers: share underlying memory (zero-copy for read)
- Strings: may require copying (MINK strings are length-prefixed, Python strings are not)
- Complex objects: require serialization/deserialization

### Recommended approach:
- Use C ABI as the bridge layer
- Generate Python wrappers from MINK type metadata
- Provide a `mink-bind` tool for binding generation
- Ship as Python wheels with platform-specific MINK binaries

---

## 14. C/C++ Integration Architecture

### MINK → C:
- Export functions with `#[export]` attribute
- C ABI calling convention
- Header generation from MINK source
- Struct layout compatible with C (blittable)

### C → MINK:
- Declare external functions with `extern "C"`
- Call MINK functions from C using the C ABI
- Link against MINK compiled library

### MINK → C++:
- Same as MINK → C (use `extern "C"` for the ABI)
- C++ wrapper layer for type safety
- RAII wrappers for MINK resources

### C++ → MINK:
- Wrap C ABI calls in C++ classes
- Provide header files with type-safe wrappers

### Header generation:
```
mink bind --target c src/lib.mink > lib.h
mink bind --target c++ src/lib.mink > lib.hpp
```

---

## 15. C# Integration Architecture

### Model:
```
MINK compiled library (.dll, C ABI)
        ↓
C# P/Invoke / source generation
        ↓
.NET application
```

### Blittable type mapping:

| MINK | C# | Notes |
|------|-----|-------|
| `Int` | `long` | 64-bit |
| `Bool` | `byte` | 0/1 |
| `Float` | `double` | IEEE 754 |
| `Str` | `IntPtr` + length | Manual marshaling |
| `Ptr<T>` | `IntPtr` | Raw pointer |
| `Struct` | Blittable struct | Same layout |

### SafeHandle pattern:
- MINK resources (heap allocations, strings) wrapped in `SafeHandle`
- Deterministic release via `IDisposable`
- Prevents use-after-free in .NET

---

## 16. Rust Interoperability Architecture

### Model: C ABI bridge

MINK ↔ Rust should go through the C ABI, NOT through direct Rust type sharing.

**Why:**
- MINK and Rust have different ownership models
- Rust's `unsafe` boundaries would leak into MINK
- C ABI is simpler and more portable

### Binding generation:
```
mink bind --target rust src/lib.mink > mink_bindings.rs
```

### Generated Rust code:
```rust
#[repr(C)]
pub struct MinkStr {
    pub len: u64,
    pub ptr: *const u8,
}

extern "C" {
    pub fn mink_add(a: i64, b: i64) -> i64;
    pub fn mink_str_new(len: u64) -> MinkStr;
}
```

---

## 17. Zero-Copy Strategy

### Truthful capability model:

| Scenario | Can be zero-copy? | Requires |
|----------|------------------|----------|
| NumPy array → MINK | Yes (read-only) | Pointer sharing, lifetime contract |
| MINK buffer → Python | Yes (read-only) | Pointer sharing |
| C++ span → MINK | Yes (borrowed) | Lifetime contract |
| MINK → C++ vector | No (ownership transfer) | Copy or move |
| Rust slice → MINK | Yes (borrowed) | C ABI pointer + length |
| MINK string → Python | No | Copy (different representations) |
| Struct value across FFI | Conditional | Blittable layout guarantee |

### Safety rules:
1. **Zero-copy requires explicit lifetime contracts** — the receiving language must not use the data after the source releases it
2. **Ownership transfer always requires a copy** — unless both languages agree on a shared allocator
3. **Borrowed data must be read-only** — mutable shared data requires synchronization
4. **MINK verifies safety at compile time** — the borrow checker prevents dangling references within MINK; cross-language lifetimes are documented but not compiler-enforced

---

## 18. AI-Native Architecture

### What makes MINK good for AI agents:

1. **Machine-readable diagnostics.** Every error has a stable code, exact location, root cause, and suggested fix. AI agents can parse these programmatically.

2. **Deterministic builds.** Same source → same output. AI agents can verify their changes produce consistent results.

3. **Strong type system.** Type errors are precise and localized. AI agents can fix them systematically.

4. **Ownership model.** Move/borrow errors are caught at compile time. AI agents can understand and fix ownership issues.

5. **Structured project metadata.** `mink.toml` provides machine-readable project configuration.

### AI tooling commands:
```
mink check --json          # Machine-readable diagnostics
mink explain E-T05         # Explain an error code
mink deps --json           # Dependency graph
mink test --json           # Structured test results
```

### Diagnostic JSON format:
```json
{
  "code": "E-T05",
  "severity": "error",
  "message": "type mismatch",
  "expected": "Int",
  "actual": "Bool",
  "span": { "file": "main.mink", "start": 42, "end": 45 },
  "suggested_fix": "cast the value to Int",
  "documentation": "https://mink.dev/docs/errors/E-T05"
}
```

---

## 19. Developer Experience Architecture

### Project workflow:
```bash
mink init my-project
cd my-project
mink add mink-std
mink build
mink test
mink run
```

### Project structure:
```
my-project/
├── mink.toml
├── mink.lock
├── src/
│   ├── main.mink        # binary entry point
│   └── lib.mink         # library root
├── tests/
│   └── test_main.mink
├── examples/
│   └── demo.mink
└── docs/
```

### Build profiles:
- `dev` — fast compilation, debug info
- `release` — optimized, no debug info
- `test` — optimized + test assertions enabled

---

## 20. Security/Supply-Chain Architecture

### Package integrity:
- SHA-256 content hashes for every package version
- Lockfile records hashes for all dependencies
- Download verification before use

### Build security:
- Build scripts (if supported) run with restricted permissions
- No automatic network access during builds
- Filesystem access limited to project directory

### Registry security:
- Package signing (optional, not required initially)
- Vulnerability advisory database
- Malicious package reporting
- Dependency confusion protection (namespace isolation)

---

## 21. Library Category Map

| Category | Standard Library | Official Libraries | Community |
|----------|-----------------|-------------------|-----------|
| **Collections** | Vec, HashMap, HashSet | BTreeMap, PriorityQueue | Custom collections |
| **Strings** | Str, formatting | Unicode, regex | NLP libraries |
| **Math** | Int arithmetic | BigNum, statistics | Scientific computing |
| **Time** | Basic duration | DateTime, timezone | Calendar libraries |
| **Filesystem** | File, Directory | Watcher, temp files | — |
| **Process** | spawn, env | Orchestration | — |
| **Networking** | TCP, UDP | HTTP, WebSocket, DNS | Protocol libraries |
| **Data** | Serialization | JSON, TOML, CSV | Protocol buffers |
| **Security** | Hashing | TLS, crypto | Auth libraries |
| **Concurrency** | Thread, Mutex | Channel, async | Actor frameworks |
| **Testing** | assert, test runner | Property testing | Mocking |
| **CLI** | Args parsing | Progress bars, colors | — |
| **AI/ML** | — | Tensor, inference | ML frameworks |
| **Game** | — | ECS, physics | Game engines |

---

## 22. Official Library Quality Standard

Every official MINK library must satisfy:

1. **Cross-platform:** Works on Windows, Linux, macOS (at minimum)
2. **Documentation:** Every public API documented with purpose, params, returns, errors, examples
3. **Tests:** ≥90% code coverage, all tests pass
4. **Benchmarks:** Performance-critical APIs have benchmarks
5. **API stability:** Semantic versioning, deprecation policy
6. **Error handling:** All errors use `Result<T,E>`, no panics in library code
7. **Ownership correctness:** No memory leaks, no use-after-free
8. **Thread safety:** Documented thread safety guarantees
9. **Security review:** Security-sensitive APIs reviewed
10. **Fuzzing:** Parser/serialization libraries fuzz-tested
11. **Examples:** Working examples for major APIs
12. **AI discoverability:** Metadata machine-readable
13. **Dependency minimization:** Minimal external dependencies
14. **Deterministic builds:** Reproducible compilation

---

## 23. Versioning/Compatibility Model

### Language versions:
- `MINK 1.0` — current, frozen
- `MINK 1.1` — backward-compatible additions
- `MINK 2.0` — potentially breaking changes (rare)

### Compiler versions:
- Semantic versioning: `MAJOR.MINOR.PATCH`
- `MAJOR` — incompatible compiler output (ABI break)
- `MINOR` — new features, backward compatible
- `PATCH` — bug fixes

### Standard library versions:
- Independent from compiler version
- `1.x` works with compiler `1.y` where `y >= x`
- Breaking stdlib changes require major version bump

### Package versions:
- Semantic versioning
- Lockfile pins exact versions
- Compatibility declared in `mink.toml`

### ABI versions:
- Runtime ABI versioned separately
- Compiler emits ABI version in executable header
- Runtime checks ABI version at startup

---

## 24. Two-Approach Decisions

### Module/Package Model

| | Approach A: Rust-style | Approach B: Python-style |
|---|---|---|
| **Pros** | Compile-time safety, proven at scale, familiar | Simple, flexible, dynamic |
| **Cons** | More complex implementation | Runtime resolution, less safety |
| **Risk** | Over-engineering for V1 | Insufficient for large projects |
| **Complexity** | Medium-High | Low-Medium |
| **CHOSEN** | ✅ | |

### Package Manager Architecture

| | Approach A: Cargo-inspired | Approach B: Go-module-inspired |
|---|---|---|
| **Pros** | Mature, handles features/platforms/workspaces | Simple, fast, less config |
| **Cons** | Complex feature resolution | Less flexible, fewer options |
| **Risk** | Feature resolution bugs | Missing needed features |
| **Complexity** | High | Medium |
| **CHOSEN** | ✅ | |

### Cross-Platform Architecture

| | Approach A: Compile-time selection | Approach B: Runtime-vtable |
|---|---|---|
| **Pros** | Zero overhead, matches current model | Single binary, runtime detection |
| **Cons** | Different binaries per platform | Indirection overhead |
| **Risk** | Binary size (one per platform) | Init complexity |
| **Complexity** | Medium | Medium-High |
| **CHOSEN** | ✅ | |

### FFI Foundation

| | Approach A: C ABI primary | Approach B: Native ABI bridges |
|---|---|---|
| **Pros** | Universal, proven, every language supports it | Zero-overhead per language |
| **Cons** | Some overhead for complex types | N implementations for N languages |
| **Risk** | Performance for hot paths | Maintenance burden |
| **Complexity** | Low-Medium | High |
| **CHOSEN** | ✅ | |

### Python Integration

| | Approach A: CPython extension | Approach B: PyO3-like model |
|---|---|---|
| **Pros** | No Rust dependency, direct C ABI | Rich Rust-Python interop |
| **Cons** | Manual marshaling | Requires Rust toolchain |
| **Risk** | Binding maintenance | Dependency on PyO3 |
| **Complexity** | Medium | Medium |
| **CHOSEN** | ✅ (Phase A, simpler) | Phase B if needed |

### Standard Library Boundary

| | Approach A: Minimal core + packages | Approach B: Large stdlib |
|---|---|---|
| **Pros** | Fast compiler builds, clear boundaries | Everything available immediately |
| **Cons** | Users must install packages for basics | Slow builds, bloated |
| **Risk** | Fragmentation | Stagnation |
| **Complexity** | Low | High |
| **CHOSEN** | ✅ | |

---

## 25. 10-Persona Audit

### 1. Language Architect
**Strengths:** Clean syntax, strong ownership model, sound type system, pattern matching, generics via monomorphization.
**Risks:** Lexical lifetimes limit expressiveness (no NLL). No traits/interfaces. No async/await syntax.
**Missing foundations:** Trait system (V2), async syntax (V2), higher-kinded types (V3).
**Migration risks:** Adding traits later requires careful backward compatibility.
**Recommended changes:** None for V1. Traits are a V2 language feature.

### 2. Compiler Engineer
**Strengths:** Clean pipeline, zero dependencies, deterministic output, three-tier IR.
**Risks:** Single-crate structure limits parallel compilation. No incremental compilation. Monomorphization is AST-level (not HIR/MIR-level).
**Missing foundations:** Incremental compilation, parallel compilation, workspace support.
**Migration risks:** Splitting into workspace crates requires interface stabilization.
**Recommended changes:** Design workspace crate split (Category B), but don't implement yet.

### 3. Runtime Engineer
**Strengths:** Deterministic heap, structured errors, leak detection, clean ABI.
**Risks:** Fixed 1 MiB heap. No OS-level allocation. Single-threaded. Windows-specific runtime services.
**Missing foundations:** Dynamic heap, platform abstraction, threading support.
**Migration risks:** Changing runtime ABI requires recompilation of all binaries.
**Recommended changes:** Design platform abstraction layer (Category A for design, C for implementation).

### 4. Library Designer
**Strengths:** Strong type system enables safe library APIs. Ownership prevents resource leaks.
**Risks:** No package system means no libraries yet. No `Option`/`Result` methods. No iteration traits.
**Missing foundations:** Package system, standard library architecture, API conventions.
**Migration risks:** API stability requires careful design from the start.
**Recommended changes:** Design package system (Category A), design stdlib architecture (Category B).

### 5. Package Manager Engineer
**Strengths:** Clean separation of concerns in the compiler. Deterministic compilation enables reproducible builds.
**Risks:** No package identity, no dependency resolution, no lockfile, no registry.
**Missing foundations:** Everything in this category.
**Migration risks:** Package format changes after packages exist are costly.
**Recommended changes:** Design package format early (Category A).

### 6. FFI/Interoperability Engineer
**Strengths:** Clean runtime ABI. No external dependencies. Self-contained code generation.
**Risks:** No stable C ABI. No `extern` syntax. No type representation for FFI. Windows-specific calling convention.
**Missing foundations:** C ABI design, type mapping, export/import syntax.
**Migration risks:** Changing C ABI after libraries depend on it is extremely costly.
**Recommended changes:** Design and freeze C ABI before any FFI work (Category A).

### 7. Cross-Platform Engineer
**Strengths:** Target abstraction exists (`Target` enum). Backend is target-independent up to emission.
**Risks:** Only x86_64-windows-pe implemented. Runtime uses Windows APIs directly. No platform abstraction.
**Missing foundations:** ELF/Mach-O emission, platform-specific runtime services, conditional compilation.
**Migration risks:** Adding platforms after ecosystem libraries assume Windows is costly.
**Recommended changes:** Design platform model (Category A), implement Linux target (Category B).

### 8. Security/Supply-Chain Engineer
**Strengths:** No external dependencies (supply chain is clean). Deterministic builds. Structured errors.
**Risks:** No package signing, no integrity verification, no sandbox model.
**Missing foundations:** Package security model, build script sandbox, dependency auditing.
**Migration risks:** Adding security after packages exist is reactive, not proactive.
**Recommended changes:** Design security model before package system implementation (Category B).

### 9. Developer-Experience Engineer
**Strengths:** Clean CLI (`mink build`, `mink check`). Good error messages with stable codes.
**Risks:** No formatter, no linter, no LSP, no IDE support, no `mink run`/`mink test`.
**Missing foundations:** Everything in this category except the basic CLI.
**Migration risks:** Tooling that doesn't match the package system is wasted work.
**Recommended changes:** Design tooling after package system is designed (Category C).

### 10. AI-Agent/Tooling Engineer
**Strengths:** Stable error codes, source spans, deterministic builds, structured project metadata.
**Risks:** No JSON output mode, no machine-readable diagnostics, no project introspection API.
**Missing foundations:** JSON diagnostic output, project inspection commands, documentation metadata.
**Migration risks:** AI tooling that requires scraping terminal output is fragile.
**Recommended changes:** Add JSON diagnostic output early (Category B), design AI metadata format (Category C).

---

## 26. Root-Cause Analysis

### Risk 1: No package system
- **Root cause:** V1 focused on language features, not ecosystem infrastructure.
- **If ignored:** No libraries can be shared. Every project reinvents everything.
- **Blocks libraries:** Yes, completely.
- **Blocks cross-platform:** Partially (platform-specific code needs package-level conditional compilation).
- **Blocks FFI:** No (FFI can work without packages).
- **Blocks AI tooling:** Partially (no dependency graph to inspect).
- **Requires compiler changes:** No (package system is external to compiler).
- **Requires runtime changes:** No.

### Risk 2: Single target (Windows)
- **Root cause:** V1 built on the developer's platform.
- **If ignored:** MINK can only be used on Windows. No cross-platform ecosystem.
- **Blocks libraries:** Yes for platform-specific libraries.
- **Blocks cross-platform:** Yes, completely.
- **Blocks FFI:** Partially (C ABI is cross-platform but emission is not).
- **Blocks AI tooling:** No.
- **Requires compiler changes:** Yes (new backends).
- **Requires runtime changes:** Yes (platform-specific runtime services).

### Risk 3: No stable C ABI
- **Root cause:** V1 has no FFI requirements.
- **If ignored:** Cannot integrate with Python, C++, C#, Rust, or any other language.
- **Blocks libraries:** No (MINK-to-MINK libraries work).
- **Blocks cross-platform:** No.
- **Blocks FFI:** Yes, completely.
- **Blocks AI tooling:** No.
- **Requires compiler changes:** Yes (export/import syntax, ABI definition).
- **Requires runtime changes:** Yes (C-callable entry points).

### Risk 4: Fixed 1 MiB heap
- **Root cause:** V1 runtime is deterministic and simple.
- **If ignored:** Real-world programs cannot run (memory exhaustion at 1 MiB).
- **Blocks libraries:** Yes (any non-trivial library needs dynamic memory).
- **Blocks cross-platform:** No.
- **Blocks FFI:** No.
- **Blocks AI tooling:** No.
- **Requires compiler changes:** No.
- **Requires runtime changes:** Yes (dynamic allocator, OS memory API).

### Risk 5: Single-threaded runtime
- **Root cause:** V1 focuses on correctness, not concurrency.
- **If ignored:** Cannot build servers, UIs, or any concurrent application.
- **Blocks libraries:** Yes for concurrent libraries.
- **Blocks cross-platform:** No.
- **Blocks FFI:** No.
- **Blocks AI tooling:** No.
- **Requires compiler changes:** No.
- **Requires runtime changes:** Yes (threading primitives, scheduler).

---

## 27. Risks

| Risk | Severity | Likelihood | Impact | Mitigation |
|------|----------|------------|--------|------------|
| No package system | Critical | Certain | No ecosystem | Design before implementation |
| Single target | High | Certain | No cross-platform | Design platform model early |
| No C ABI | High | Certain | No FFI | Design and freeze before FFI work |
| Fixed heap | High | Certain | No real programs | Implement dynamic allocator |
| Single-threaded | Medium | Certain | No concurrent apps | Design concurrency model |
| Lexical lifetimes | Medium | Certain | Limited expressiveness | Defer to V2 (NLL) |
| No traits/interfaces | Medium | Certain | Limited polymorphism | Defer to V2 |
| Monomorphization at AST level | Low | Certain | Less efficient generics | Defer to V2 (HIR-level) |
| No incremental compilation | Low | Certain | Slow builds for large projects | Defer to V2 |
| Single-crate compiler | Low | Certain | Limits parallel compilation | Split when interfaces stabilize |

---

## 28. Required Foundation Changes

### Category A — Must change before ecosystem implementation:

1. **A-1: C ABI design.** Define the stable C ABI, type mapping, calling conventions, and export/import syntax. This is a *design* document, not a code change. Must be frozen before any FFI work.

2. **A-2: Package architecture design.** Define `mink.toml` format, package identity, dependency declaration, module system semantics, and lockfile format. This is a *design* document. Must be frozen before package manager implementation.

3. **A-3: Cross-platform design.** Define target triple format, platform abstraction model, conditional compilation syntax, and platform detection. This is a *design* document. Must be frozen before multi-target implementation.

### Category B — Should change before ecosystem implementation:

4. **B-1: Dynamic heap allocator.** Replace the fixed 1 MiB arena with an OS-level allocator (mmap/VirtualAlloc). This is a runtime change, not a compiler change.

5. **B-2: JSON diagnostic output.** Add `--json` flag to `mink check` and `mink build` for machine-readable diagnostics. Small compiler change.

6. **B-3: Standard library architecture.** Define module naming, namespace conventions, and the boundary between compiler-provided and package-provided functionality. Design document.

7. **B-4: Security model design.** Define package integrity, build script sandboxing, and dependency trust model. Design document.

8. **B-5: Error code documentation.** Document every existing error code (E-L01 through E-B12, E-S10 through E-S14, E-R01 through E-R10, E-T01 through E-T41) with explanation, examples, and suggested fixes.

---

## 29. Deferred Work

### Category C — Can be implemented alongside ecosystem work:
- Formatter (C-1)
- Linter (C-2)
- `mink run` command (C-3)
- `mink test` command (C-4)
- Linux target implementation (C-5)
- macOS target implementation (C-6)
- Dynamic heap implementation (C-7, after design)
- Thread support (C-8, after design)

### Category D — Postponed:
- Incremental compilation (D-1)
- Parallel compilation (D-2)
- Workspace/crate split (D-3)
- LSP implementation (D-4)
- Debugger integration (D-5)
- Async/await syntax (D-6)
- Trait system (D-7)

### Category E — V2/V3 language features:
- Non-lexical lifetimes (E-1)
- Traits/interfaces (E-2)
- Async/await (E-3)
- HIGHER-kinded types (E-4)
- Const generics (E-5)
- Pattern matching on structs (E-6, partially implemented)

### Category F — Explicitly rejected:
- Garbage collection (F-1, ownership model is the safety strategy)
- Exception-based error handling (F-2, Result<T,E> is the model)
- Runtime reflection (F-3, compile-time analysis preferred)

---

## 30. Recommended Architecture

The recommended architecture for MINK's ecosystem is:

1. **Keep the V1 compiler pipeline unchanged.** It is sound and does not need modification for ecosystem construction.

2. **Add a C ABI layer** as the FFI foundation. This is the most critical missing piece for MINK's strategic goal of interoperability.

3. **Add a package system** (Cargo-inspired) as the distribution mechanism. This enables library sharing and dependency management.

4. **Add cross-platform support** (Linux, then macOS) as the deployment foundation. This enables MINK to be used beyond Windows.

5. **Add a dynamic heap allocator** as the runtime foundation. This enables real-world programs.

6. **Design but defer** advanced features (traits, async, NLL, incremental compilation) to V2.

The architecture preserves MINK's strengths (deterministic compilation, sound ownership, zero dependencies, structured diagnostics) while enabling the ecosystem to grow.

---

## 31. Implementation Dependency Graph

```
A-1 (C ABI design)
    ├── B-5 (Error code docs)
    └── C-5 (Linux target)

A-2 (Package architecture design)
    ├── C-3 (mink run)
    ├── C-4 (mink test)
    └── C-7 (Dynamic heap)

A-3 (Cross-platform design)
    ├── C-5 (Linux target)
    └── C-6 (macOS target)

B-1 (Dynamic heap)
    └── (enables real-world programs)

B-2 (JSON diagnostics)
    └── (enables AI tooling)

B-3 (Standard library architecture)
    └── (enables library ecosystem)

B-4 (Security model)
    └── (enables safe package distribution)
```

**Critical path:** A-1 → A-2 → A-3 → B-1 → C-5/C-6

---

## 32. Post-V1 Roadmap

### Phase A: Foundation Design (Next Session)
- Design C ABI (A-1)
- Design package architecture (A-2)
- Design cross-platform model (A-3)
- Design standard library architecture (B-3)
- Design security model (B-4)
- Document all error codes (B-5)

### Phase B: Core Infrastructure
- Implement dynamic heap allocator (B-1)
- Implement `mink.toml` parsing (A-2 foundation)
- Implement JSON diagnostic output (B-2)
- Implement `mink run` and `mink test` (C-3, C-4)

### Phase C: Package System
- Implement module system with separate compilation (A-2)
- Implement dependency resolver
- Implement lockfile
- Implement `mink init`, `mink add`, `mink build` (with dependencies)

### Phase D: Cross-Platform
- Implement x86_64-linux-elf target (C-5)
- Implement platform abstraction layer
- Implement aarch64-linux-elf target
- Implement aarch64-apple-darwin target

### Phase E: FFI Foundation
- Implement C ABI export/import
- Implement `extern "C"` syntax
- Implement header generation
- Implement `mink bind` tool

### Phase F: Standard Library
- Implement core module (collections, strings, math)
- Implement system module (fs, process, env)
- Implement network module (TCP, UDP, HTTP)

### Phase G: Python Integration
- Implement CPython extension module bridge
- Implement buffer sharing
- Implement `mink-bind` for Python

### Phase H: Developer Tooling
- Implement formatter
- Implement linter
- Implement LSP (basic)

### Phase I: C/C++ Integration
- Implement C++ header generation
- Implement RAII wrappers

### Phase J: C#/.NET Integration
- Implement P/Invoke bindings
- Implement SafeHandle wrappers

---

## 33. Exact Next Milestone

**Immediate next milestone: Design Session**

In the next session, produce the following design documents:

1. **C ABI Specification** — type mapping, calling conventions, export/import syntax, struct layout rules, string representation, error handling across the boundary.

2. **Package Architecture Specification** — `mink.toml` format, package identity, module system semantics, dependency declaration, lockfile format, workspace model.

3. **Cross-Platform Specification** — target triple format, platform abstraction model, conditional compilation syntax, platform detection, runtime services per platform.

4. **Standard Library Architecture** — module naming, namespace conventions, compiler-provided vs. package-provided functionality, API conventions.

5. **Security Model** — package integrity, build script sandboxing, dependency trust, supply chain verification.

These are all *design* documents, not code. They freeze the architecture before implementation begins.

---

## 34. Final Go/No-Go Decision

### Decision: CONDITIONAL GO

**MINK's V1 architecture is sufficient to begin ecosystem construction**, provided:

1. The three Category-A design documents (C ABI, package architecture, cross-platform model) are produced and frozen before implementation begins.

2. The dynamic heap allocator (B-1) is implemented early, as it unblocks real-world programs.

3. The existing compiler pipeline, type system, ownership model, and runtime are NOT modified unless a genuine architectural defect is discovered.

**The V1 architecture is NOT a blocker.** The pipeline is sound. The type system is sound. The ownership model is sound. The code generation is deterministic. What's missing is ecosystem infrastructure (packages, FFI, cross-platform), not language foundations.

**MINK is ready to build the ecosystem.**

---

*This audit was produced by Buffy (Codebuff) on August 25, 2026.*
*Repository state: v1.0.0, commit aaf8866, 163 tests passing, clean working tree.*
