# Session 25: Function Signature Type Annotations

**Status:** complete (session 25 changes).

## 1. Scope

The MINK grammar previously had bare-identifier function parameters and no
return-type syntax: `fn name(param1, param2) { body }`. Type annotations
on parameters and return types were explicitly listed as deferred to a
"type-system milestone" in `docs/language/CORE_GRAMMAR.md`.

This session completes the milestone by adding:

- **Parameter type annotations:** `fn f(x: Int, y: Float) { ... }` — each
  parameter may optionally carry a declared type (`: Type`). When present,
  the type checker enforces that the parameter's usage matches the declared
  type. When absent, the type is inferred from usage (existing behavior).
- **Return-type annotations:** `fn f() -> Int { ... }` — a function may
  declare a return type after its parameter list. When present, every
  `return expr;` in the body must produce a value of the annotated type.
  When absent, the return type is inferred from `return` expressions
  (existing behavior).
- **`Null` as a named type:** `Null` is now recognized in type annotations
  (`fn f() -> Null { ... }`), matching its status as a first-class scalar
  type (session 24).

All existing programs without annotations continue to work unchanged:
annotations are purely additive.

## 2. Grammar

The new productions (added to `docs/language/CORE_GRAMMAR.md` §17):

```
Fn          := 'fn' Ident '(' ParamList? ')' ('->' Type)? Block
Param       := Ident (':' Type)?
```

The existing `Type` production is reused (named types, `Ptr<T>`, `&T`/`&mut T`,
`[T; N]`).

## 3. AST (`src/ast/mod.rs`)

- `Param` gains `ty: Option<Ty>` — the optional type annotation.
- `FnItem` gains `return_ty: Option<Ty>` — the optional return-type
  annotation.
- Both fields are `None` for programs written before session 25, preserving
  backward compatibility.

## 4. Parser (`src/parser/mod.rs`)

- `parse_param()` now checks for `:` after the parameter identifier and
  parses the type when present. The parameter's span covers the identifier
  and any annotation.
- `parse_fn()` now checks for `->` after the parameter list and parses the
  return type when present.
- The `->` token (`TokenKind::Arrow`) already existed in the lexer; no
  lexical changes were required.
- Error recovery is unchanged: a malformed annotation produces a parse
  error and the parser continues.

## 5. Type Checker (`src/typecheck/checker.rs`)

- `Null` is added to `resolve_type()` as a recognized named type
  (`TypeKind::Null`), alongside `Int`, `Float`, `Bool`, `Char`, and `Str`.
- `pre_register()` is updated to build a map of function annotations
  (parameter types and return type) from the AST before the main loop. For
  each function symbol:
  - If a parameter has a declared type, the parameter's type slot is the
    resolved concrete type (not an inference variable).
  - If a parameter has no annotation, the type slot is an inference
    variable (existing behavior).
  - If the function has a declared return type, the result slot is the
    resolved concrete type.
  - If the function has no return annotation, the result slot is an
    inference variable (existing behavior).
- `check_fn()` is unchanged: it links parameter symbols to their type
  slots and sets `self.fn_result` for return checking. When the slots are
  concrete types (from annotations), unification enforces them; when they
  are inference variables, they are inferred from usage.
- No new error codes are needed: parameter and return-type mismatches use
  the existing `E-T01` (TypeMismatch) with the declared type as
  "expected" and the actual type as "found".

## 6. HIR, MIR, Backend

No changes required in HIR lowering, MIR lowering, or the native backend.
The type annotations are consumed during type analysis and expressed as
concrete `TypeId`s in the type table. Later stages already handle concrete
types; they never see annotations directly.

## 7. Tests

`tests/function_annotations.rs` (60 tests):

**Parser tests (9):**
- Return-type annotation accepted for all scalar types, `Ptr<T>`,
  arrays, references
- Parameter type annotations accepted for all type forms
- Mixed annotated/unannotated parameters
- Struct and enum type annotations parse correctly
- Annotation span covers the full annotation
- Rejection: missing arrow, bare arrow, double arrow, non-type token
  after arrow

**Type checker tests (18):**
- Annotated return type enforced (positive and mismatch for Int, Float,
  Char, Null, Str, Bool)
- Annotated parameter type enforced (positive and mismatch at call site
  and body use)
- Multiple parameter annotations enforced
- Mixed annotation enforcement (one annotated, one not)
- Backward compatibility: unannotated functions still infer
- Mixed annotations: partially inferred parameters and return types
- Recursive functions with annotations (positive and mismatch)
- Mutual recursion with annotations

**Native E2E tests (13):**
- Annotated Int, Bool, Float, Char return types compile and execute
- Recursive factorial with annotations
- Mutual recursion with annotations
- Struct parameter and return annotations
- Enum parameter and return annotations
- Void function
- Chained annotated function calls
- Loop with annotated function
- Byte-identical determinism

**Regression tests (3):**
- Unannotated program still works
- Struct program still works
- Enum match program still works

**Edge-case tests (4):**
- Empty params with return annotation
- Single param without return
- Deeply nested type annotation (Ptr<Int>)
- Reference type annotation with struct
- Function called from annotated function
- Empty body with return annotation

`tests/parser_hardening.rs`:
- `return_type_syntax_is_rejected` replaced by two new tests:
  `return_type_annotation_accepted` (positive) and
  `malformed_return_type_is_rejected` (negative)
- `missing_function_body_does_not_consume_the_next_item` updated to use
  `fn f() -> int fn g() {}` (missing body, not valid body)
- New `parse_ok` helper added

## 8. Test counts

1149 → **1209** (+60 in `tests/function_annotations.rs`; +1 net in
`tests/parser_hardening.rs` with the replaced/updated tests).

## 9. Known limitations

- **Let binding type annotations** (`let x: Type = expr;`) remain deferred
  to a future session. Only function parameter and return-type annotations
  are implemented.
- **Tuples, generics, optional/result types** remain future milestones.
- **`Null` as a named type** is now accepted in annotations, but `null`
  remains the only way to construct a `Null` value.
- Existing programs without annotations continue to work unchanged.

## 10. Files changed

| File | Change |
|---|---|
| `src/ast/mod.rs` | Added `ty: Option<Ty>` to `Param`, `return_ty: Option<Ty>` to `FnItem` |
| `src/parser/mod.rs` | Parse `: Type` after param names, `-> Type` after param list |
| `src/typecheck/checker.rs` | Use declared types in `pre_register()`, add `Null` to `resolve_type()` |
| `tests/function_annotations.rs` | New test file (60 tests) |
| `tests/parser_hardening.rs` | Updated/added tests for return-type syntax |
| `tests/typecheck.rs` | Updated `Param`/`FnItem` construction (added `ty: None`, `return_ty: None`) |
| `tests/semantics.rs` | Updated `Param`/`FnItem` construction |
| `tests/hir.rs` | Updated `FnItem` construction |
| `docs/language/CORE_GRAMMAR.md` | Added §17 (session-25 grammar additions), updated exclusions |
| `docs/implementation/FUNCTION_ANNOTATIONS_IMPLEMENTATION.md` | This document |
| `README.md` | Updated "What works today" and test count |
