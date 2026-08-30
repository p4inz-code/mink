# MINK Ecosystem Dependency Graph

**Version:** 0.1.0 (Design Draft)
**Date:** August 25, 2026
**Status:** DESIGN ONLY — not implemented
**Scope:** Dependency graph, implementation boundary, and staged roadmap

---

## 1. Ecosystem Layer Dependency Graph

### Complete dependency map

```
                        ┌─────────────────────────────────────┐
                        │        L11: AI Infrastructure       │
                        │  JSON diagnostics, structured errors│
                        │  machine-readable metadata          │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │        L10: Domain Libraries        │
                        │  Web, database, ML, gaming          │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L9: Developer Tooling          │
                        │  LSP, formatter, linter, debugger   │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L8: FFI/Interop                │
                        │  C ABI, Python, C++, C#, Rust       │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L7: Registry                   │
                        │  Package hosting, versioning        │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L6: Package Manager            │
                        │  Dependencies, lockfile, build      │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L5: Module System              │
                        │  Package identity, visibility       │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L4: Standard Library           │
                        │  Collections, strings, I/O, net     │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L3: Core Library               │
                        │  Option, Result, primitives         │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L2: Runtime                    │
                        │  ABI, heap, intrinsics              │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L1: Compiler                   │
                        │  Lexer, parser, type checker, etc.  │
                        └──────────────┬──────────────────────┘
                                       │ depends on
                        ┌──────────────▼──────────────────────┐
                        │      L0: Language                   │
                        │  Syntax, semantics, type system     │
                        └─────────────────────────────────────┘
```

### Cross-cutting concerns

```
┌─────────────────────────────────────────────────────────────┐
│                    Security (all layers)                     │
│  Package integrity, build sandboxing, supply chain          │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                  Cross-Platform (all layers)                 │
│  Windows, Linux, macOS, future targets                      │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                  AI-Friendly (all layers)                    │
│  JSON output, stable error codes, machine-readable metadata │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. What Must Exist Before Each Layer

### L0 (Language) — EXISTS (V1.0.0)
- ✅ Syntax
- ✅ Semantics
- ✅ Type system
- ✅ Ownership model
- ✅ Pattern matching
- ✅ Generics (monomorphization)
- ✅ Closures

### L1 (Compiler) — EXISTS (V1.0.0)
- ✅ Lexer
- ✅ Parser
- ✅ AST
- ✅ Monomorphizer
- ✅ Semantic analysis
- ✅ Type checking
- ✅ Ownership analysis
- ✅ HIR lowering
- ✅ MIR lowering
- ✅ MIR optimization
- ✅ Backend lowering
- ✅ x86_64 code generation
- ✅ PE container builder
- ✅ Runtime embedding

### L2 (Runtime) — EXISTS (V1.0.0)
- ✅ ABI (internal)
- ✅ Heap (fixed 1 MiB arena)
- ✅ Intrinsics (rt_alloc, rt_free, rt_mem_*, rt_str_*, rt_vec_*)
- ⬜ Dynamic heap (needs design + implementation)
- ⬜ Platform abstraction (needs design + implementation)

### L3 (Core Library) — PARTIALLY EXISTS
- ✅ Option<T> (type only, no methods)
- ✅ Result<T,E> (type only, no methods)
- ✅ Vec<T> (runtime intrinsics only, no methods)
- ✅ String operations (runtime intrinsics)
- ⬜ Option methods (needs implementation)
- ⬜ Result methods (needs implementation)
- ⬜ Vec methods (needs implementation)
- ⬜ String formatting (needs implementation)

### L4 (Standard Library) — NOT STARTED
- ⬜ Collections (HashMap, HashSet)
- ⬜ Strings (extended operations)
- ⬜ Math
- ⬜ Time
- ⬜ Filesystem
- ⬜ Process
- ⬜ Environment
- ⬜ Networking
- ⬜ Concurrency

### L5 (Module System) — NOT STARTED
- ⬜ Package identity (mink.toml)
- ⬜ Module declarations
- ⬜ Visibility rules
- ⬜ Import/export system
- ⬜ Separate compilation

### L6 (Package Manager) — NOT STARTED
- ⬜ Dependency resolution
- ⬜ Lockfile
- ⬜ Build system
- ⬜ CLI commands (init, add, build, test, run)

### L7 (Registry) — NOT STARTED
- ⬜ Package hosting
- ⬜ Version management
- ⬜ Integrity verification
- ⬜ Search

### L8 (FFI/Interop) — NOT STARTED
- ⬜ C ABI specification
- ⬜ Export/import syntax
- ⬜ Header generation
- ⬜ Python integration
- ⬜ C++ integration
- ⬜ C# integration
- ⬜ Rust integration

### L9 (Developer Tooling) — NOT STARTED
- ⬜ Formatter
- ⬜ Linter
- ⬜ LSP
- ⬜ Debugger

### L10 (Domain Libraries) — NOT STARTED
- ⬜ JSON library
- ⬜ Web framework
- ⬜ Database drivers
- ⬜ ML inference

### L11 (AI Infrastructure) — NOT STARTED
- ⬜ JSON diagnostic output
- ⬜ Structured error codes
- ⬜ Machine-readable metadata
- ⬜ Compiler introspection

---

## 3. Circular Dependency Prevention

### Rules
1. Each layer may only depend on layers below it
2. No upward dependencies (L4 cannot depend on L6)
3. No lateral dependencies between unrelated layers (L8 cannot depend on L4)
4. Cross-cutting concerns (security, cross-platform, AI) are orthogonal

### Dependency validation

```
L0 ← L1 ← L2 ← L3 ← L4 ← L5 ← L6 ← L7
                                  ↑
                              L8 (FFI depends on L5 module system)
                              L9 (Tooling depends on L5 module system)
                              L10 (Libraries depend on L4 standard library)
                              L11 (AI depends on L1 compiler)
```

### Exception: FFI/Interop (L8)
- L8 depends on L2 (runtime) for ABI
- L8 does NOT depend on L4 (standard library)
- This ensures FFI works without the full standard library

### Exception: AI Infrastructure (L11)
- L11 depends on L1 (compiler) for diagnostics
- L11 does NOT depend on L4 (standard library)
- This ensures AI tooling works with minimal dependencies

---

## 4. Implementation Boundary

### IMPLEMENT NOW (Session 51+)
None. This session is design-only.

### DESIGN ONLY (This session)
- ✅ C ABI specification
- ✅ Package architecture
- ✅ Cross-platform architecture
- ✅ Standard library architecture
- ✅ Security architecture
- ✅ Interoperability roadmap
- ✅ AI developer architecture
- ✅ Library priority framework
- ✅ Ecosystem dependency graph

### AFTER PACKAGE FOUNDATION
- Module system implementation
- Package manager implementation
- Dependency resolver
- Lockfile management
- `mink init`, `mink add`, `mink build` (with dependencies)

### AFTER CROSS-PLATFORM FOUNDATION
- Linux x86_64 target implementation
- Linux ARM64 target implementation
- macOS ARM64 target implementation
- Platform abstraction layer
- Conditional compilation

### AFTER ABI FOUNDATION
- C ABI export/import syntax
- C header generation
- `mink bind` tool
- Exported deallocation functions

### LIBRARY PHASE
- JSON library (#1)
- Filesystem library (#2)
- Strings extended operations (#3)
- Collections (HashMap, HashSet) (#4)
- Math library (#5)

### LATER
- Process management
- CLI argument parsing
- Testing framework
- Time library
- Encoding (Base64, hex, TOML)
- Networking (TCP, UDP)
- Concurrency (threads, mutexes)
- Logging
- Regex
- Compression
- Serialization framework

### V2+ (Language evolution required)
- Traits/interfaces
- Async/await
- Non-lexical lifetimes
- Higher-kinded types
- Const generics

### NOT PLANNED
- Garbage collection (ownership model is the safety strategy)
- Exception-based error handling (Result<T,E> is the model)
- Runtime reflection (compile-time analysis preferred)

---

## 5. Critical Path

```
V1.0.0 (exists)
    │
    ▼
[Session 51-52] Core primitive methods + JSON library
    │
    ▼
[Session 53-54] Dynamic heap allocator + Platform abstraction
    │
    ▼
[Session 55-56] Linux x86_64 target
    │
    ▼
[Session 57-58] C ABI implementation + Export/import syntax
    │
    ▼
[Session 59-60] Filesystem + Strings library
    │
    ▼
[Session 61-62] Package system (mink.toml, module system)
    │
    ▼
[Session 63-64] Package manager (dependency resolution, lockfile)
    │
    ▼
[Session 65-66] Collections (HashMap, HashSet)
    │
    ▼
[Session 67-68] Process + CLI + Testing
    │
    ▼
[Session 69-70] macOS ARM64 + Linux ARM64
    │
    ▼
[Session 71-72] Python integration
    │
    ▼
[Session 73-74] C++ integration
    │
    ▼
[Session 75-76] C# integration
    │
    ▼
[Session 77-78] Registry
    │
    ▼
[Session 79-80] Developer tooling (formatter, linter, LSP)
    │
    ▼
[Session 81+] Domain libraries, AI infrastructure, ecosystem maturity
```

---

## 6. Staged Roadmap

### STAGE 0: Architecture Foundation (THIS SESSION)
- Design C ABI specification ✅
- Design package architecture ✅
- Design cross-platform architecture ✅
- Design standard library architecture ✅
- Design security architecture ✅
- Design interoperability roadmap ✅
- Design AI developer architecture ✅
- Design library priority framework ✅
- Design ecosystem dependency graph ✅

### STAGE 1: Required Infrastructure (Sessions 51-56)
- Implement Option/Result/Vec methods
- Implement string formatting
- Implement dynamic heap allocator
- Implement platform abstraction
- Implement Linux x86_64 target
- Add JSON diagnostic output
- Add error code documentation

### STAGE 2: Library #1 — JSON (Sessions 57-60)
- Implement JSON parser (recursive descent, zero-copy)
- Implement JSON serializer (streaming, efficient)
- Implement C ABI export/import syntax
- Implement C header generation
- Add JSON to standard library
- Document, test, benchmark, security audit

### STAGE 3: Library #2 — Filesystem + Strings (Sessions 61-66)
- Implement Path type
- Implement File operations
- Implement directory operations
- Implement extended string operations
- Implement package system (mink.toml, module system)
- Implement package manager
- Document, test, benchmark, security audit

### STAGE 4: Library #3 — Collections + Math (Sessions 67-72)
- Implement HashMap<K,V>
- Implement HashSet<T>
- Implement basic math operations
- Implement process management
- Implement CLI argument parsing
- Implement testing framework
- Document, test, benchmark, security audit

### STAGE 5: Library #4 — Cross-Platform + Interop (Sessions 73-78)
- Implement macOS ARM64 target
- Implement Linux ARM64 target
- Implement Python integration
- Implement C++ integration
- Implement C# integration
- Document, test, benchmark, security audit

### STAGE 6: Library #5 — Application Libraries (Sessions 79-84)
- Implement time library
- Implement encoding (Base64, hex, TOML)
- Implement networking (TCP, UDP)
- Implement logging
- Implement registry
- Document, test, benchmark, security audit

### STAGE 7: Ecosystem Maturity (Sessions 85+)
- Implement developer tooling (formatter, linter, LSP)
- Implement domain libraries (web, database, ML)
- Implement AI infrastructure
- Ecosystem growth and community

### STAGE 8: Language Evolution (V2+)
- Traits/interfaces
- Async/await
- Non-lexical lifetimes
- Higher-kinded types
- Const generics

---

## 7. No Circular Dependencies

### Verified dependency order

```
Language (L0)
    ↓
Compiler (L1)
    ↓
Runtime (L2)
    ↓
Core Library (L3)
    ↓
Standard Library (L4)
    ↓
Module System (L5)
    ↓
Package Manager (L6)
    ↓
Registry (L7)
    ↓
FFI/Interop (L8) ← depends on L2 (runtime), NOT L4
    ↓
Developer Tooling (L9) ← depends on L5 (module system), NOT L4
    ↓
Domain Libraries (L10) ← depends on L4 (standard library)
    ↓
AI Infrastructure (L11) ← depends on L1 (compiler), NOT L4
```

### No upward dependencies
- L4 (standard library) does NOT depend on L6 (package manager)
- L8 (FFI) does NOT depend on L4 (standard library)
- L11 (AI) does NOT depend on L4 (standard library)

### No lateral dependencies
- L8 (FFI) does NOT depend on L9 (tooling)
- L10 (domain libraries) does NOT depend on L8 (FFI)
- L11 (AI) does NOT depend on L10 (domain libraries)

---

*This specification is part of the MINK Ecosystem Architecture Design Pack (Session 50).*
*Do NOT implement until the design is frozen and reviewed.*
