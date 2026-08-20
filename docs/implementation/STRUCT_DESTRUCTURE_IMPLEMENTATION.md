# MINK — Struct Destructuring in Let Bindings Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 32 — struct destructuring in let bindings

## 1. Overview

Session 32 adds struct destructuring to let bindings: `let Point { x, y } = p;`
extracts each named field of a struct value into a separate binding. This is the
natural follow-up to Session 31's tuple destructuring, completing the core
destructuring story for aggregate types.

The implementation covers the full compiler pipeline: parser, AST, semantic
analysis, type checking, HIR, MIR, ownership analysis, backend, and native
execution.

## 2. Grammar Additions

```
LetBinding  := 'let' 'mut'? LetPattern (':' Type)? '=' Expr ';'
LetPattern  := Ident | TupleLetPattern | StructLetPattern
StructLetPattern := Ident '{' StructPatternField (',' StructPatternField)* ','? '}'
StructPatternField := Ident (':' Pattern)?
```

See `docs/language/CORE_GRAMMAR.md` §25 for the full specification.

## 3. Parser

- **Detection:** `parse_let` checks if the token after `let [mut]` is `Ident`
  followed by `{`. If so, it dispatches to `parse_let_struct_destructure`;
  otherwise it falls through to tuple destructuring or simple bindings.
- **Field parsing:** `parse_struct_pattern_field` parses each field as
  `name` (shorthand) or `name: pattern` (explicit binding).
- **Backward compatibility:** The `name` field of `LetItem` is set to the
  first binding's identifier for backward compatibility.

## 4. AST

- `Pattern` gains a `Struct` variant with `name: Ident`,
  `fields: Vec<StructPatternField>`, and `span: Span`.
- `StructPatternField` holds `name: Ident`, `binding: Option<Pattern>`,
  and `span: Span`.

## 5. Type System

**New error codes:**
- `E-T39` (`UnknownStructFieldInPattern`) — the pattern names a field the
  struct does not declare.
- `E-T40` (`MissingStructFieldInPattern`) — the pattern omits a declared
  field.
- `E-T41` (`StructPatternTypeMismatch`) — the pattern's struct name does
  not match the initializer's type.

**Type checking rules:**
- The initializer must be a struct type (`E-T37` otherwise).
- The struct type name in the pattern must match the initializer's struct
  type (`E-T41` on mismatch).
- Every field in the pattern must exist on the struct (`E-T39`).
- Every declared field must appear in the pattern (`E-T40`).
- Each field binding is unified with the corresponding struct field's type.
- Optional type annotations (`let Point { x, y }: Point = expr;`) check
  the whole struct type against the annotation.

## 6. HIR

- `HirPattern` gains a `Struct` variant with `name: HirName`,
  `fields: Vec<HirStructPatternField>`, and `span: Span`.
- `HirStructPatternField` holds `name: HirName` and `binding: HirIdent`.
- The struct type name is stored as `HirName` (not `HirIdent`) because it
  is a type reference, not a value binding.

## 7. MIR

Struct destructuring lowers to field extraction via `MirRvalueKind::Member`:

```
init = <evaluator>
x = Member { base: init, member: "x" }   // field "x"
y = Member { base: init, member: "y" }   // field "y"
```

Each field is extracted by name (reusing the existing struct member access
path) and stored in a separate local.

## 8. Backend

No changes: the backend already handles `FieldLoad` instructions for
struct member access. The destructured fields lower to the same
instructions as explicit `p.x` / `p.y` field access.

## 9. Ownership

Each destructured binding receives a copy of its field (matching the
existing struct field access semantics: "observes without consuming").
The initializer is not consumed by destructuring.

## 10. Diagnostics

| Code   | Description |
| ------ | ----------- |
| E-T37  | Cannot destructure non-tuple/non-struct type |
| E-T38  | Tuple destructuring arity mismatch |
| E-T39  | Unknown field in struct destructuring pattern |
| E-T40  | Missing field in struct destructuring pattern |
| E-T41  | Struct pattern type mismatch |

## 11. Known Limitations

- **Match-arm struct patterns:** `match p { Point { x, y } => { .. } }` is
  not yet supported. Struct patterns in let-destructuring only.
- **Const struct destructuring:** `const Point { x, y } = p;` remains
  deferred.
- **Nested struct destructuring in native backend:** deeply nested struct
  patterns work at the type-checking level but the backend handles them
  through repeated member access lowering.
- **Or-patterns and guards in struct let:** remain deferred.

## 12. Tests

**1,522 tests passing** (1,494 pre-existing + 28 new).

New test file: `tests/struct_destructure.rs` with:
- 6 parse tests (basic, single field, explicit binding, mutable, trailing
  comma, type annotation, empty braces)
- 5 analysis tests (basic, explicit binding, mutable, expression usage,
  function return)
- 5 type-check tests (non-struct, unknown field, missing field, wrong
  struct type, field type consistency)
- 12 native E2E execution tests (basic, single field, explicit binding,
  mutable, function return, loop, computation, type annotation,
  if-expression, multiple structs, determinism)
