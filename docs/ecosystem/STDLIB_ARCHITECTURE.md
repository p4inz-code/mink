# MINK Standard Library Architecture

**Version:** 0.1.0 (Design Draft)
**Date:** August 25, 2026
**Status:** DESIGN ONLY — not implemented
**Scope:** Complete standard library design for the MINK ecosystem

---

## 1. Goals

1. **Coherent system.** The standard library is a carefully designed set of modules with clear dependencies and interfaces, not a collection of random convenience functions.

2. **Layered architecture.** Modules are organized in layers: core → memory → collections → strings → system → network → dev.

3. **Cross-platform.** Every module works on all supported platforms (Windows, Linux, macOS).

4. **Production-ready.** Every module has documentation, tests, benchmarks, and security review.

5. **AI-friendly.** Machine-readable metadata, deterministic behavior, structured errors.

---

## 2. Library Layers

### Layer model

```
┌─────────────────────────────────────────────┐
│  L7: Domain Libraries (external packages)   │
│  Web, database, ML, gaming                  │
├─────────────────────────────────────────────┤
│  L6: Developer Tools                        │
│  Testing, logging, CLI, diagnostics         │
├─────────────────────────────────────────────┤
│  L5: Application Libraries                  │
│  Serialization, compression, crypto         │
├─────────────────────────────────────────────┤
│  L4: System Libraries                       │
│  Filesystem, process, networking, threads   │
├─────────────────────────────────────────────┤
│  L3: Core Libraries                         │
│  Collections, strings, math, time, encoding │
├─────────────────────────────────────────────┤
│  L2: Core Primitives                        │
│  Option, Result, Int, Bool, Float, Ptr      │
├─────────────────────────────────────────────┤
│  L1: Runtime                                │
│  ABI, heap, intrinsics, platform abstraction│
├─────────────────────────────────────────────┤
│  L0: Compiler                               │
│  Lexer, parser, type checker, code generator│
└─────────────────────────────────────────────┘
```

### Dependency rule
- Each layer may only depend on layers below it
- No circular dependencies between layers
- L2 (core primitives) depends only on L1 (runtime)
- L3 (core libraries) depends on L1 + L2
- L4 (system libraries) depends on L1 + L2 + L3

---

## 3. Module Classification

### Core (L2) — Compiler-provided

These are implemented by the compiler and runtime, not as MINK source:

| Module | Description | Priority |
|--------|-------------|----------|
| `option` | `Option<T>` type and methods | **Core** |
| `result` | `Result<T,E>` type and methods | **Core** |
| `primitives` | `Int`, `Bool`, `Float`, `Char`, `Ptr<T>` | **Core** |
| `unit` | `()` type | **Core** |

### Memory (L1-L2) — Runtime-provided

| Module | Description | Priority |
|--------|-------------|----------|
| `alloc` | Heap allocation (`rt_alloc`, `rt_free`) | **Core** |
| `mem` | Memory operations (`rt_mem_load`, `rt_mem_store`) | **Core** |

### Collections (L3) — Standard Library

| Module | Description | Priority |
|--------|-------------|----------|
| `vec` | Dynamic array (`Vec<T>`) | **High** |
| `slice` | Borrowed slice (`Slice<T>`) | **High** |
| `string` | UTF-8 string (`Str`) | **High** |
| `hashmap` | Hash map (`HashMap<K,V>`) | **High** |
| `hashset` | Hash set (`HashSet<T>`) | **Medium** |
| `btree_map` | B-tree map (`BTreeMap<K,V>`) | **Later** |
| `btree_set` | B-tree set (`BTreeSet<T>`) | **Later** |
| `linked_list` | Linked list (`LinkedList<T>`) | **Later** |
| `deque` | Double-ended queue (`Deque<T>`) | **Later** |
| `priority_queue` | Priority queue (`PriorityQueue<T>`) | **Later** |

### Strings (L3) — Standard Library

| Module | Description | Priority |
|--------|-------------|----------|
| `str` | String operations (concat, eq, from_int) | **Core** (runtime intrinsics) |
| `fmt` | String formatting (`format!`, `Display`) | **High** |
| `unicode` | Unicode support (codepoints, graphs) | **Medium** |
| `regex` | Regular expressions | **Later** |

### Math (L3) — Standard Library

| Module | Description | Priority |
|--------|-------------|----------|
| `math` | Basic math (abs, min, max, pow, sqrt) | **High** |
| `random` | Random number generation | **Medium** |
| `bigint` | Arbitrary-precision integers | **Later** |
| `bignum` | Arbitrary-precision floats | **Later** |
| `statistics` | Statistical functions | **Later** |
| `complex` | Complex numbers | **Later** |

### Time (L3) — Standard Library

| Module | Description | Priority |
|--------|-------------|----------|
| `time` | Duration, Instant, SystemTime | **High** |
| `datetime` | DateTime, timezone support | **Medium** |
| `chrono` | Calendar operations | **Later** |

### Filesystem (L4) — System Library

| Module | Description | Priority |
|--------|-------------|----------|
| `fs` | File operations (open, read, write, delete) | **High** |
| `path` | Path manipulation (join, parent, filename) | **High** |
| `dir` | Directory operations (create, list, remove) | **High** |
| `fs_watcher` | Filesystem watcher | **Later** |
| `tempfile` | Temporary files | **Medium** |

### Process (L4) — System Library

| Module | Description | Priority |
|--------|-------------|----------|
| `process` | Process spawn, wait, kill | **High** |
| `env` | Environment variables | **High** |
| `args` | Command-line arguments | **High** |

### Networking (L4) — System Library

| Module | Description | Priority |
|--------|-------------|----------|
| `net` | TCP/UDP sockets | **Medium** |
| `http` | HTTP client/server | **Later** |
| `dns` | DNS resolution | **Later** |
| `tls` | TLS/SSL support | **Later** |

### Concurrency (L4) — System Library

| Module | Description | Priority |
|--------|-------------|----------|
| `thread` | Thread creation and management | **Medium** |
| `sync` | Mutex, RwLock, Condvar | **Medium** |
| `channel` | Message passing channels | **Later** |
| `async` | Async/await runtime | **Later** (V2 language feature) |

### Encoding (L5) — Application Library

| Module | Description | Priority |
|--------|-------------|----------|
| `base64` | Base64 encoding/decoding | **Medium** |
| `hex` | Hex encoding/decoding | **Medium** |
| `json` | JSON parsing/serialization | **High** |
| `toml` | TOML parsing/serialization | **Medium** |
| `csv` | CSV parsing/serialization | **Medium** |
| `xml` | XML parsing/serialization | **Later** |
| `yaml` | YAML parsing/serialization | **Later** |

### Compression (L5) — Application Library

| Module | Description | Priority |
|--------|-------------|----------|
| `gzip` | Gzip compression | **Later** |
| `zlib` | Zlib compression | **Later** |
| `lz4` | LZ4 compression | **Later** |

### Crypto (L5) — Application Library

| Module | Description | Priority |
|--------|-------------|----------|
| `hash` | Hash functions (SHA-256, MD5) | **Medium** |
| `hmac` | HMAC | **Later** |
| `aes` | AES encryption | **Later** |
| `rsa` | RSA encryption | **Later** |
| `ed25519` | Ed25519 signatures | **Later** |

### Serialization (L5) — Application Library

| Module | Description | Priority |
|--------|-------------|----------|
| `serde` | Serialization framework | **Later** (V2 language feature: traits) |
| `bincode` | Binary encoding | **Later** |
| `protobuf` | Protocol Buffers | **Later** |

### Diagnostics (L6) — Developer Tools

| Module | Description | Priority |
|--------|-------------|----------|
| `log` | Logging (levels, structured) | **Medium** |
| `diagnostics` | Machine-readable diagnostics | **High** |
| `explain` | Error code explanation | **High** |

### Testing (L6) — Developer Tools

| Module | Description | Priority |
|--------|-------------|----------|
| `test` | Test framework (assert, test runner) | **High** |
| `property` | Property-based testing | **Later** |
| `mock` | Mocking framework | **Later** |
| `bench` | Benchmarking framework | **Later** |

### CLI (L6) — Developer Tools

| Module | Description | Priority |
|--------|-------------|----------|
| `cli` | Argument parsing | **Medium** |
| `progress` | Progress bars | **Later** |
| `colors` | Terminal colors | **Later** |
| `table` | Table formatting | **Later** |

---

## 4. Module Dependencies

### Dependency graph

```
option (core)
result (core)
  └── option (core)

vec (collections)
  └── option (core)
  └── result (core)

string (strings)
  └── vec (collections)
  └── option (core)
  └── result (core)

hashmap (collections)
  └── option (core)
  └── result (core)

math (math)
  └── option (core)
  └── result (core)

time (time)
  └── option (core)
  └── result (core)

fs (filesystem)
  └── string (strings)
  └── path (filesystem)
  └── result (core)

path (filesystem)
  └── string (strings)
  └── option (core)

process (process)
  └── string (strings)
  └── vec (collections)
  └── result (core)

env (process)
  └── string (strings)
  └── option (core)

net (networking)
  └── string (strings)
  └── result (core)

thread (concurrency)
  └── result (core)

json (encoding)
  └── string (strings)
  └── vec (collections)
  └── result (core)

test (testing)
  └── string (strings)
  └── result (core)

log (diagnostics)
  └── string (strings)
  └── time (time)

cli (cli)
  └── string (strings)
  └── vec (collections)
  └── result (core)
```

### Rules
1. No circular dependencies
2. Each module declares its dependencies explicitly
3. Dependencies are always at the same or lower layer
4. Core modules (L2) have no dependencies on higher layers

---

## 5. API Conventions

### Naming
- `snake_case` for functions, methods, and modules
- `PascalCase` for types and traits
- `SCREAMING_SNAKE_CASE` for constants
- No abbreviations (use `string` not `str`, `function` not `fn`)

### Error handling
- All fallible operations return `Result<T, E>`
- No panics in library code (only in tests and debug assertions)
- Error types are structured with error codes and messages

### Ownership
- Library functions follow MINK's ownership model
- Borrowed parameters use `&T` or `&mut T`
- Owned parameters use `T` (move semantics)
- No hidden allocations (explicit `alloc`/`free`)

### Documentation
- Every public API has a doc comment
- Doc comments include: purpose, parameters, returns, errors, examples
- Examples are runnable (tested in CI)

### Testing
- Every public API has unit tests
- ≥90% code coverage
- All tests pass on all platforms
- Performance-critical APIs have benchmarks

---

## 6. V1 Existing Modules

### What exists today

| Module | Location | Status |
|--------|----------|--------|
| `Option<T>` | `stdlib/option.mink` | Implemented, no methods |
| `Result<T,E>` | `stdlib/result.mink` | Implemented, no methods |
| `Vec<T>` | Runtime intrinsics | Implemented (`rt_vec_*`) |
| String ops | Runtime intrinsics | Implemented (`rt_str_*`) |

### V1 gaps
1. `Option<T>` has no methods (`.unwrap()`, `.map()`, `.and_then()`)
2. `Result<T,E>` has no methods (`.unwrap()`, `.map()`, `.unwrap_or()`)
3. No `Vec<T>` methods (`.push()`, `.len()`, `.get()`)
4. No string formatting
5. No collections beyond `Vec<T>`
6. No math operations
7. No time operations
8. No filesystem operations
9. No process operations
10. No networking

---

## 7. V1→V2 Evolution Plan

### Phase 1: Core method implementations (V1.x)
- Implement `Option<T>` methods (`.unwrap()`, `.map()`, `.is_some()`, `.is_none()`)
- Implement `Result<T,E>` methods (`.unwrap()`, `.map()`, `.is_ok()`, `.is_err()`)
- Implement `Vec<T>` methods (`.push()`, `.len()`, `.get()`, `.iter()`)
- Implement string formatting (`format!` macro or function)

### Phase 2: Core libraries (V2)
- Implement `math` module (abs, min, max, pow, sqrt)
- Implement `time` module (Duration, Instant)
- Implement `path` module (join, parent, filename)
- Implement `string` module (split, trim, contains, replace)

### Phase 3: System libraries (V2+)
- Implement `fs` module (open, read, write, delete)
- Implement `process` module (spawn, wait, kill)
- Implement `env` module (get, set, remove)
- Implement `test` module (assert, test runner)

### Phase 4: Application libraries (V3+)
- Implement `json` module (parse, stringify)
- Implement `net` module (TCP, UDP)
- Implement `log` module (levels, structured)
- Implement `cli` module (argument parsing)

---

## 8. Cross-Platform Considerations

### Platform-specific implementations
- Each module may have platform-specific implementations
- The public API is identical across platforms
- Platform differences are isolated to internal implementation

### Example: filesystem

```mink
// Public API (identical on all platforms)
pub fn read(path: Str) -> Result<Str, IoError>;
pub fn write(path: Str, data: Str) -> Result<(), IoError>;
pub fn delete(path: Str) -> Result<(), IoError>;

// Internal implementations (platform-specific)
#[cfg(target_os = "windows")]
mod windows {
    fn read_impl(path: Str) -> Result<Str, IoError> {
        // Win32 API: CreateFile, ReadFile, CloseHandle
    }
}

#[cfg(target_os = "linux")]
mod linux {
    fn read_impl(path: Str) -> Result<Str, IoError> {
        // POSIX: open, read, close
    }
}
```

---

## 9. Package System Integration

### Module naming
- Standard library modules are in the `mink-std` package
- Import with `use mink_std::collections::Vec;`
- External packages import with `use <package>::<module>::<item>;`

### Feature flags
- The standard library uses feature flags for optional modules
- Default features include core modules (option, result, vec, string)
- Advanced modules (net, crypto, async) require explicit features

```toml
[dependencies]
mink-std = { version = "1.0.0", features = ["collections", "strings", "math"] }
```

### Versioning
- The standard library version is independent of the compiler version
- `mink-std 1.x` works with compiler `1.y` where `y >= x`
- Breaking changes require a major version bump

---

## 10. AI-Friendly Design

### Machine-readable metadata
- Every module has structured metadata (name, version, description, API surface)
- The compiler can introspect module APIs
- AI agents can discover and use modules programmatically

### Deterministic behavior
- All library functions are deterministic (same input → same output)
- Random number generation uses explicit seeds
- Time functions can be mocked for testing

### Structured errors
- All errors have stable codes
- Errors include source locations and suggested fixes
- Errors are machine-readable (JSON output mode)

---

## 11. Two-Approach Analysis

### Approach A: Minimal core + packages (CHOSEN)

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Fast compiler builds, clear boundaries, modular, no bloat |
| **Cons** | Users must install packages for basics, potential fragmentation |
| **Complexity** | Low |
| **Performance** | Good (only linked modules are compiled) |
| **Security** | Smaller attack surface |
| **Compatibility** | Excellent — users choose what they need |
| **Maintainability** | Each module is independently maintained |
| **Ecosystem impact** | Maximum — encourages package ecosystem growth |

### Approach B: Large stdlib

| Criterion | Assessment |
|-----------|------------|
| **Pros** | Everything available immediately, no package hunting |
| **Cons** | Slow builds, bloat, hard to maintain, stagnation |
| **Complexity** | High |
| **Performance** | Poor (large compilation units) |
| **Security** | Larger attack surface |
| **Compatibility** | Limited — users get everything whether they need it or not |
| **Maintainability** | Difficult — monolithic library is hard to evolve |
| **Ecosystem impact** | Negative — discourages external package development |

### Decision: **Approach A**

**Reasoning:** MINK's ecosystem strategy requires a thriving package ecosystem. A minimal core library encourages package development. The standard library provides essential building blocks; external packages provide everything else. This is the Rust model, and it works extremely well.

---

*This specification is part of the MINK Ecosystem Architecture Design Pack (Session 50).*
*Do NOT implement until the design is frozen and reviewed.*
