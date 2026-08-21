# Generics Implementation (Session 35)

## Overview

MINK now supports **parametric polymorphism** through AST-level monomorphization. Generic functions with type parameters are instantiated at call sites with concrete types inferred from arguments.

## Syntax

```
// Generic function with one type parameter
fn identity<T>(x: T) -> T {
    return x;
}

// Generic function with two type parameters
fn first<T>(a: T, b: T) -> T {
    return a;
}

// Calls with inferred type arguments
let a = identity(42);       // T = Int
let b = identity(true);     // T = Bool
let c = first(10, 20);      // T = Int
let d = first(true, false); // T = Bool
```

## Architecture

### Monomorphization Pass

The monomorphization pass runs after parsing and before semantic analysis:

1. **Phase 1 — Collection**: Scans all top-level function declarations for generic
   parameters. Each generic function is stored by name for later reference.

2. **Phase 2 — Walking**: Traverses all expressions looking for calls to generic
   functions. At each call site, type arguments are **inferred** from the
   argument expressions (e.g., integer literal → `Int`, boolean literal → `Bool`).
   A mangled name is computed (e.g., `identity__Int`) and the call is rewritten.

3. **Phase 3 — Cleanup**: The original generic function declarations are removed
   from the AST. Concrete instantiations (with substituted types) are appended.

### Span Reassignment

A critical design requirement: monomorphized functions are clones of the original
generic function. To prevent span-based resolution conflicts in the semantic
analyzer and type checker, **all identifiers** in each cloned function are
reassigned to unique synthetic spans. This ensures:
- `pre_register`'s `fn_info` map (keyed on `span.start()`) doesn't collide
- The semantic analyzer's resolution table doesn't have duplicate span entries
- Each monomorphized function is independently resolvable

### Type Substitution

Types are substituted recursively through:
- `TyKind::Named("T")` — generic parameters stored as named types by the parser
- `TyKind::GenericParam("T")` — explicit generic parameter syntax
- Nested types: `Ptr`, `Ref`, `Array`, `Tuple`, `NamedApp`

### Type Inference

Type arguments are inferred by matching function parameter types against call
argument expressions. Currently supported inference patterns:
- `Int` literal → `Int`
- `Float` literal → `Float`
- `Bool` literal → `Bool`
- `Char` literal → `Char`
- `Str` literal → `Str`
- `Null` literal → `Null`

Multi-type-parameter inference: each parameter is matched independently.
Unresolved type parameters cause the call to be treated as a non-generic call
(type error if the name doesn't exist).

### Name Mangling

Concrete function names are mangled as: `{base}__{Type1}_{Type2}_...`
with type parameters sorted alphabetically by their parameter name.

Examples:
- `identity` with `T = Int` → `identity__Int`
- `first` with `T = Int` → `first__Int`
- `swap` with `T = Int, U = Bool` → `swap__Bool__Int`

## Limitations (Genuine V1 Scope)

- **No explicit type arguments** at call sites (e.g., `identity::<Int>(42)` is
  not yet supported). All type arguments must be inferable from call arguments.
- **No generic structs or enums** yet — only generic functions.
- **Limited inference**: only literal expressions are inferred. Variable
  references, member accesses, and complex expressions as arguments are not
  yet supported for type inference. This means a call like
  `identity(some_variable)` where `some_variable: Int` will fail to infer `T`.
- **No recursive generics** or higher-kinded types.
- **No trait bounds** or constrained generics.

## File Changes

| File | Changes |
|------|---------|
| `src/ast/mod.rs` | Added `GenericParam`, `generic_params` fields on `FnItem`, `StructItem`, `EnumItem`, `TyKind::GenericParam` |
| `src/parser/mod.rs` | Added `parse_generic_params()`, generic type application parsing in `parse_type_ref` |
| `src/monomorphize/mod.rs` | **New file**: complete monomorphization pass |
| `src/lib.rs` | Added `pub mod monomorphize` |
| `src/driver.rs` | Integrated monomorphization into the compilation pipeline |
| `tests/generics.rs` | **New file**: 14 comprehensive tests |

## Test Coverage

- Parser: declaration parsing, multiple type parameters
- Type checking: single/multiple instantiations, return types, expressions,
  nested calls, type annotations
- HIR/MIR: full pipeline lowering with generics
- Regression: non-generic functions, unused generics
- Cross-module: generic functions imported via `use` (tested manually)
