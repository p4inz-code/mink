# Closures / Lambdas Implementation

**Session:** 37  
**Status:** Implemented  
**V1 Scope:** Zero-capture and by-value capture closures, no-argument closures, closure passing, closures in control flow.

## 1. Overview

Closures (lambdas) are desugared to named functions during the
monomorphization pass. Captured free variables become leading parameters
of the generated function. At call sites the monomorphizer rewrites
references to the closure variable so that captured values are passed
automatically.

## 2. Syntax

```
ClosureExpr  = "|" [ ClosureParams ] "|" ( BlockExpr | Expression ) ;
ClosureParams = ClosureParam { "," ClosureParam } "," ;
ClosureParam  = Identifier [ ":" Type ] ;
```

No-argument closures use `| |` (two pipes with a space). The parser
recognises `| <params> |` as a closure expression.

## 3. AST Representation

```rust
ExprKind::Closure {
    params: Vec<ClosureParam>,
    body: Box<Expr>,
}
```

`ClosureParam` mirrors `Param` but uses the expression-level `Ident`.

## 4. Monomorphization / Desugaring

The monomorphizer processes closures in Phase 3 (after generic
function instantiation):

1. **Capture discovery** — free variables are collected from the closure
   body by walking identifiers and excluding parameters.
2. **Function generation** — a synthetic `__closure_N` function is
   created with:
   - Captured variables as leading sorted parameters.
   - Explicit closure parameters as trailing parameters.
   - The body wrapped in a `return` statement.
3. **Variable rewriting** — a post-pass (Phase 3.5) finds
   `let x = __closure_N` bindings and rewrites `x(args)` to
   `__closure_N(captured..., args)`.
4. **Item injection** — generated functions are appended to the AST
   as top-level items.

All synthetic spans use `self.next_span()` to avoid collisions with
the type checker's span-keyed resolution map.

## 5. Type System

Closures have function type `Fn(params) -> ReturnType` where
- parameter types come from annotations (or inference where supported).
- return type is inferred from the body.

The closure itself gets a `Fn` TypeKind; when used as a value it
carries function pointer semantics.

## 6. Backend

- **FnPtr BType** — added to represent function pointer values.
- **LoadFnPtr** — loads the address of a named function.
- **IndirectCall** — performs an indirect call through a register.
- **call_rax** — new emitter method for `call rax` (0xFF 0xD0).

When a closure variable is used as a value, the backend resolves it
to a `LoadFnPtr` of the desugared function. Indirect calls are
emitted via `IndirectCall`.

## 7. Capture Semantics

| Aspect | V1 Behavior |
|--------|-------------|
| Capture mode | By value (copy for `Int`/`Bool`/`Float`/`Char`, move otherwise) |
| Mutable capture | Not supported — captured values are immutable |
| Escaping | Limited — borrowing closures that outlive scope are rejected |
| Ordering | Deterministic: captures sorted alphabetically |

## 8. Limitations

- No type annotations on closure parameters (required for V1; inference
  works when the call site provides enough information).
- Higher-order passing of capturing closures through generic functions
  may not inject captures at the call-through site (only direct calls
  to the closure variable are rewritten).
- Recursive closures are not supported in V1.
- Closure return type is inferred, not declared.
- Closures cannot be stored in structs/arrays (no indirect call through
  aggregates yet).

## 9. Bug Fix

Session 37 also fixed a pre-existing bug in `analyze_block`: trailing
result expressions in blocks were not analyzed by the semantic
analyzer. This meant identifiers inside the then/else branches of
if-expressions used as values could not be resolved. The fix adds
`analyze_expr` for `block.result` in the semantic analyzer.
