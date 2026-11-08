# Struct patterns in match arms

**Status:** Implemented (Session 239)
**Version:** 0.1.5
**Feature:** structural matching over struct values in `match` statements and
`match` expressions.

## 1. Overview

Before Session 239, struct patterns existed only in `let` destructuring:
`let Point { x, y } = p;`. A `match` arm could bind or test scalars and enum
variants, but `match p { Point { x, y } => ... }` failed with `E-P24` because
the parser treated `Point` as a binding and then found `{` instead of `=>`.
The type checker also rejected every struct scrutinee with `E-T26`, and MIR
lowering had no struct-pattern branch.

Session 239 closes that gap end-to-end. The same `Pattern::Struct` AST node
now parses in a match arm, is checked against a struct scrutinee, lowers to
field extraction plus the ordinary pattern-test chain, and executes natively.
The feature works in both match forms and composes with guards, or-patterns,
literal field patterns, nested structs, tuples, and enum payload patterns.

## 2. Grammar

```
StructPattern      = Ident "{" [ StructPatternField { "," StructPatternField } [ "," ] ] "}" ;
StructPatternField = Ident [ ":" Pattern ] ;
MatchPattern       = ... | StructPattern ;
```

The grammar is additive. `let` struct destructuring keeps its existing
dedicated parser path; match arms use the general pattern parser.

## 3. Parser

`parse_pattern_base` recognizes `Ident '{'` after consuming the identifier and
parses a `Pattern::Struct` with the existing `parse_struct_pattern_field`
helper. This reuses field parsing for shorthand (`x`) and explicit
(`x: pattern`) forms, trailing commas, nested patterns, and empty braces.
Malformed patterns report the existing `E-P24`/`E-P07`/`E-P26` family and
recover at the next arm boundary.

## 4. Type checking

The match scrutinee whitelist now includes `TypeKind::Struct`. A new
`check_struct_match_pattern` validates a struct pattern against the scrutinee:

- a concrete scrutinee must be the same nominal struct (`E-T41`);
- an unresolved scrutinee is pinned by the pattern's struct name;
- unknown fields are `E-T39`;
- every declared field must be present (`E-T40`);
- shorthand fields bind to the field type; explicit field patterns recurse
  through the ordinary `check_arm_pattern` path, so literal, enum, tuple,
  nested struct, or-pattern, and binding checks all apply.

Coverage has a new `partial` bit. A struct pattern is `all` (irrefutable and
therefore exhaustive) only when every field pattern is irrefutable. A
refutable field pattern produces `partial` coverage: the arm is live, but it
does not make the match exhaustive and it is not mistaken for an empty
pattern during unreachable-arm checking. This required a small fix to
`coverage_contains`; without it, the second refutable struct arm was falsely
reported as `E-T25`.

Field patterns inside data-carrying enum payloads use the same helper through
`check_payload_pattern`, so `E::V(Point { x, y })` gets the same diagnostics
and coverage rules.

## 5. HIR

`HirStructPatternField` retains `binding: HirIdent` for the `let`
destructuring path and gains `pattern: Box<HirPattern>` for the full field
pattern. The box breaks the otherwise recursive
`HirPattern -> fields -> HirPattern` type.

`lower_pattern` lowers explicit field patterns recursively; shorthand fields
become `HirPattern::Binding` of the field name. `let` lowering continues to
read `binding`, while match lowering reads `pattern`.

## 6. MIR

`lower_pattern_chain` gains a `HirPattern::Struct` branch. It extracts each
field with a `MirRvalueKind::Member` temporary (through the new
`emit_member_load` helper), then recursively lowers that field's pattern.
Fields are tested left-to-right, with each failure going to the arm's else
block. An empty struct pattern is irrefutable and jumps directly to the arm
body.

No backend instruction was added: struct field access already existed for
`let` destructuring and explicit `.field` expressions. The new MIR reuses the
same member-load path.

## 7. Ownership

The ownership walk registers struct-pattern bindings as copies, matching the
existing `let` and tuple-pattern rules; the scrutinee is observed and remains
live. Nested struct, tuple, and or-pattern field bindings are walked so their
scope release is correct. Refutable field patterns do not bind when they fail;
they introduce no ownership state.

## 8. Diagnostics

| Code | Meaning for a struct match pattern |
| --- | --- |
| `E-T01` | A field pattern's literal or usage has the wrong type; a guard is not `Bool`. |
| `E-T24` | Refutable struct fields need a catch-all arm. |
| `E-T25` | An arm follows an irrefutable struct pattern that already covers every value. |
| `E-T34` | Or-pattern alternatives bind different field names. |
| `E-T39` | The pattern names a field the struct does not declare. |
| `E-T40` | The pattern omits a declared field. |
| `E-T41` | The pattern's struct name or the scrutinee type is not the struct type. |

## 9. Exhaustiveness rules

- A struct pattern with only binding, wildcard, and recursively irrefutable
  struct fields covers the whole scrutinee (`all`).
- A field pattern that is refutable (literal, range, enum variant, tuple, or a
  struct containing one) makes the whole arm `partial`. The compiler does not
  yet construct a product coverage for those fields, so the match needs a `_`
  or binding fallback even if the refutable arms happen to cover every case.
- A malformed struct pattern is marked partial while its own diagnostic is
  reported, preventing a false unreachable-arm cascade.
- Guards commit no coverage, as before.

## 10. Examples

Two runnable examples are included:

- `examples/struct_match_geometry/main.mink` — `Shape::Rect(Rect { width, height })`
  and `Shape::Circle(Circle { radius })` compute areas directly in match arms.
  Output: `areas: 20 27 0`, exit 0.
- `examples/struct_match_dispatch/main.mink` — literal integer/boolean field
  patterns select command variants, with a final catch-all. Output:
  `dispatch: 103 14 5 7006`, exit 0.

## 11. Tests

`tests/struct_match.rs` contains 23 tests:

- 5 parser tests: statement, expression, nested fields/trailing comma, empty
  pattern, unclosed pattern rejection;
- 12 type-checking/diagnostic tests: irrefutable exhaustiveness, refutable
  catch-all requirement, unknown/missing/wrong fields, non-struct scrutinee,
  field type mismatch, non-Bool guard, or-pattern binding mismatch, nested
  fields, and enum-payload nesting;
- 7 native execution tests: statement, expression, literal field, nested
  fields plus guard, or-pattern, wildcard/rename, and enum payload.

All 23 pass natively through the CLI. The pre-existing `struct_destructure`,
`pattern_matching`, `match_expressions`, `sum_types`, `hir`, `mir`, and
ownership suites remain green in the focused regression run.

## 12. Known limitations

- No `..` rest syntax; every declared field must be named, matching `let`
  destructuring.
- No product coverage for refutable fields, so a set of refutable struct arms
  still needs a catch-all even when logically exhaustive.
- Field bindings copy; struct patterns do not move fields out of the
  scrutinee.
- Nested struct patterns in `let` destructuring are type-checked but were not
  part of this feature; match-arm nesting is fully lowered and tested.
- No array patterns, slice patterns, or `name @ pattern` bindings.
