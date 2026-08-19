# MINK — Tuple Types and Expressions Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 29 — Tuple types, expressions, and field access

## 1. Overview

Session 29 adds tuples to MINK: fixed-length, heterogeneous value sequences.
Tuples are a foundational type that enables multi-return values, anonymous
product types, and structured data without named fields. The implementation
covers the full compiler pipeline: lexer, parser, AST, type system, HIR,
MIR, ownership analysis, backend, and runtime layout.

## 2. Grammar Additions

```
Type        := ... | TupleType
TupleType   := '(' Type (',' Type)* ','? ')' | '()'
Primary     := ... | TupleExpr | UnitExpr
TupleExpr   := '(' Expr (',' Expr)* ','? ')'   (two or more elements)
UnitExpr    := '()'
Postfix     := ... | TupleFieldAccess
TupleFieldAccess := '.' IntLit
Pattern     := ... | TuplePattern
TuplePattern:= '(' Pattern (',' Pattern)* ','? ')' | '()'
```

See `docs/language/CORE_GRAMMAR.md` §22 for the full specification.

## 3. Type System

**New type kind:** `TypeKind::Tuple(Vec<TypeId>)` in the type table.

- Tuples are structurally compared and interned: `(Int, Bool)` is the
  same type everywhere.
- An empty tuple `()` resolves to `TypeKind::Unit`.
- A single-element tuple `(Int,)` is distinct from `Int`.
- Tuple types unify element-wise: same length and element types.

**Unification rules:**
- Two tuple types with the same element count unify element-wise.
- Different-length tuples never unify.
- Tuples never unify with non-tuple types.

**Display format:** `(Int, Bool)`, `()`, `(Int,)`.

## 4. Parser

- **Tuple types:** `parse_type` handles `LParen` by parsing
  comma-separated types. An empty `()` produces a unit tuple type.
  A trailing comma is allowed.
- **Tuple expressions:** `parse_primary` handles `LParen` by checking
  for `()` (unit), parsing the first expression, and then checking for
  `,` to determine tuple vs. group expression.
- **Tuple field access:** `parse_postfix` handles `.` followed by an
  integer token as a tuple field access (`ExprKind::TupleFieldAccess`).
  Chained access requires grouping: `(x.0).1` because `x.0.1` lexes
  `0.1` as a float literal.
- **Tuple patterns:** `parse_pattern_base` handles `LParen` to parse
  comma-separated patterns.

## 5. AST

New variants added to existing enums:

- `TyKind::Tuple(Vec<Ty>)` — a tuple type.
- `ExprKind::Tuple(Vec<Expr>)` — a tuple construction expression.
- `ExprKind::TupleFieldAccess { base, index }` — `base.index`.
- `Pattern::Tuple { elements, span }` — a tuple pattern.

## 6. HIR

New variants:

- `HirExprKind::Tuple(Vec<HirExpr>)` — lowered tuple expression.
- `HirExprKind::TupleFieldAccess { base, index, index_span }` — lowered
  field access.
- `HirPattern::Tuple { elements, span }` — lowered tuple pattern.

## 7. MIR

- **Tuple construction** lowers to `MirRvalueKind::TupleLit { elems }`.
- **Tuple field access** lowers to `MirRvalueKind::Member` with the
  index as the member name string, reusing the struct field access path.
  The backend resolves the byte offset from the tuple's layout.
- **Tuple patterns** in match arms lower through `lower_pattern_chain`
  by extracting each element field and matching recursively.

## 8. Backend

- **Classification:** `classify_tuple` returns `BType::Struct` (tuples
  reuse struct layout).
- **Layout:** `layout::tuple_layout` computes C-style packed layout:
  elements in declaration order, padded for alignment.
- **Slot allocation:** `aggregate_words` and `aggregate_value_size`
  handle `TypeKind::Tuple` by computing layout from element types.
- **Field access:** `resolve_member` detects tuple types by checking
  `TypeKind::Tuple`, parses the member name as an integer index, and
  returns the byte offset from `tuple_layout`.
- **Tuple literal construction:** `MirRvalueKind::TupleLit` emits
  `FieldStore` instructions for each element at its layout offset.

## 9. Ownership

Tuple expressions evaluate each element in transfer mode (owning
elements move); the tuple is owned if any element is owned. Tuple field
access observes without consuming. Tuple patterns bind each element
according to its sub-pattern's ownership rules.

## 10. Diagnostics

| Code   | Description |
| ------ | ----------- |
| E-T35  | Tuple index out of range |

## 11. Known Limitations

- **Chained tuple field access:** `x.0.1` does not work because the
  lexer merges `0.1` into a float literal. Use `(x.0).1` instead.
- **Nested tuples in native backend:** `(Int, (Int, Int))` is not yet
  supported by the native subset (the inner tuple type is not classified
  as a supported aggregate).
- **Tuple destructuring in let bindings:** `let (a, b) = x;` is not
  yet supported (a natural follow-up).
- **Tuple patterns in match:** Basic support exists but exhaustiveness
  checking for tuples is deferred.
- **Tuple equality:** `==`/`!=` on tuples is not supported.
- **Tuple payloads in sum types:** `enum E { V(Int, Bool) }` remains
  deferred (multiple payloads per variant).

## 12. Tests

**1,415 tests passing** (1,381 pre-existing + 34 new).

New test file: `tests/tuples.rs` with:

- 12 parse tests (tuple types, expressions, field access, chained access)
- 13 analysis tests (type checking, field access validation)
- 5 negative tests (out-of-range, non-tuple access, type mismatches)
- 4 native E2E execution tests (field access, arithmetic, function args, const binding)
