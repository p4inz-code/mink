# MINK Sum Types Implementation

**Status:** Implemented (Session 19)
**Version:** 0.1.0

This document describes the MINK **sum-type** milestone: data-carrying enum
variants (`enum Option { Some(Int), None }`), payload construction
(`Option::Some(5)`), payload patterns (`Option::Some(x)` extracting `x`),
the tagged-union byte layout, payload-aware exhaustiveness, ownership of
owned payloads, and native x86-64 execution. It builds directly on the enum
foundation of Session 17 (`docs/implementation/ENUM_TYPES_IMPLEMENTATION.md`),
the pattern-matching foundation of Session 18
(`docs/implementation/PATTERN_MATCHING_IMPLEMENTATION.md`), and the
aggregate/ownership foundations of Sessions 14–16, and deliberately does
**not** introduce explicit discriminants (`A = 5`), enum equality for
tagged unions, or payload types without a deterministic value layout.

## 1. Frozen Session 19 rules

### 1.1 Data-carrying variant declarations

- A variant may declare exactly **one payload type**:
  `enum Shape { Circle(Int), Nothing }`. Unit variants (`Nothing`) and
  data-carrying variants (`Circle(Int)`) mix freely within one enum.
- A payload type must be a **value type with a deterministic layout**:
  `Int`, `Bool`, `Str`, a struct, another enum, or (recursively) a
  combination of these. Pointers, references, arrays, and function types
  are rejected as payloads (`E-T27` invalid variant payload).
- A payload type may name a struct or enum declared anywhere at module
  scope (declaration order is irrelevant, exactly like struct fields).
- `Variant()` with an empty payload list is a parse error (`E-P25`);
  `Variant(A, B)` with more than one payload is a parse error (the `,` is
  reported as `E-P14` expected `)`). A data-carrying variant carries
  exactly one payload in declarations, constructions, and patterns alike.

### 1.2 Construction

- `E::V(expr)` constructs the enum value: the discriminant word is `V`'s
  position in declaration order (unchanged from Session 17) and the
  payload area holds `expr`'s value. The expression's type is the nominal
  enum type `E`.
- The payload expression must unify with the variant's declared payload
  type: a mismatch is `E-T28` (variant payload mismatch, with rendered
  expected/actual types).
- Arity is checked both ways: constructing a unit variant with a payload
  (`E::A(5)`) and constructing a data-carrying variant without one
  (`E::B`) are both `E-T29` (variant payload arity). Unit construction
  (`E::A`) is unchanged from Session 17.

### 1.3 Tagged-union layout

- An enum **with any data-carrying variant** is a **tagged union**: the
  discriminant word at offset 0, then a payload area sized for the
  **largest** variant payload, aligned to the largest payload alignment.
  The size is rounded up to the enum's alignment, which is the maximum of
  the word and the payload alignments. `enum E { A, B(Int) }` is 16 bytes
  (tag word + 8-byte payload).
- A **unit-only** enum keeps the Session-17 layout: the value *is* the
  discriminant word (`(8, 8)`, scalar). `scalar_size_align` reports an
  enum as scalar only when every variant is a unit variant.
- Layout is deterministic: discriminants and payloads follow declaration
  order and the C-style alignment rules of Session 14. Recursion is
  detected through a **kind-tagged by-value path** (struct and enum ids
  live in separate namespaces, so `struct P` and `enum P` never collide):
  an enum whose payload (transitively) contains itself by value is
  rejected (`E-T18` invalid aggregate layout), as is a mutually recursive
  pair of enums. Payload layouts are bounded by `MAX_AGGREGATE_BYTES`.

### 1.4 Payload patterns

- A variant pattern may carry exactly one payload pattern:
  `E::V(x)` (binding), `E::V(_)` (wildcard), `E::V(5)` (literal),
  `E::V(E2::X)` (nested variant), recursively nested as needed.
- A payload pattern whose type does not unify with the payload type is
  `E-T01` (type mismatch), reported on the inner pattern.
- A binding payload pattern binds the extracted payload value in the
  arm's scope (copy semantics for copy payloads; a move out of the
  scrutinee for owned payloads — see §1.6).
- Exhaustiveness is **payload-aware and recursive** (§1.5), and `E::V()`
  with an empty payload list is `E-P25`.

### 1.5 Exhaustiveness and unreachable arms

- Match coverage tracks every variant, and a variant's coverage is
  **complete only when its payload is covered**: the coverage of a
  data-carrying variant holds an optional sub-coverage for the payload's
  type, which is itself a matchable-coverage structure.
- `E::B(_)` (a catch-all payload) completes the variant; `E::B(E2::X)`
  alone leaves `E2::Y` uncovered, so a missing `E::B(E2::Y)` arm is
  `E-T24` (non-exhaustive match) naming the missing sub-variant.
- An arm that can never run — an exhaustive prefix already covered the
  same value, a repeated literal payload (`E::B(1)` twice), or a repeated
  catch-all payload (`E::B(_)` twice) — is `E-T25` (unreachable arm).
  Distinct literal payloads (`E::B(1)` then `E::B(2)`) are *not*
  unreachable: each covers a different payload value.
- Unit-only enums keep the Session-18 exhaustiveness rules unchanged.

### 1.6 Ownership of payloads

- An enum whose payload may own a heap value (`Str`, or a struct/enum
  containing one) is a **tracked value**: construction transfers the
  payload into the value (`E::V(s)` moves an owned `s`; using `s`
  afterwards is `E-S10` use of a moved value), and the enum moves as a
  whole on transfer. A copy payload (`Int`, `Bool`) leaves the enum
  freely copyable, and a string-literal payload is an immutable constant
  that copies (exactly like string fields in structs).
- A payload pattern that binds an owned payload (`E::V(x)` where the
  payload may own) **moves the payload out of the scrutinee**: the
  scrutinee's payload is consumed on every arm that binds it, and using
  the scrutinee after the match is `E-S10`. Binding a copy payload does
  not consume the scrutinee; matching a unit variant never moves.
- The whole enum moves on transfer only when its payload is Owned; an
  all-Immutable enum copies and stays usable.

### 1.7 Equality

- Tagged-union equality is **not supported**: comparing two values of an
  enum with a data-carrying variant with `==`/`!=` is `E-T30` (enum
  equality). Unit-only enums keep Session-17 discriminant equality.

### 1.8 Diagnostics (new codes)

| Code | Kind | Meaning |
|------|------|---------|
| E-P25 | `EmptyPayload` | `Variant()` with no payload |
| E-T27 | `InvalidVariantPayload` | a payload type without a deterministic value layout |
| E-T28 | `VariantPayloadMismatch` | constructed payload does not match the declared type |
| E-T29 | `VariantPayloadArity` | payload given to a unit variant, or missing on a data-carrying one |
| E-T30 | `EnumEquality` | `==`/`!=` on a tagged-union enum |

All are structured errors with exact spans and stable codes, reported
before code generation.

## 2. Design boundaries

- **Enums stay closed and nominal.** Each declaration is a distinct type;
  the payload changes the value's *shape*, never its type.
- **The discriminant is still compiler-computed.** Declaration order
  assigns 0, 1, …; there is no `= n` syntax.
- **Tagged unions are aggregates, not scalars.** They have a deterministic
  byte layout (tag word + payload area), participate in the aggregate
  size bound, and are rejected where the native calling convention cannot
  carry them (a tagged union cannot be a function result — the convention
  returns one word).
- **No unsafe Rust.** Everything is safe Rust plus emitted machine code.
- **Deterministic.** Identical sources produce byte-identical images and
  identical runtime behavior.

## 3. Implementation architecture

- `src/ast/mod.rs` — `EnumVariant.payload: Option<Ty>`,
  `ExprKind::EnumVariant { name, variant, payload: Option<Box<Expr>> }`,
  `Pattern::EnumVariant { name, variant, payload: Option<Box<Pattern>> }`.
- `src/parser/mod.rs` — `parse_variant_payload_type` in enum declarations
  (with recovery that skips a malformed payload to its closing `)`, so an
  extra payload argument is not re-parsed as a variant name), the
  `E::V(expr)` construction arm in `parse_enum_variant`, and the payload
  pattern arm in `parse_pattern` (`E-P25` for empty payload lists).
- `src/parser/error.rs` — `ParseErrorKind::EmptyPayload` (`E-P25`).
- `src/semantics/analyzer.rs` — payload patterns bind their identifiers;
  construction payloads are plain expressions (no symbol changes).
- `src/typecheck/ty.rs` — `EnumVariantInfo.payload: Option<TypeId>`.
- `src/typecheck/checker.rs` — registration is restructured into four
  passes: enum names, struct names, enum variant resolution (payloads may
  reference structs/enums regardless of order), struct field resolution,
  then layout validation. Construction typing (`E-T28`/`E-T29`),
  payload-pattern checking, recursive payload-aware coverage
  (`E-T24`/`E-T25`), payload-type validation (`E-T27`), and tagged-union
  equality rejection (`E-T30`).
- `src/typecheck/error.rs` — the five new codes above; `E-T28` carries
  structured expected/actual payload types.
- `src/runtime/layout.rs` — `enum_layout` (tagged-union computation),
  `VariantPayloadLayout`, a **kind-tagged recursion path** (`PathEntry`)
  so struct and enum ids never collide, and `scalar_size_align` reporting
  only unit-only enums as scalar.
- `src/hir/mod.rs`, `src/hir/lower.rs` — payloads carried through
  `HirExprKind::EnumVariant` and `HirPattern::EnumVariant`.
- `src/mir/mod.rs`, `src/mir/lower.rs` — `MirRvalueKind::EnumInit`
  (construction), `EnumTag` (discriminant extraction), `EnumPayload`
  (payload extraction); `lower_match` emits the top-level tag test, then
  extracts the payload into a temporary, then matches the inner pattern
  through a chain of test blocks (`lower_payload_chain`). The optimizer
  and validator treat the three rvalues as opaque value producers.
- `src/backend/ir.rs`, `src/backend/lower.rs`, `src/backend/verify.rs` —
  `BInstKind::EnumInit`/`EnumTag`/`EnumPayload`; `classify_enum` and
  `words_of` give a tagged union its layout's word count; the emitter
  loads/stores the tag word and the payload area; `FieldLoad`/`FieldStore`
  accept enum bases; tagged-union function results are rejected
  (`E-B03`-style unsupported type).
- `src/ownership/mod.rs` — `Shape::Enum`/`State::Enum` with payload
  provenance, construction transfer, payload-pattern binding, and
  scrutinee consumption on owned-payload matches.

## 4. Conservative decisions and known limitations

- **No explicit discriminants in Session 19.** Values were assigned by
  declaration order; Session 20 added `= n` syntax (explicit tags work on
  data-carrying variants too) — see
  `docs/implementation/DISCRIMINANTS_IMPLEMENTATION.md`.
- **No tagged-union equality.** `==`/`!=` on a data-carrying enum is
  `E-T30`; unit-only enums keep discriminant equality.
- **No enum-to-`Int` conversion**, iteration, or deriving.
- **No function results of tagged-union type** — the native calling
  convention returns one word (rejected at backend lowering).
- **Payload arity is exactly one.** Tuples would be the mechanism for
  multi-value payloads and remain a future milestone.

## 5. Tests

`tests/sum_types.rs` covers the milestone end-to-end: parser
(declarations, construction, payload patterns, `E-P25`, recovery),
type system (construction typing, arity, mismatch, payload validation,
tagged-union equality rejection, payload-aware exhaustiveness, nested
coverage, unreachable arms), layout (tagged-union geometry, unit-only
scalars, recursion rejection), ownership (payload transfer, payload
binding moves, copy payloads), MIR lowering (EnumInit/EnumTag/EnumPayload,
determinism), backend lowering (multi-word locals, result rejection), and
native execution (payload matches dispatching, extracting, printing,
nested payloads, string payload round-trips, struct payloads, unit
variants alongside payloads). Session 20 (explicit discriminants) adds
`tests/discriminants.rs` — see `DISCRIMINANTS_IMPLEMENTATION.md` §5. The
whole suite is deterministic and runs through the same quality gates as
every milestone.
