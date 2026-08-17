# Session 26: Let-Binding Type Annotations

**Status:** complete (session 26 changes).

## 1. Scope

The MINK grammar previously had bare-identifier let and const bindings with no
type annotations: `let name = expr;` and `const name = expr;`. Type annotations
on bindings were explicitly listed as deferred to a "type-system milestone" in
`docs/language/CORE_GRAMMAR.md` §2.

This session completes the milestone by adding:

- **Let binding type annotations:** `let x: Int = 1;` — a `let` binding may
  optionally carry a declared type (`: Type`). When present, the type checker
  enforces that the initializer expression's type matches the declared type.
  When absent, the type is inferred from the initializer (existing behavior).
- **Const binding type annotations:** `const X: Int = 42;` — mirroring let
  bindings, a `const` may also carry an optional type annotation.

All existing programs without annotations continue to work unchanged:
annotations are purely additive.

## 2. Grammar

The new productions (added to `docs/language/CORE_GRAMMAR.md` §19):

```
LetBinding  := 'let' 'mut'? Ident (':' Type)? '=' Expr ';'
ConstBinding:= 'const' Ident (':' Type)? '=' Expr ';'
```

The existing `Type` production is reused (named types, `Ptr<T>`, `&T`/`&mut T`,
`[T; N]`).

## 3. AST (`src/ast/mod.rs`)

- `LetItem` gains `ty: Option<Ty>` — the optional type annotation.
- `ConstItem` gains `ty: Option<Ty>` — the optional type annotation.
- Both fields are `None` for programs written before session 26, preserving
  backward compatibility.

## 4. Parser (`src/parser/mod.rs`)

- `parse_binding_tail()` now checks for `:` after the binding identifier and
  parses the type when present. The binding's span covers the identifier,
  any annotation, and the `= expr;`.
- `parse_const()` now passes the `ty` field through from the shared
  `parse_binding_tail()`.
- The `:` token (`TokenKind::Colon`) already existed in the lexer; no
  lexical changes were required.
- Error recovery is unchanged: a malformed annotation produces a parse
  error and the parser continues.

## 5. Type Checker (`src/typecheck/checker.rs`)

- `check_item()` is updated: when a `Let` binding has a type annotation,
  the annotation type is resolved via `resolve_type()` and unified with the
  initializer's type. A mismatch produces `E-T01` with the annotated type
  as "expected" and the actual type as "found". The name span is included
  as a related location.
- `check_stmt()` applies the same logic for `Let` bindings inside function
  bodies.
- `check_item()` and `check_stmt()` also apply annotation enforcement to
  `Const` bindings with type annotations.
- The deferred member re-type paths (`resolve_deferred_members()`,
  `resolve_deferred_stmt()`) also check let/const binding annotations
  when their initializer types are recomputed.
- No new error codes are needed: type mismatches use the existing `E-T01`
  (TypeMismatch).

## 6. HIR, MIR, Backend

No changes required in HIR lowering, MIR lowering, or the native backend.
The type annotations are consumed during type analysis and expressed as
concrete `TypeId`s in the type table. Later stages already handle concrete
types; they never see annotations directly.

## 7. Tests

`tests/let_annotations.rs` (82 tests):

**Parser tests (18):**
- Let binding type annotations accepted for all scalar types (`Int`, `Float`,
  `Bool`, `Char`, `Str`, `Null`)
- `let mut` with type annotation
- Struct, enum, pointer, array, reference, and mutable-reference type annotations
- No-annotation bindings still parse
- Const binding with and without annotation
- Mixed annotated and unannotated bindings
- Annotation span covers the full annotation

**Type checker positive tests (16):**
- Annotation matches for all scalar types, struct, enum, array, pointer,
  reference, const
- Mixed annotated/unannotated bindings
- Annotated let used in expressions and function calls

**Type checker negative tests (10):**
- Annotation mismatch for Int, Float, Bool, Char, Str, struct, enum, const
- Unknown type name produces error
- Annotation mismatch with function return value

**Backward compatibility tests (4):**
- Unannotated let, mutable let, const
- Complex unannotated program with struct

**Recursive/mutual recursion tests (2):**
- Recursive function with let annotation
- Mutual recursion with let annotation

**Native E2E tests (14):**
- Annotated Int, Bool, Float, Char let bindings compile and execute
- `let mut` with annotation
- Annotated let in a loop
- Struct, enum, array let annotations
- Chained function calls with annotated let
- Const annotation
- Multiple annotated bindings
- Loop counter with annotation

**Determinism test (1):**
- Byte-identical determinism

**Regression tests (5):**
- Unannotated program, struct, enum match, function annotations, borrowing

**Edge-case tests (12):**
- Annotation with function return, complex/grouped/negated expressions
- Comparison and logical operators
- If-condition variable annotation
- Ternary-like pattern
- Const used in function
- Forward declaration
- Multiple same-type bindings
- String literal annotation
- Empty body with annotation

## 8. Test counts

1209 → **1291** (+82 in `tests/let_annotations.rs`; +1 net in
`tests/parser_hardening.rs` with the updated tests).

## 9. Known limitations

- **Tuples, generics, optional/result types** remain future milestones.
- **`let` binding annotations are enforced by the type checker only;** there
  is no compile-time layout check for annotated bindings (layout validation
  is at the struct/enum declaration level, not the binding level).
- Existing programs without annotations continue to work unchanged.

## 10. Files changed

| File | Change |
|---|---|
| `src/ast/mod.rs` | Added `ty: Option<Ty>` to `LetItem` and `ConstItem` |
| `src/parser/mod.rs` | Parse `: Type` after binding names in `parse_binding_tail()` |
| `src/typecheck/checker.rs` | Enforce type annotations on let/const bindings in `check_item()`, `check_stmt()`, `resolve_deferred_members()`, and `resolve_deferred_stmt()` |
| `tests/let_annotations.rs` | New test file (82 tests) |
| `tests/parser_hardening.rs` | Updated `excluded_constructs_inside_functions_are_rejected` (removed let-annotation case) and `combined_excluded_program_reports_every_offender` (updated expectations) |
| `tests/semantics.rs` | Updated `LetItem` construction (added `ty: None`) |
| `docs/language/CORE_GRAMMAR.md` | Added §19 (session-26 grammar additions) |
| `docs/implementation/LET_ANNOTATIONS_IMPLEMENTATION.md` | This document |
| `README.md` | Updated "What works today" and test count |
