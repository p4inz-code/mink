# MINK — Compiler Architecture

**Status:** Planning / Specification
**Version:** 0.1.0

## 1. Objective

The MINK compiler transforms valid MINK source into efficient executable output while providing fast builds, strong diagnostics, incremental analysis, optimization, debugging, portability, and tooling integration.

The compiler must support correctness, performance, incremental compilation, cross-platform targets, strong diagnostics, IDE/LSP integration, AI-readable diagnostics, optimization, debugging, native interoperability, and long-term language evolution.

## 2. High-Level Pipeline

Source → Lexer → Parser → AST → Semantic Analysis → Type Analysis → HIR → MIR → Optimization → Backend / Code Generation → Executable (linked against the MINK runtime)

The stages through code generation are implemented (`src/hir/`, `src/mir/`, `src/backend/`, `docs/implementation/HIR_IMPLEMENTATION.md`, `docs/implementation/MIR_IMPLEMENTATION.md`, `docs/implementation/OPTIMIZATION_IMPLEMENTATION.md`, `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`): HIR is the typed, symbol-resolved, owned IR produced from the AST plus the semantic and type results, MIR is the control-flow-oriented IR (basic blocks, statements, and terminators) lowered from HIR and structurally validated, a deterministic, behavior-preserving optimization pipeline (boolean constant folding, copy propagation, CFG simplification, unreachable-block elimination, dead-code elimination) runs on the validated MIR with structural validation before the first pass and after every pass, and the native backend (`src/backend/`) lowers the optimized MIR into a portable backend instruction representation, verifies its structural integrity, and emits a machine image for a selected target. Generated images are linked against the MINK runtime (`src/runtime/`): a small, deterministic, safe-Rust runtime that provides process initialization, a bump-and-free-list heap, structured errors (`E-R01+`), and the intrinsic services (`rt_alloc`, `rt_free`, `rt_load`, `rt_store`, `rt_print_int`, `rt_exit`) the backend emits calls to. `mink check` validates, lowers, and optimizes through MIR; `mink build` continues through the backend and writes an executable.

Compiler stages must have clearly separated responsibilities.

## 3. Frontend

The frontend is responsible for lexical analysis, parsing, syntax representation, name resolution, type checking, semantic analysis, diagnostics, and source-aware tooling.

The lexer must preserve accurate source spans.

The parser must provide strong error recovery so useful analysis can continue after recoverable syntax errors.

Name resolution must handle variables, functions, types, modules, imports, generics, members, visibility, and shadowing.

Type checking must support primitive types, composite types, functions, generics, optional types, Result types, traits/interfaces, inference, conversions, pattern matching, async types, and FFI types.

## 4. Semantic Analysis

Semantic analysis must validate rules beyond syntax and basic typing.

It may include exhaustiveness, unreachable code, invalid control flow, ownership/lifetime rules, async restrictions, concurrency rules, visibility, constant evaluation, API compatibility, and security-sensitive patterns.

## 5. Intermediate Representations

MINK uses HIR after semantic analysis and MIR for lower-level optimization and code generation.

HIR retains information needed for diagnostics, desugaring, generic processing, and analysis: it is a typed, symbol-resolved, owned tree mirroring the source structure, with explicit control-flow nodes (`if`/`else`, loops, `break`/`continue`/`return`).

MIR makes control flow, values, calls, memory operations, and resource operations explicit enough for optimization: every function is a graph of basic blocks, each block is an ordered list of statements ending in exactly one terminator (return, jump, or conditional branch), and control-flow constructs have been lowered into explicit jumps and branches. MIR also carries the loop state machine for `for` loops (range iteration via `RangeNext`/`RangeFinished`) and preserves exact source spans and canonical types.

## 6. Optimization

An optimization stage is implemented (`src/mir/optimize.rs`, `docs/implementation/OPTIMIZATION_IMPLEMENTATION.md`): a composable pipeline of passes — boolean constant folding, copy propagation (redundant move elimination), CFG simplification, unreachable-block elimination, and dead-code elimination — runs to a fixpoint over validated MIR, with structural validation before the first pass and after every pass. Folding is deliberately limited to the boolean algebra because MIR constants carry no decoded literal values (`Bool(bool)` is the only value-carrying constant); see the optimization doc §2.

Potential future optimizations include constant propagation, common-subexpression elimination, inlining, devirtualization, escape analysis, allocation elimination, loop optimization, vectorization, and link-time optimization.

Optimizations must preserve observable language semantics.

Standard profiles should include development, debug, release, size-optimized, and performance-optimized configurations.

## 7. Backend

The backend (`src/backend/`) is implemented as a target-independent core plus a target-specific emission layer, so additional architectures can be added without touching lowering or verification:

- **Lowering** (`src/backend/lower.rs`) walks the optimized MIR once and produces a portable instruction representation (`src/backend/ir.rs`) — functions, typed locals, instructions, terminators, and statics — preserving deterministic source order and exact source spans. Everything outside the supported native subset (floating point, strings, characters, `null`, member/index places, function values, module bindings needing runtime initialization) is rejected with a structured `E-B01+` diagnostic instead of being miscompiled; lowering reports every independent problem in deterministic order.
- **Verification** (`src/backend/verify.rs`) defensively checks the lowered program's structural integrity (local and block references, operand types, terminator shape), so malformed or mutated instructions fail cleanly (`E-B07`) instead of panicking.
- **Emission** (`src/backend/emit/`) turns verified instructions into a machine image for a selected `Target`. The first milestone implements `x86_64-windows-pe`: a self-contained x86-64 code generator (`src/backend/emit/x86_64.rs`) plus a PE container builder (`src/backend/emit/pe.rs`) that assemble a complete Windows executable with no external toolchain. The code generator also assembles the runtime services it references — a machine-code runtime (`src/backend/emit/runtime.rs`) implementing process initialization, the deterministic bump/free-list heap, and the intrinsic services behind a fixed calling convention (`src/runtime/abi.rs`). The `x86_64-linux-elf` and `aarch64-linux-elf` targets are recognized but not yet implemented and are rejected with `E-B11`.

`mink build` validates the entry function (`fn main()` with no parameters; its integer/boolean result becomes the process exit code), lowers, verifies, and emits. Diagnostics carry stable codes `E-B01`…`E-B12`; see `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md` for the design and the supported subset.

## 8. Incremental Compilation

Incremental compilation is a major requirement.

Unaffected components must not be unnecessarily recompiled.

Caching may operate across modules, dependencies, generic instantiations, IR stages, and generated artifacts.

Cache invalidation must be correct and deterministic.

## 9. Parallel Compilation

Independent parsing, analysis, type checking, dependency analysis, and code generation work should execute concurrently where safe.

Compiler output should remain deterministic wherever practical.

## 10. Diagnostics

Diagnostics must originate from a structured diagnostic engine.

Each diagnostic should contain a stable code, severity, source span, related spans, message, explanation, root cause, suggested fixes, and documentation identifier.

Human-readable terminal output is only one presentation layer.

The compiler should minimize cascading errors and identify probable root causes.

Machine-applicable fixes should be supported where safety can be established.

## 11. AI Compiler Interface

The compiler must expose machine-readable diagnostics for AI coding systems.

AI diagnostics should provide error code, exact location, root cause, expected state, actual state, related locations, suggested repair, relevant symbols, relevant documentation, dependency relationships, and confidence where inference is uncertain.

AI systems must not need to scrape terminal output.

## 12. IDE and LSP

The compiler semantic engine should power diagnostics, completion, go-to-definition, find references, rename, hover information, signature help, refactoring, code actions, and symbol search.

The IDE and compiler must not maintain conflicting language semantics.

## 13. Debugging

The compiler must generate useful debug information when requested, including source locations, function names, variables, types, stack information, and async relationships where practical.

## 14. Testing and Fuzzing

Compiler development must include lexer, parser, type-system, semantic-analysis, diagnostic, IR, optimization, backend, end-to-end, cross-platform, and regression tests.

The lexer, parser, type checker, diagnostic engine, IR transformations, and serialization systems should be fuzz-tested.

Compiler crashes are high-priority defects.

## 15. Security

Source code, dependencies, and build inputs must be treated as potentially hostile.

The compiler should minimize arbitrary code execution during compilation, unsafe plugin execution, dependency trust assumptions, unbounded resource consumption, and malformed-input crashes.

## 16. Determinism

Given identical source, dependencies, compiler version, target, configuration, and relevant environment inputs, the compiler should produce reproducible results wherever practical.

## 17. Performance

Compiler performance must be measured across cold builds, incremental builds, large projects, dependency-heavy projects, generic-heavy projects, and IDE analysis.

Quantitative benchmarks must be established before implementation matures.

## 18. Open Architecture Decisions

The following must be finalized before architecture freeze:

- Lexer and parser technology
- Syntax tree architecture
- HIR design
- MIR design
- Type-checking algorithm
- Generic compilation strategy
- Memory-model integration
- Async lowering strategy
- Backend technology
- Object and linking strategy
- Incremental compilation strategy
- Compiler cache format
- Diagnostic wire format
- LSP architecture
- Plugin architecture
- Cross-compilation architecture
