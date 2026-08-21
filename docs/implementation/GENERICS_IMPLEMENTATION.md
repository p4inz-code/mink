# Generics Implementation (Sessions 35–36)

## Overview

MINK supports **parametric polymorphism** through AST-level monomorphization.
Generic functions, structs, and enums with type parameters are instantiated
with concrete types inferred from usage sites or specified via explicit type
arguments.

## Syntax

```
// Generic function with one type parameter
fn identity<T>(x: T) -> T {
    return x;
}

// Generic function with two type parameters
fn swap<T, U>(a: T, b: U) -> T {
    return a;
}

// Generic struct
struct Pair<T> {
    first: T,
    second: T,
}

// Generic enum
enum Maybe<T> {
    Some(T),
    Nothing,
}

// Calls with inferred type arguments
let a = identity(42);           // T = Int
let b = identity(true);         // T = Bool
let p = Pair { first: 10, second: 20 };  // T = Int
let s = Maybe::Some(42);       // T = Int

// Calls with explicit type arguments
let x = identity::<Int>(42);
let p2 = make_pair::<Int>(10, 20);
```

## Architecture

### Monomorphization Pass

The monomorphization pass runs after parsing and before semantic analysis:

1. **Phase 1 — Collection**: Scans all top-level declarations for generic
   parameters. Stores generic functions, structs, and enums by name.

2. **Phase 2 — NamedApp Resolution**: Walks all type annotations in the AST,
   resolving `NamedApp` types (e.g., `Pair<Int>`) to concrete `Named` types
   (e.g., `Pair__Int`). Creates concrete struct/enum declarations as needed.

3. **Phase 3 — Expression Walking**: Traverses all expressions looking for calls
   to generic functions, struct literals, and enum variants. Infers or uses
   explicit type arguments, rewrites names to mangled concrete names, and
   creates concrete struct/enum declarations.

4. **Phase 4 — Name Resolution in Bodies**: After type substitution in
   monomorphized function bodies, resolves struct literal and enum variant names
   using the function's substitution context. This handles cases like
   `Pair { first: a, second: b }` inside a generic function body where
   `a` and `b` are variable identifiers (not literals).

5. **Phase 5 — Cleanup**: Removes original generic declarations, appends
   concrete instantiations.

### Explicit Type Arguments

The parser supports explicit type arguments via `::<Type>` syntax:
- Parsed in the primary expression handler when `Ident::<` is detected
- The callee expression span is set to just the identifier span (not the full call span)
- The monomorphizer uses explicit type args directly instead of inferring from arguments

### Span Reassignment

Monomorphized functions are clones of originals. All identifiers are reassigned
to unique synthetic spans to prevent resolution conflicts in the semantic
analyzer and type checker.

### Type Substitution

Types are substituted recursively through `Named("T")`, `GenericParam("T")`,
and nested types (`Ptr`, `Ref`, `Array`, `Tuple`, `NamedApp`).

### Struct/Enum Inference

When a struct literal or enum variant is encountered inside a monomorphized
function body, the monomorphizer uses the function's own substitution context
to determine the concrete name. This avoids the need to infer types from
variable-identifier arguments.

### Name Mangling

Concrete names: `{base}__{Type1}_{Type2}_...` (types sorted alphabetically).
- `identity` with `T = Int` → `identity__Int`
- `Pair` with `T = Int` → `Pair__Int`
- `Maybe` with `T = Int` → `Maybe__Int`

## File Changes

| File | Changes |
|------|---------|
| `src/ast/mod.rs` | `GenericParam`, `generic_params` on `FnItem`/`StructItem`/`EnumItem`, `TyKind::GenericParam`, `type_args` on `Call` |
| `src/parser/mod.rs` | `parse_generic_params()`, generic type application, `::<Type>` explicit type args, inline enum variant parsing |
| `src/monomorphize/mod.rs` | Complete monomorphization pass (functions, structs, enums, explicit type args, body name resolution) |
| `src/lib.rs` | `pub mod monomorphize` |
| `src/driver.rs` | Monomorphization integrated into compilation pipeline |
| `tests/generics.rs` | 29 comprehensive tests |

## Limitations (Genuine V1 Scope)

- **Limited type inference**: only literal expressions are inferred. Variable
  references as arguments require explicit type args.
- **No recursive generics** or higher-kinded types.
- **No trait bounds** or constrained generics.
