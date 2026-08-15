# MINK Aggregate Types Implementation

**Status:** Implemented (Session 14)
**Version:** 0.1.0

This document describes the second memory-backed aggregate foundation of the
MINK language: user-declared **structs** and fixed-size **arrays** with
deterministic byte layout, struct/array literals, member/index access, and
native x86-64 execution. It builds directly on the typed pointer/string
memory model of Session 13 (`docs/implementation/STRING_MEMORY_IMPLEMENTATION.md`)
and deliberately does **not** introduce explicit borrow syntax, GC,
arbitrary raw-pointer syntax, generics, or a dynamic object model. Since
Session 15, compile-time **move semantics** apply to aggregates
containing heap-owning values (an Owned-containing struct moves as a
whole; an all-immutable struct copies) — see
`docs/implementation/OWNERSHIP_IMPLEMENTATION.md`.

## 1. Design boundaries

- **Deterministic C-style layout.** Struct and array values have a byte
  layout computed once from the type table by
  `src/runtime/layout.rs` (`struct_layout` / `array_layout`): field offsets
  follow declaration order with per-field alignment, arrays are contiguous
  runs of `len` elements with stride equal to the element size. Identical
  declarations always yield identical layouts.
- **Aggregates are values.** A struct or array value occupies a stack slot
  like any other value; assignment and argument passing copy the whole
  value (word-wise; byte-wise for unaligned tails). There is no reference
  semantics and no implicit sharing.
- **Place semantics.** `base.field` and `base[i]` are *places*: reading
  evaluates the base and indexes into the value; assignment
  (`base.field = v`, `base[i] = v`, and chains like `g.rows[1].y = 40`)
  resolves the full path and stores through it, so nested mutation reaches
  the root object.
- **No explicit borrow syntax; no GC; no arbitrary raw-pointer syntax;
  no generics.** Since session 15, aggregates containing an Owned value
  move as a whole on transfer (use-after-move is `E-S10`); aggregates
  holding only Immutable values copy freely. Unsupported aggregate
  behavior fails deterministically at compile time (`E-B03`, see §6).
- **No unsafe Rust.** Everything is safe Rust plus emitted machine code.
- **Deterministic.** Identical sources produce byte-identical images and
  identical runtime behavior; every runtime error has a stable code and
  exit status.

## 2. Language surface

```mink
struct Point { x: Int, y: Int }        // declaration: name + field list
struct Grid { rows: [Point; 2] }       // array-typed fields: [T; N]

fn main() {
    let p = Point { x: 1, y: 2 };      // struct literal (all fields)
    let a = [1, 2, 3];                 // array literal (length inferred)
    let g = Grid { rows: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }] };

    p.x;                               // member read  -> Int
    a[0];                              // index read   -> Int
    p.x = 10;                          // member store (p must be `mut`)
    a[1] = 20;                         // index store  (a must be `mut`)
    g.rows[1].y = 40;                  // deep place store
}
```

- **Struct declarations** (`struct Name { f: T, ... }`) live in the type
  namespace, separate from value names; field types are resolved after all
  declarations are visible, so structs may reference each other regardless
  of declaration order (but only **acyclically** by value — §4).
- **Array types** are written `[T; N]` with a positive integer literal
  length `N >= 1`.
- **Struct literals** must name every declared field exactly once, in any
  order: `P { x: 1, y: 2 }`. In condition positions (`if P { ... }`) the
  literal must be parenthesized, because `Name {` otherwise opens a block.
- **Array literals** infer their length from the element count:
  `[e1, e2, e3]`. Elements must all have one common type.
- Aggregate **returns** (a function whose result is a struct/array) and
  aggregate **module statics** are rejected deterministically this session
  (see §6); passing aggregates **into** functions is supported.

## 3. Type system changes

- `TypeKind::Struct(StructId)` — a nominal type: each declaration is a
  distinct type (the first declaration of a name wins; duplicates are
  `E-S08`). Display is the declared name (`Point`).
- `TypeKind::Array { elem: TypeId, len: u64 }` — a structural type:
  canonicalized, so `[Int; 3]` compares equal by identity across the
  program. Display is `Array<Int, 3>`.
- `StructInfo` / `StructFieldInfo` — the type table records each struct's
  name and its fields (name + field `TypeId`), in declaration order.
  Struct ids are assigned sequentially as declarations are registered and
  index the table's struct list.
- `TypeId`/`StructId` are opaque stable ids; struct types are never
  inference variables, so member/index resolution is a table lookup, not a
  unification.
- Deferred member/index typing: when a member/index expression's base is
  an unresolved inference variable (e.g. a parameter whose type is only
  pinned at call sites), the checker records the expression and re-types
  it in a second pass once the base type is known. The second pass is
  bottom-up: every parent that consumed a deferred expression is re-checked
  against the resolved types (operators, assignments, conditions,
  iterables, returns), so a mismatch that was invisible while the base was
  unresolved — `s + g.tag` with a `Bool` field, an `Int` field used as a
  condition — is diagnosed (`E-T02`/`E-T01`) instead of silently compiling;
  diagnostics the recomputation reproduces are deduplicated.

## 4. Deterministic layout model

`src/runtime/layout.rs` is the authoritative layout engine. Scalar
size/alignment (`scalar_layout`):

| Type        | Size | Align |
| ----------- | ---- | ----- |
| Int, Float, Str, Null, Ptr\<T\> | 8 | 8 |
| Bool, Char  | 1  | 1  |
| Range\<T\>  | 16 | 8  |
| Unit        | 0  | 1  |

Aggregate rules:

- **Struct:** fields are placed in declaration order; each field's offset
  is rounded up to the field's alignment; the struct's size is rounded up
  to its alignment; the struct's alignment is the maximum field alignment.
  Example: `struct F { a: Bool, b: Int, c: Bool, d: Int }` → offsets
  0, 8, 16, 24; size 32; align 8.
- **Array:** elements are laid out consecutively with stride equal to the
  element size; size is `len * elem_size`; alignment is the element
  alignment. `[Bool; 3]` is 3 bytes with stride 1; `[Point; 2]` is 32
  bytes with stride 16.
- **Nested aggregates inline** (no indirection): a struct field of struct
  type occupies that struct's full layout inside the parent.
- **Recursion is rejected:** a struct reachable from itself by value
  (directly or through other structs/arrays) has no finite size and is
  `E-T18` at compile time; the layout engine returns
  `LayoutError::Recursive` rather than looping.
- **Empty structs** (`struct P { }`) are `E-T18` (`LayoutError::Empty`).
- **Size bound:** every aggregate value is bounded by
  `MAX_AGGREGATE_BYTES` (1 MiB, the size of the fixed heap); a larger
  type is rejected (`E-T18` / `LayoutError::TooLarge`). Size arithmetic is
  checked (`LayoutError::Overflow` never panics).

**Value image in a slot.** A scalar value occupies its slot's first words.
An aggregate value's bytes run **downward** from the slot's first word:
byte `b` of the value lives at `word0 - b` (word `k` at `word0 - 8k`).
This keeps unaligned fields (a `Bool` at byte offset 3) addressable from
the slot base with one `lea`, and is the convention the emitter's copy
helpers and bounds-checked addressing all share. Struct/array slots are
allocated with `ceil(size / 8)` words.

## 5. Pipeline

### Semantics (`src/semantics`)

- Struct names are registered in a **type-namespace** table separate from
  the value `SymbolTable`; duplicate struct names are `E-S08`, duplicate
  fields within one struct are `E-S09`. The first declaration of a name
  wins. Struct literals never resolve their name as a value.

### Type checking (`src/typecheck`)

- Struct declarations are pre-registered (with no fields), then all field
  types are resolved and recorded once every declaration is visible, so
  mutual references work.
- Struct literals: unknown type `E-T15`, unknown field `E-T12`, missing
  field `E-T13`, duplicate initializer `E-T14`; every initializer must
  type-match its field.
- Member access: base must be a struct (`E-T07`), member must exist
  (`E-T08`); the expression types as the field type.
- Index access: base must be an array (`E-T09`), index must be `Int`
  (`E-T10`); a constant index that is `>= len` or `< 0` is `E-T11` (a
  variable index is bounds-checked at runtime, `E-R10`); the expression
  types as the element type.
- Array literals: non-empty (`E-T17`), elements unified to one type.
  Array types: length must be a positive integer literal (`E-T16`).
- Invalid aggregate layout (recursive, empty, oversized) is `E-T18`.
- Member/index assignment requires the base binding to be `mut`
  (semantics `AssignmentToImmutable`), and the stored value must
  type-match the field/element (`E-T01`).

### HIR / MIR

- `HirStruct` items carry the declared fields; HIR has `StructLit` and
  `ArrayLit` expression nodes with exact spans.
- MIR gains a **place** representation: a read of `base.field` /
  `base[i]` is a `Member`/`Index` rvalue; a write is a `MirTargetKind`
  whose path is a list of `Field(name)` / `Index(operand)` steps over an
  evaluated base. Deep chains (`g.rows[1].y`) lower to nested place steps,
  so stores reach the root object instead of a copy.
- Struct items are skipped during MIR lowering (types are compile-time
  only). Struct/array literals materialize into a temp via field/element
  stores, then copy into the binding.

### Backend IR and lowering (`src/backend`)

- `BType::Struct` / `BType::Array` classify aggregate locals; `words()`
  gives the slot word count from the resolved layout.
- `BInstKind::FieldLoad` / `FieldStore` — read/write a field at its
  resolved byte offset within a struct slot.
- `BInstKind::IndexLoad` / `IndexStore` — read/write an element at
  `index * stride` within an array slot, with a runtime bounds check.
- `BInstKind::PlaceStore` — a store through a resolved chain of steps
  (`field` at offset / `index` scaled by stride), used for deep place
  mutation.
- `BProgram` carries the struct/array layout tables; lowering resolves
  every member/index against `struct_layout` / `array_layout` (the same
  engine the type checker used), so backend and typechecker cannot
  disagree about offsets.
- Aggregate **returns** and aggregate **module statics** are rejected
  during lowering (`E-B03`), so no unsupported aggregate ABI path exists.

### x86-64 emitter (`src/backend/emit/x86_64.rs`)

- Slot computation and frame sizing use the resolved layout; aggregate
  arguments are copied word-wise into callee slots (byte-wise for
  unaligned tails) using the downward value-image convention.
- `IndexLoad`/`IndexStore`/`PlaceStore` index steps emit a bounds check
  (`0 <= i < len`) that jumps to the shared `E-R10` fail block at the end
  of the function (not inline, so valid indices never fall through into
  the fail call).

## 6. Error catalog (new in this session)

| Code  | Where     | Meaning                                        |
| ----- | --------- | ---------------------------------------------- |
| E-S08 | semantics | duplicate struct declaration                   |
| E-S09 | semantics | duplicate field in one struct                  |
| E-T07 | typecheck | member access on a non-struct                  |
| E-T08 | typecheck | unknown member                                 |
| E-T09 | typecheck | index access on a non-array                    |
| E-T10 | typecheck | non-`Int` index                                |
| E-T11 | typecheck | constant index out of range                    |
| E-T12 | typecheck | unknown field in a struct literal              |
| E-T13 | typecheck | missing field in a struct literal              |
| E-T14 | typecheck | duplicate field initializer                    |
| E-T15 | typecheck | unknown type in a struct literal / field type  |
| E-T16 | typecheck | invalid array length (zero or non-literal)     |
| E-T17 | typecheck | empty array literal                            |
| E-T18 | typecheck | invalid aggregate layout (recursive/empty/oversized) |
| E-R10 | runtime  | array index out of range (exit status 110)     |
| E-B03 | backend  | unsupported aggregate return/static            |

All errors carry exact source spans and structured messages.

## 7. What is deliberately absent

- No explicit borrow syntax or GC; move semantics for Owned-containing
  aggregates are enforced at compile time (session 15, `E-S10`).
- No arbitrary raw-pointer syntax (`&`, casts, address-of).
- No generics; no reflection or dynamic object model.
- No aggregate returns or aggregate module statics (rejected with `E-B03`).
- No string concatenation/slicing beyond Session 13's intrinsics.

## 8. Validation

- `tests/aggregate.rs` (59 tests): struct/array parsing and recovery,
  duplicate-name/field diagnostics, member/index typing, literal rules
  (`E-T12`–`E-T17`), constant-index range checking, layout determinism
  (offsets, alignment, sizes, strides, nesting, bool packing, arrays of
  structs), recursive/empty layout errors, and native end-to-end
  execution: field access/mutation, deep place chains, array mutation with
  dynamic indices, `E-R10` bounds checks (including inside place chains
  and negative indices), bool fields, struct copy semantics, structs
  containing strings, struct arguments, and byte-identical image
  determinism.
- Existing suite: full `cargo test` passes **803 tests** (703 before
  Session 14 + 59 new aggregate tests + 41 ownership tests from Session
  15). See `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md` §13 for
  the per-file breakdown.

## 9. Known limitations

- Aggregate values cannot be returned from functions or stored at module
  scope yet (`E-B03`).
- Arrays are fixed-size values; no resizing, no slices, no iteration
  beyond manual index loops.
- No enum/unions/tagged values; structs and arrays are the only
  user-defined aggregate forms.
- The 1 MiB heap bound is also the maximum single aggregate value size.
