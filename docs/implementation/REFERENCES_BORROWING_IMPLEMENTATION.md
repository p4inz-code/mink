# MINK References and Borrowing Implementation

**Status:** Design frozen (Session 16)
**Version:** 0.1.0

This document freezes the Session 16 reference/borrowing rules **before**
implementation and records what is built. It adds explicit references
(`&T` / `&mut T`) with compile-time borrow checking on top of the Session
15 move-semantics foundation (`docs/implementation/OWNERSHIP_IMPLEMENTATION.md`),
which is **not** weakened: moves, provenance, E-S10/E-S11, and the
ownership gate before HIR all remain exactly as frozen.

## 1. Frozen Session 16 rules

The following rules are authoritative for this session.

### 1.1 Reference types

- **`&T`** — an immutable (shared) reference to a `T` value. **Copy**: any
  number of shared references may exist and may be copied freely.
- **`&mut T`** — a mutable (exclusive) reference to a `T` value.
  **Move-only**: exactly one exclusive reference exists; transferring it
  moves it (the source binding is dead after, like an Owned `Str`).
- Both are first-class types: interned structurally (identical element
  type and mutability share one `TypeId`), unified structurally, and
  displayed as `&Int` / `&mut Int`. `&T` never unifies with `&mut T`,
  with `Ptr`, `Str`, or any value type.
- **No reference-to-reference types.** `&r` where `r` is a reference is a
  type error (E-T19); `&&T` / `&mut &T` type syntax is rejected the same
  way. Passing a reference onward is by value (copy for `&T`, move for
  `&mut T`).
- `T` may be `Int`, `Bool`, `Str`, `Ptr<Int>`, a struct, or an array.
  (`Float`/`Char` locals are rejected by the backend today regardless.)

### 1.2 Borrow expressions

- `&place` — shared borrow. The place's **root binding** must be live and
  not exclusively borrowed; its shared-borrow count increments. Multiple
  shared borrows of the same root coexist.
- `&mut place` — exclusive borrow. The root must be live and **not
  borrowed at all** (no shared, no exclusive). The root binding must also
  be mutable (`let mut`) — otherwise E-S13.
- A borrow target is a **local place**: an identifier, a member chain
  (`p.x`), an index (`a[i]`), or a group of one. `&(a + b)`, `&5`, and
  `&*r` (deref-rooted borrows) are E-T19 this session (reborrowing is
  deferred). `&e` where `e` is already a reference is E-T19.
- **Root-level tracking.** Borrowing any member or element borrows the
  whole root binding (conservative): `&p.x` and `&p.y` coexist (both
  shared), but `&mut p.x` conflicts with any other borrow of `p`. Shared
  borrows never conflict with each other; only exclusive borrows conflict.
- While a root is **shared-borrowed**: reads are allowed; mutation
  (`rt_str_set_byte`, assignment through it) and consumption
  (`rt_str_free`, transfers) are **E-S12** (borrow conflict).
- While a root is **exclusively borrowed**: any direct use of the root
  (read, write, move, re-borrow) is **E-S12**. Reads and writes *through*
  the exclusive reference are the point of the borrow and are allowed.

### 1.3 Deref

- `*r` — the referent of reference `r`. Reading `*r` yields a copy of the
  referent value (for a `Str` referent, an Immutable read view — moving
  out through a reference is intentionally impossible; the runtime
  remains the backstop, E-R05/E-R06).
- `*r` as an assignment target requires `r: &mut T` (else E-T21 for
  `&T`); the value must unify with `T`.
- `*r` where `r` is not a reference is **E-T20** (cannot dereference).
- Mutating intrinsic calls on a deref (`rt_str_set_byte(*r, …)`) require
  an exclusive reference (E-S12 through a shared one); consuming
  intrinsics on a deref (`rt_str_free(*r)`) are always E-S12 (cannot free
  through a reference).

### 1.4 Borrow lifetimes (lexical, local)

- A borrow lives exactly as long as the **reference binding holding it**
  is live. It is released when that binding is reassigned, moved,
  overwritten, or when the block that declared it exits (scope exit).
- A reference **cannot escape its function**: returning a reference to a
  function-local binding is **E-S14** (dangling reference).
- **Reference parameters** (`fn f(r)` called with `&x` / `&mut x`) are
  *caller borrows*: inside the callee their source is the caller's frame.
  A callee may read through them, write through an exclusive one, pass
  them on, and **return a reference derived directly from a reference
  parameter** (the caller's borrow propagates to the call result).
- At a call site, `f(&x)` / `f(&mut x)` holds the borrow of `x` for the
  duration of the call. If the callee returns a reference derived from
  that parameter, the borrow transfers to the call result (`let y =
  f(&x)` makes `y` borrow `x`); otherwise the borrow is released when the
  call completes.
- **Function result provenance (extended).** Each function's result
  records, when it returns a reference, which parameter index the
  reference derives from (computed by the same deterministic fixpoint as
  Session 15 provenance). A reference return from two different
  parameters is conservatively treated as an unknown-source caller borrow
  (see §4).
- **Loops and branches.** Borrows declared inside a loop body are
  released when the body's block exits (per iteration), so a borrow
  cannot escape a loop. Branch merging is deterministic: the walk is
  linear and source-ordered; moves/borrows from earlier branches are
  reflected in later code (conservative — a borrow or move in one `if`
  arm is treated as happening, matching Session 15's deterministic
  branch-merge behavior).

### 1.5 Aggregates with reference fields

- A struct may declare reference-typed fields: `struct S { r: &Int }`.
  Constructing `S { r: &x }` borrows `x` (the borrow is attached to the
  struct binding and released when it dies).
- A struct containing only `&T` fields is Copy (copies share the borrow —
  the shared count increments); a struct containing a `&mut T` field is
  move-only.
- Arrays of references are supported as values; reading `a[i]` yields the
  reference (copy or move by element type).
- Struct/array values containing references cannot be returned from
  functions or stored at module scope (existing E-B03 / constant rules).

### 1.6 Diagnostics (stable E-T / E-S ranges)

| Code | Kind | Meaning |
| ---- | ---- | ------- |
| E-T19 | `InvalidBorrowTarget` | `&`/`&mut` of a non-place, of a reference, or of a deref (borrow/reborrow forms not in the model) |
| E-T20 | `DerefNonReference` | `*e` where `e` is not a reference |
| E-T21 | `AssignThroughImmutableRef` | assignment through `&T` (only `&mut T` allows writes) |
| E-S12 | `BorrowConflict` | conflicting borrows; mutation/consumption while borrowed; free through a reference |
| E-S13 | `InvalidBorrow` | `&mut` of an immutable (`let`-declared) binding |
| E-S14 | `DanglingReference` | returning a reference to a function-local value |

### 1.7 Pipeline and determinism

- The ownership analysis (already in the pipeline between type analysis
  and HIR) gains borrow tracking; it still runs only on a clean semantic
  and type front end, still gates HIR lowering, and remains deterministic
  and source-ordered (no `HashMap` iteration order may influence any
  diagnostic).
- The type checker rejects E-T19/E-T20/E-T21; the borrow analyzer rejects
  E-S12/E-S13/E-S14. Invalid programs fail before code generation.

### 1.8 Deliberately absent (later sessions)

- Reborrowing (`&*r`), reference-to-reference types, explicit lifetime
  annotations or inference beyond the lexical model above, disjoint-field
  exclusive borrows, refs to derefs, reference comparison/arithmetic,
  references in module statics, `&` of literals (promotion).
- Drops/RAII, shared ownership, weak references — unchanged.

## 2. Design boundaries

- **References are addresses in the native backend.** A reference value
  is a single word holding the machine address of a stack slot (or of a
  field/element region inside one). `&place` computes the address with the
  same deterministic place machinery the backend already uses for
  aggregate stores (`E-R10` bounds checks on index steps); `*r` loads or
  stores `size` bytes through the address. Mutability is purely a
  compile-time concept — the ABI is an address either way.
- **Sound by construction.** Borrows cannot escape their function
  (E-S14), cannot outlive their source (lexical release), and the runtime
  remains the safety backstop for everything else (E-R04–E-R10). The
  analyzer is conservative: it can only reject (never accept) an unsafe
  program.
- **No unsafe Rust.** Everything is safe Rust plus emitted machine code.

## 3. Implementation architecture

- `src/typecheck/ty.rs` — `TypeKind::Ref { mutable, elem }`, interned and
  structurally unified; `display` renders `&T` / `&mut T`.
- `src/ast/mod.rs` — `ExprKind::Borrow { mutable, operand }`,
  `ExprKind::Deref { operand }`, `TyKind::Ref { mutable, inner }`.
- `src/parser/mod.rs` — unary `&` / `&mut` / `*` expressions; type syntax
  `&T` / `&mut T`.
- `src/typecheck/checker.rs` — borrow/deref typing, `E-T19`/`E-T20`/
  `E-T21`, reference types in struct fields, restrictions (§1.8).
- `src/hir/`, `src/mir/` — `Borrow`/`Deref` expression forms; MIR gains a
  `RefAddr` rvalue (root + place steps) and a `Deref` read rvalue /
  `Deref` assignment target; validate/optimize extended conservatively.
- `src/backend/` — `BType::Ref` (word-sized), `RefAddr`/`RefLoad`/
  `RefStore` instructions, verifier checks, x86-64 emission reusing the
  slot-address and byte-copy machinery.
- `src/ownership/mod.rs` — borrow state (`Shared(count)` / `Exclusive` /
  `None`) per root binding, borrow views on reference values, lexical
  release (reassignment, move, scope exit), call/return reference flow
  through the fixpoint, `E-S12`/`E-S13`/`E-S14`.

## 4. Conservative decisions and known limitations

- **Disjoint-field exclusive borrows are rejected.** `&mut p.x` conflicts
  with any other borrow of `p` even though the fields are disjoint.
- **Multi-source reference returns** (a function that may return a
  reference derived from either of two parameters) keep the borrows of
  every reference-typed argument alive for the rest of the calling scope
  (unknown-source result; conservative over-rejection only).
- **No reborrowing:** passing an exclusive reference on requires a move
  (`f(r)` consumes `r`); you cannot write `&mut *r`. `&T` copies onward.
- **Moving out through a reference is impossible:** `let t = *r` for a
  `Str` referent copies an Immutable view; freeing it is rejected by the
  existing runtime validation (E-R05), never unsafety.
- **Borrows of array elements** borrow the whole array root (per-element
  liveness is out of scope, as in Session 15).
- Loop-body borrows are released at each iteration's end; a reference
  cannot be stored across iterations except through an outer binding,
  which is tracked normally.

## 5. Examples

```mink
struct P { x: Int, tag: Bool }

fn read(r) {
    rt_print_int(*r);        // shared reference parameter, read through
}

fn bump(r) {
    *r = *r + 1;             // exclusive reference parameter, write through
}

fn identity(r) {
    return r;                // caller borrow propagates to the result
}

fn main() {
    let mut v = 41;
    let r1 = &v;             // shared borrow of v
    let r2 = &v;             // a second shared borrow is fine
    read(r1); read(r2);
    let w = &mut v;          // exclusive borrow: no other borrow may be live
    bump(w);                 // v is now 42
    let y = identity(w);     // the exclusive borrow transfers to y
    rt_print_int(*y);        // 42
    rt_print_int(v);         // 42 (y is dead after last use? no: lexical —
                             // y is live until scope exit, so v stays
                             // borrowed here; reads of v while y is live
                             // are E-S12 in this model)
}
```

Note the last line: with lexical lifetimes, `v` remains exclusively
borrowed until `y`'s binding dies (scope exit), so reading `v` directly
while `y` is live is a borrow conflict. Programs must structure around
this (drop `y` by reassigning it, or use a scope).

## 6. Validation

- `tests/references.rs` — type-level rules (E-T19/E-T20/E-T21, refs in
  structs, no ref-of-ref), borrow rules (E-S12/E-S13/E-S14), lexical
  release across scopes, multiple shared borrows, conflicting
  mutable/immutable borrows, mutation through `&mut`, reference
  parameters and returns, recursive and mutually recursive functions,
  struct/array/field/index references, owned strings +  references, pointers vs references, branch joins, loops, moved values +
  references, deep nesting, deterministic diagnostics, and native
  end-to-end programs with exact output and exit codes. Parser, typecheck,
  and backend suites add structural coverage of the new syntax, types, and
  instructions (`tests/parser.rs`, `tests/typecheck.rs`, `tests/backend.rs`).
- Existing suite (878 tests) remains green; the full suite after session
  17 (data-free enums) is **919 tests** (see
  `NATIVE_BACKEND_IMPLEMENTATION.md` §13).

## 7. Status

Frozen for the constructs it covers. Statements and declarations outside
it are rejected with stable diagnostics. Later sessions extend this
document additively.
