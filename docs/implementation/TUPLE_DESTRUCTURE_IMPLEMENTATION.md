# MINK — Tuple Destructuring in Let Bindings Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 31 — tuple destructuring in let bindings

## 1. Overview

Session 31 adds tuple destructuring to let bindings: `let (a, b) = expr;`
extracts each element of a tuple value into a separate binding. This is the
natural follow-up to Session 29's tuple types and expressions, completing
the core tuple usability story.

The implementation covers the full compiler pipeline: parser, AST,
semantic analysis, type checking, HIR, MIR, ownership analysis, backend,
and native execution.

## 2. Grammar Additions

```
LetBinding  := 'let' 'mut'? LetPattern (':' Type)? '=' Expr ';'
LetPattern  := Ident | TupleLetPattern
TupleLetPattern := '(' LetPattern (',' LetPattern)* ','? ')' | '()'
```

See `docs/language/CORE_GRAMMAR.md` §24 for the full specification.

## 3. Parser

- **Detection:** `parse_let` checks if the token after `let [mut]` is `(`.
  If so, it dispatches to `parse_let_destructure`; otherwise it uses the
  existing `parse_binding_tail` for simple `name = expr;` bindings.
- **Pattern parsing:** `parse_let_destructure` reuses the existing
  `parse_pattern` infrastructure, which already handles `LParen` for tuple
  patterns in match arms (session 29).
- **Backward compatibility:** `const` bindings are unchanged; const
  destructuring is deferred.

## 4. AST

- `LetItem` gains a `pattern: Option<Pattern>` field. When `Some`, the
  binding is a destructuring pattern. The `name` field remains for backward
  compatibility (set to the first binding's identifier).

## 5. Type System

**New error codes:**
- `E-T37` (`CannotDestructure`) — a tuple destructuring pattern is applied
  to a non-tuple type.
- `E-T38` (`DestructureArityMismatch`) — the destructuring pattern has a
  different number of elements than the tuple type.

**Type checking rules:**
- The initializer type must be a tuple (`E-T37` otherwise).
- The pattern's element count must match the tuple's element count
  (`E-T38` on mismatch).
- Each element pattern's binding is unified with the corresponding tuple
  element type.
- Optional type annotations (`let (a, b): (Int, Bool) = expr;`) check the
  whole tuple type against the annotation.
- Nested tuple patterns are recursively checked.

## 6. HIR

- `HirLet` gains a `pattern: Option<HirPattern>` field. When `Some`, the
  pattern is lowered from the AST `Pattern::Tuple`.
- The binding's type is set to the initializer's type (the tuple type),
  not the first element's type.

## 7. MIR

Tuple destructuring lowers to element extraction via `MirRvalueKind::Member`:

```
init = <evaluator>
a = Member { base: init, member: "0" }   // element 0
b = Member { base: init, member: "1" }   // element 1
```

Each element is extracted via field access (reusing the existing tuple
field access path) and stored in a separate local.

## 8. Backend

No changes: the backend already handles `FieldLoad` instructions for
tuple member access. The destructured elements lower to the same
instructions as explicit `x.0` / `x.1` field access.

## 9. Ownership

Each destructured binding receives a copy of its element (matching the
existing tuple field access semantics: "observes without consuming").
The initializer is not consumed by destructuring.

## 10. Diagnostics

| Code   | Description |
| ------ | ----------- |
| E-T37  | Cannot destructure non-tuple type |
| E-T38  | Tuple destructuring arity mismatch |

## 11. Known Limitations

- **Nested tuples in native backend:** `(Int, (Int, Int))` destructuring
  works at the type-checking level but the inner tuple is not classified
  as a supported aggregate by the native backend.
- **Const destructuring:** `const (a, b) = expr;` remains deferred.
- **Struct destructuring:** `let P { x, y } = p;` is a future feature.
- **Or-patterns and guards in let:** `let (a | b, c) = expr;` and
  `let (a, b) if condition = expr;` remain deferred.

## 12. Tests

**1,494 tests passing** (1,454 pre-existing + 40 new).

New test file: `tests/tuple_destructure.rs` with:
- 8 parse tests (basic destructuring, nested, wildcards, annotations)
- 14 analysis tests (type checking, annotations, negative cases)
- 18 native E2E execution tests (basic, nested, functions, loops,
  mutation, if-expressions, break values, const bindings, determinism)
