# MINK Enum Types Implementation

**Status:** Implemented (Session 17)
**Version:** 0.1.0

This document describes the MINK **enum** foundation: user-declared
enumerations of named alternatives (`enum Direction { North, South }`),
variant-construction paths (`Direction::North`), nominal enum typing,
single-word discriminant layout, enum equality, and native x86-64
execution. It builds directly on the aggregate foundation of Session 14
(`docs/implementation/AGGREGATE_TYPES_IMPLEMENTATION.md`) and the
ownership/move and reference/borrowing foundations of Sessions 15–16, and
deliberately does **not** introduce data-carrying variants, pattern
matching, exhaustiveness, generics, or implicit conversions.

## 1. Frozen Session 17 rules

### 1.1 Enum declarations

- An enum is declared `enum Name { Variant, ... }` and lives in the
  **type namespace**, exactly like a struct name: it never collides with
  functions, bindings, parameters, or loop variables. Struct and enum
  names share one type namespace, so two enums may not share a name
  (`E-S15`), and an enum and a struct may not share a name (the later
  declaration reports its own duplicate kind: `E-S08` for a struct after
  an enum, `E-S15` for an enum after a struct). The first declaration
  wins.
- Variants are named alternatives in declaration order. A variant name
  may repeat across different enums (`enum E { A } enum F { A }` is fine);
  a duplicate within one enum is `E-S16` (first variant wins).
- The variant list may be empty syntactically (`enum Empty {}`); an enum
  with no variants simply has no constructible values. Trailing commas
  are allowed.
- Enum names are never value symbols: `E` alone is not an expression
  (referencing it is `E-S01`), only `E::Variant` is.

### 1.2 Variant construction

- `E::V` constructs the enum value whose discriminant is `V`'s position
  in `E`'s variant list (0-based, in declaration order). The expression's
  type is the nominal enum type `E`.
- A missing variant name (`E::`) is a parse error (`E-P22`).
- A path whose first segment names a non-enum type (`S::Q` on a struct,
  `Int::Foo`) is `E-T22` (not an enum).
- A path whose variant is not declared by the enum (`E::Missing`) is
  `E-T23` (unknown variant).

### 1.3 Enum values

- An enum value is a **single machine word** holding its variant's
  discriminant (0 for the first variant, 1 for the second, …). Layout is
  `(8, 8)` — the same word class as `Int`/`Bool`/pointers — so enums are
  scalars: they copy freely, are never heap-allocated, and are never
  subject to ownership/move analysis.
- Enum values are comparable with `==` and `!=`; the result is `Bool`.
  Equality requires the **same nominal enum type** — comparing `E::A ==
  F::A` is `E-T02`, because the discriminants alone do not identify the
  type.
- There is no ordering (`<`, `>`) between enum values this session, no
  arithmetic, and no conversion to/from `Int` (the discriminant is
  compiler-computed, not a user-facing literal).

### 1.4 Composition

- Enums compose with the existing value model: an enum can be a struct
  field type, an array element type, a function parameter, a `let`
  binding, or a function return value. Struct literals, member access,
  array literals, indexing, and calls all accept enum values.
- Passing an enum to a function copies the word; assigning `e2 = e1`
  copies the word; `let copy = e;` copies the word. No move semantics
  apply (enums own nothing).

### 1.5 Diagnostics (stable E-P / E-S / E-T ranges)

| Code | Kind | Meaning |
|------|------|---------|
| E-P22 | `ExpectedVariant` | a variant name is required after `::` |
| E-S15 | `DuplicateEnum` | two enum declarations share a name |
| E-S16 | `DuplicateVariant` | a variant is declared twice in one enum |
| E-T22 | `NotAnEnum` | `X::V` where `X` is not an enum type |
| E-T23 | `UnknownVariant` | `E::V` where `E` has no variant `V` |

All five are structured errors with exact spans, stable codes, and (for
`E-S15`/`E-S16`) a related "original declaration" span. They fail before
code generation.

### 1.6 Pipeline and determinism

- Enum declarations flow through every stage: AST → semantic analysis
  (type namespace) → type checking (nominal registration) → HIR (enum
  item + variant expression) → MIR (enum-variant constant carrying the
  compiler-computed discriminant) → optimization (the discriminant is a
  value, so constant folding/copy propagation treat it like any word
  constant) → native backend (`BType::Enum`, a single word) → emitted
  machine code.
- Discriminants are assigned deterministically from declaration order, so
  identical sources always produce identical values and identical images.

### 1.7 Deliberately absent (later sessions)

- Data-carrying variants (`enum Option { Some(Int), None }`), payload
  storage, and sum-type layout.
- Pattern matching and exhaustiveness diagnostics.
- Explicit discriminants (`enum E { A = 5 }`).
- `Int`/enum conversion, enum iteration, or deriving.
- Generics over enums.

## 2. Design boundaries

- **Enums are closed, nominal types.** Each declaration is a distinct
  type; `E::A == F::A` fails because the types differ, not because the
  discriminants differ.
- **Enums are scalars.** A single word holds the discriminant; there is
  no tag+payload representation, no heap allocation, and no alignment
  beyond word alignment.
- **No implicit conversions.** An enum is never silently coerced to
  `Int` or back.
- **No unsafe Rust.** Everything is safe Rust plus emitted machine code.
- **Deterministic.** Identical sources produce byte-identical images and
  identical runtime behavior.

## 3. Implementation architecture

- `src/ast/mod.rs` — `ItemKind::Enum(EnumItem)`, `EnumItem`, `EnumVariant`,
  and `ExprKind::EnumVariant { name, variant }` (both names `Box`ed so the
  `Expr` node stays 80 bytes — the same size as before enums, preserving
  the parser's documented deep-nesting stack budget).
- `src/parser/mod.rs` — `parse_enum` (variant list, trailing comma,
  recovery via `skip_to_variant_boundary`), `parse_type` untouched
  (enums are named types), and the `Ident :: Ident` expression arm that
  produces `ExprKind::EnumVariant` or records `E-P22`.
- `src/parser/error.rs` — `ParseErrorKind::ExpectedVariant` (`E-P22`).
- `src/semantics/analyzer.rs` — `register_enum` under the shared
  `register_type` type-namespace machinery: `E-S15` for duplicate enum
  names, `E-S16` for duplicate variants. Struct names and enum names
  share one type namespace (both declarations register through the same
  `types` map), so `struct E` + `enum E` duplicates are also rejected
  (with the later declaration's duplicate kind).
- `src/typecheck/ty.rs` — `TypeKind::Enum(EnumId)`, `EnumId`,
  `EnumInfo` (name + variants with deterministic discriminants),
  `TypeTable::register_enum` / `enum_info` / `enums` / `enum_id`, and
  `display` rendering (`E` for the enum type).
- `src/typecheck/checker.rs` — enums are registered (and their variant
  tables resolved) before struct fields are typed, so `struct T { c:
  Color }` resolves; variant expressions type as the enum type
  (`E-T23` unknown variant, `E-T22` non-enum path); equality unifies the
  same nominal enum type (`E-T02` on mismatch).
- `src/typecheck/error.rs` — `TypeErrorKind::NotAnEnum` (`E-T22`),
  `UnknownVariant` (`E-T23`).
- `src/runtime/layout.rs` — `TypeKind::Enum(_)` classifies as
  `(WORD_SIZE, WORD_SIZE)` in `scalar_layout`/`scalar_size_align`.
- `src/ownership/mod.rs` — enum values are `Immutable`/copyable: variant
  expressions and enum-typed operands never move, so no ownership rule
  applies (there is nothing to own).
- `src/hir/mod.rs`, `src/hir/lower.rs` — `HirItemKind::Enum(HirEnum)` and
  `HirExprKind::EnumVariant { enum_id, variant }`.
- `src/mir/mod.rs`, `src/mir/lower.rs` — `MirConstantKind::Enum {
  variant }` carries the compiler-computed discriminant; enum variant
  expressions lower to a `Use(Constant(Enum { .. }))` rvalue. `validate`
  and `optimize` treat it as a plain word constant (no source-text
  decoding needed — the discriminant is already the value).
- `src/backend/ir.rs`, `src/backend/lower.rs`, `src/backend/verify.rs` —
  `BType::Enum` (single word, like `Int`/`Bool`/pointers); variant
  constants decode to `DecodedConstant::Word(discriminant)`; the verifier
  accepts enum-typed word operands for `LoadConst`, comparisons, calls,
  and returns.

## 4. Conservative decisions and known limitations

- **No payloads.** Variants carry no data; a future session adds
  data-carrying variants and sum-type layout.
- **No pattern matching.** Matching/`switch` over variants is future
  work; today programs test equality (`e == E::V`).
- **No explicit discriminants.** Values are assigned by declaration
  order; there is no `= n` syntax.
- **Empty enums are legal but useless.** `enum Empty {}` compiles (and
  its layout is still a word), but no value of that type can be
  constructed, so `E::V` paths into it are `E-T23`.
- **Enum vs struct names share the type namespace.** A struct and an enum
  may not share a name; the later declaration reports its own duplicate
  kind (`E-S08`/`E-S15`).

## 5. Examples

```mink
enum Direction { North, South, East, West }
enum Color { Red, Green, Blue }

struct Tag { c: Color, id: Int }

fn label(t) {
    if t.c == Color::Red { return 10; }
    if t.c == Color::Green { return 20; }
    return 30;
}

fn main() {
    let mut t = Tag { c: Color::Green, id: 5 };
    rt_print_int(label(t));          // 20
    let colors = [Color::Red, Color::Blue];
    if colors[1] == Color::Blue { rt_print_int(99); }   // 99
    t.c = Color::Red;
    rt_print_int(label(t));          // 10
    return;
}
```

## 6. Validation

- `tests/enums.rs` (25 tests) — enum declaration parsing (including
  trailing comma and empty enums), variant-path parsing, structured
  parser errors and recovery, `E-S15`/`E-S16` duplicates, nominal enum
  typing, `E-T22`/`E-T23`, enum equality across/within enums, enum in
  structs and arrays, discriminant order, single-word layout
  (`(8, 8)`), MIR enum-constant lowering, backend `BType::Enum` locals,
  discriminant survival through optimization, and native end-to-end
  programs (equality, copies, function parameters/returns, struct/array
  composition, and a 300-variant enum) with exact output and exit codes.
- `tests/parser.rs` (+3) — enum declarations as items, `E::V` variant
  paths, `E-P22` with no variant name.
- `tests/semantics.rs` (+3) — `E-S15` duplicate enum, shared
  enum/struct type namespace (`E-S08`/`E-S15`), `E-S16` duplicate
  variant.
- `tests/typecheck.rs` (+7) — variant expressions typed as the enum,
  nominal distinctness, `E-T23`/`E-T22`, enum assignment mismatch,
  equality/inequality typing, enums in struct fields.
- `tests/backend.rs` (+2) — enum locals lower to word-sized
  `BType::Enum`, discriminant values survive lowering and folding.
- Existing suite (878 tests) remains green; full suite after session 17:
  **919 tests**, all passing.

## 7. Status

Frozen for the constructs it covers. Statements and declarations outside
it are rejected with stable diagnostics. Later sessions extend this
document additively.
