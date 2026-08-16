# MINK Explicit Enum Discriminants Implementation

**Status:** Implemented (Session 20)
**Version:** 0.1.0

This document describes the MINK **explicit enum discriminants** milestone:
`enum E { A = 5, B }` declarations on unit and data-carrying variants,
implicit continuation (the next variant gets the previous value plus one,
starting at 0), the wrapping 64-bit literal model, duplicate-discriminant
rejection (`E-T31`), implicit-continuation overflow rejection (`E-T32`),
and the explicit tag values flowing through construction, pattern
matching, equality, tagged-union layout, MIR/backend lowering, and native
x86-64 execution. It builds directly on the enum foundation of Session 17
(`docs/implementation/ENUM_TYPES_IMPLEMENTATION.md`), the pattern-matching
foundation of Session 18 (`docs/implementation/PATTERN_MATCHING_IMPLEMENTATION.md`),
and the sum-types foundation of Session 19
(`docs/implementation/SUM_TYPES_IMPLEMENTATION.md`), and deliberately does
**not** introduce enum-to-`Int` conversion, iteration, deriving, or
tagged-union equality.

## 1. Frozen Session 20 rules

### 1.1 Explicit discriminant declarations

- A variant may declare an explicit discriminant: `enum E { A = 5, B }`.
  The value must be an **integer literal** — decimal, `0x`/`0o`/`0b`
  radix-prefixed, with `_` separators, optionally negated (`A = -1`) —
  exactly the literal forms a constant array index accepts. A missing,
  non-integer, or float value is the parse error `E-P19` (expected an
  integer literal), with recovery to the next variant.
- Unit variants (`A = 5`) and data-carrying variants (`B(Int) = 10`) may
  both carry explicit discriminants, and they mix freely with implicit
  variants (`enum E { A = 5, B, C(Int) = 10, D }`).
- The discriminant is a **compile-time constant**; `A = 5 + 1` has no
  syntax (the `+` is a parse error).

### 1.2 Effective discriminants and implicit continuation

- Every variant's **effective discriminant** is the tag word written by
  construction and tested by pattern matching. An explicit variant's value
  is its literal's wrapping 64-bit value (the language's literal model:
  `0xFFFFFFFFFFFFFFFF` is the same bit pattern as `-1`). An implicit
  variant's value is the previous variant's value plus one, starting at 0 —
  so `enum E { A, B }` keeps the Session-17 values `0, 1`, and
  `enum E { A = 5, B }` gives `B` the value `6`.
- Duplicate effective discriminants are rejected: two variants whose tags
  are equal could not be distinguished by the tag word. Both explicit
  duplicates (`A = 5, B = 5`) and implicit/explicit collisions
  (`A, B = 0`) are `E-T31`, reported at the later variant with the earlier
  variant as the related location. Only the root duplicate is reported.
- Implicit continuation that would overflow the 64-bit tag word (an
  implicit variant following an explicit `9223372036854775807`) is
  `E-T32`, reported once at the first variant that cannot continue. An
  explicit variant after the overflow resolves normally.
- Negative discriminants are allowed: they are ordinary 64-bit two's
  complement values, and implicit continuation applies unchanged
  (`A = -1, B` gives `B` the value `0`).

### 1.3 Semantics are unchanged

- The discriminant changes the value a variant's tag word holds — never
  the enum's type, coverage, or ownership. Exhaustiveness (`E-T24`/`E-T25`)
  tracks variants by name; pattern matching tests the explicit tag values.
- Unit-enum equality compares the (now explicit) tag words; tagged-union
  equality remains `E-T30`.
- Layout is unchanged: a unit-only enum is a single word (the tag), and an
  enum with a data-carrying variant is a tagged union (tag word + payload
  area). The backend looks up a variant's payload geometry by matching the
  discriminant **value**, not by indexing the variant list by position.

### 1.4 Diagnostics (new codes)

| Code | Kind | Meaning |
|------|------|---------|
| E-T31 | `DuplicateDiscriminant` | two variants share the same effective discriminant value |
| E-T32 | `DiscriminantOverflow` | an implicit discriminant would continue past the largest 64-bit value |

`E-P19` (expected an integer literal) is reused for malformed
discriminant literals. All are structured errors with exact spans and
stable codes, reported before code generation.

## 2. Design boundaries

- **The discriminant is still a single machine word.** Values are 64-bit
  two's complement, so a unit-only enum stays one word regardless of the
  tag values.
- **No enum-to-`Int` conversion**, iteration, or deriving. The tag value
  is only observable through construction, pattern matching, and unit-enum
  equality.
- **No tagged-union equality.** `==`/`!=` on a data-carrying enum remains
  `E-T30`, explicit tags or not.
- **No unsafe Rust.** Everything is safe Rust plus emitted machine code.
- **Deterministic.** Identical sources produce byte-identical images and
  identical runtime behavior; duplicate detection is order-based, so
  diagnostics never depend on hash iteration.

## 3. Implementation architecture

- `src/ast/mod.rs` — `EnumVariant.discriminant: Option<Expr>` (an integer
  literal, possibly a unary-minus literal); the variant span grows to cover
  `= literal`.
- `src/parser/mod.rs` — `parse_variant_discriminant` after the variant
  name and optional payload: `=` then an integer literal (optionally
  negated), with `E-P19` and recovery to the next variant boundary.
- `src/typecheck/error.rs` — `DuplicateDiscriminant` (`E-T31`, with the
  earlier variant as the related location) and `DiscriminantOverflow`
  (`E-T32`).
- `src/typecheck/checker.rs` — `resolve_enum_variants` walks the variants
  in declaration order computing effective discriminants: explicit values
  decode through the same wrapping literal path as constant array indices,
  implicit values continue from `previous + 1` (overflow tracked with
  `checked_add`), duplicates are detected against the running list, and
  `E-T31`/`E-T32` are reported once each. The `PendingVariant` struct
  carries the name, span, payload type, and discriminant expression.
- `src/typecheck/ty.rs` — `EnumVariantInfo.discriminant` widened from
  `u32` to `i64` (the tag is a full word).
- `src/runtime/layout.rs` — `VariantPayloadLayout.discriminant` widened to
  `i64`; geometry computation is unchanged.
- `src/mir/mod.rs`, `src/mir/lower.rs` — `EnumInit.discriminant` and
  `MirConstantKind::Enum { variant }` widened to `i64`; construction and
  match tag tests carry the explicit values.
- `src/backend/ir.rs`, `src/backend/lower.rs` — `EnumInit.discriminant`
  widened to `i64`; the payload-geometry lookup finds the variant layout by
  matching the discriminant value; `DecodedConstant::Word` carries the tag
  directly.
- `src/backend/emit/x86_64.rs` — the tag word is emitted with `movabs`
  directly from the `i64` value.

## 4. Conservative decisions and known limitations

- **Explicit values use the wrapping 64-bit literal model** — a literal
  outside `i64` range means the same two's-complement value it means
  anywhere else in the language (documented, tested). Only the implicit
  continuation (which has no literal to wrap) reports an overflow.
- **No ordering or arithmetic on tags** — the tag value is not `Int`; it
  cannot be compared with `<`, added, or converted.
- **No `A = 5 + 1` expressions** — only literals, exactly like constant
  array indices.
- **Discriminants do not affect function-result rules** — tagged unions
  still cannot be function results (the calling convention returns one
  word).
- **No reordering or stable-sorting of variants** — declaration order
  drives implicit continuation and duplicate reporting.

## 5. Tests

`tests/discriminants.rs` covers the milestone end-to-end: parser
(explicit discriminants on unit and data-carrying variants, negated/radix/
separated literals, `E-P19` for every malformed form, clean recovery),
type system (effective values, implicit continuation, mixed, negative,
wrapping, `E-T31` duplicates with related spans, single-root reporting,
`E-T32` overflow with no cascade, tagged-union equality still `E-T30`),
layout (unit-only enums stay scalar, tagged unions keep their geometry,
variant payload layouts record the explicit tags), MIR lowering
(unit construction, data-carrying construction, match tag tests,
determinism), backend lowering (multi-word locals, tagged-union result
rejection), ownership (copy semantics unchanged, owned payloads still
move, payload binding still consumes the scrutinee), and native execution
(dispatch, payload extraction, equality, negative/radix/separated tags,
mixed unit+payload enums, byte-identical image determinism). The whole
suite is deterministic and runs through the same quality gates as every
milestone.
