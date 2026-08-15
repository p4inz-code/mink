# MINK Ownership and Borrowing Implementation

**Status:** Implemented (Session 15)
**Version:** 0.1.0

This document freezes the Session 15 ownership/borrowing rules **before**
implementation and records what was built. It describes a minimal, sound
ownership foundation: move semantics for heap-owning values with
compile-time use-after-move detection, and implicit function-local borrows
for reads/mutation of strings. It deliberately does **not** add borrow
syntax, lifetimes, generics, or full Rust-level borrow checking.

## 1. Frozen Session 15 rules

The following rules are authoritative for this session.

### 1.1 Value classes

- **Copy types** (always copy, never move): `Int`, `Float`, `Bool`,
  `Char`, `Range<T>`, `Unit`, and `Ptr<T>`. Raw pointers are Copy by
  design: every access is validated by the runtime (liveness table,
  alignment, bounds), so aliasing a pointer is safe.
- **`Str` values** are one of two provenances:
  - **Immutable** — a string literal: immortal image data. Copying is
    always harmless; copying an Immutable `Str` keeps the source usable.
  - **Owned** — a heap blob (from `rt_str_alloc`), a function parameter,
    or the result of a call whose result provenance is Owned. There is
    exactly one owner; transferring an Owned `Str` **moves** it and the
    source becomes **dead**.
- **Aggregates** (structs, arrays) are Copy iff they contain no Owned
  `Str` transitively; otherwise they move as a whole (see §1.3). Structs
  track provenance **per `Str`-typed field**; arrays track **whole-array**
  provenance (per-element liveness is out of scope).

### 1.2 Borrows (implicit, function-local, no syntax)

- **Read borrows**: `rt_str_len`, `rt_str_byte`, `rt_print_str`, and the
  operands of equality/other operators *observe* a value: the binding must
  be live and stays live.
- **Mutate borrow**: `rt_str_set_byte` observes its `Str` argument and
  requires it to be **Owned** — mutating an Immutable (literal) string is
  a compile-time error (E-S11). The binding stays live and owned.
- **Consume**: `rt_str_free` takes the `Str` by value — the binding is
  moved (dead after). Freeing an Immutable string compiles (the runtime
  already rejects it, `E-R05`); the binding is still marked dead.
- Borrows never escape the function; there is no borrow syntax, no
  reference type, and no lifetime annotation this session.

### 1.3 Transfer positions (MOVE if Owned; COPY if Immutable)

1. `let x = e;` / `let mut x = e;` — the source is dead after if Owned;
   `x` takes `e`'s provenance.
2. `x = e;` — the source is dead after if Owned; `x`'s old value is
   dropped (today this leaks, `E-R06` at exit — unchanged); `x` takes
   `e`'s provenance. Assigning to a dead `x` is legal (resurrection).
3. Struct-literal field initializer `P { f: e }` — `e` is dead after if
   Owned; the field takes `e`'s provenance.
4. Array-literal element `[e]` — `e` is dead after if Owned; the array is
   Owned iff any element is Owned.
5. `return e;` — `e` is dead after if Owned; contributes to the
   function's **result provenance**.
6. User-function call argument `f(e)` — `e` is dead after if Owned;
   parameters are Owned on entry.
7. `rt_str_free(e)` — consume (§1.2).
8. Reading a `Str`-typed field in a transfer position (`let n = p.name;`)
   — if the field is Owned it moves; the **field** becomes dead (partial
   move: other fields remain usable); moving the whole struct then
   errors. If the field is Immutable it copies.
9. Reading an array element in a transfer position (`let x = a[i];`) — if
   the array is Owned, the **whole array** moves (conservative; `a`
   becomes dead). Element reads in borrow positions never move.

### 1.4 Result provenance

- A function's result provenance is `Owned` if any `return` value is
  Owned, `Immutable` if every `return` is Immutable, and irrelevant
  otherwise (no `Str` result). Computed by a deterministic fixpoint:
  parameters are Owned, unknown callee result provenance is Owned
  (conservative), and passes iterate until no function changes (the
  lattice is monotone: `Owned → Immutable` as callee information
  improves).

### 1.5 Diagnostics (stable E-S range)

| Code | Kind | Meaning |
| ---- | ---- | ------- |
| E-S10 | `UseOfMovedValue` | reading or moving a dead binding/field (use after move, or moving a whole struct whose field was moved) |
| E-S11 | `MutatingImmutableString` | `rt_str_set_byte` on an Immutable (literal) string |

### 1.6 Pipeline and determinism

- Ownership analysis runs only when semantic and type analysis are clean
  (it needs valid types); its errors gate HIR lowering; errors are
  source-ordered and never panic on any AST. Existing recovery behavior
  is unchanged: lexical/syntax errors suppress all analysis; semantic
  errors suppress ownership; ownership errors suppress HIR/MIR/backend.
- Moves are a compile-time fiction: generated code is unchanged (the
  backend still copies bytes). No runtime or backend changes are
  required this session.

### 1.7 Deliberately absent (later sessions)

- Borrow syntax (`&`, `&mut`), reference types, lifetimes, generics.
- Per-element array liveness; drops/RAII; shared ownership.
- Ownership through pointers (`Ptr` remains Copy/raw).

## 2. Design boundaries

- **One owning type today.** Only `Str` (via `rt_str_alloc`) can own a
  heap resource, so ownership is a closed, testable model. The runtime
  remains the safety backstop: every access is validated (`E-R04/05/07/08/09/10`),
  so even programs that pass ownership analysis but trip a runtime check
  fail with structured errors, never unsafety.
- **Conservative where precise tracking is out of scope:** parameters are
  assumed Owned; arrays move whole; a struct is Copy iff its *values* are
  Immutable (tracked per field). Conservative decisions are documented in
  §4 and can only reject (never accept) unsafe programs.
- **No unsafe Rust.** Everything is safe Rust plus emitted machine code.

## 3. Implementation

- `src/ownership/mod.rs` — the analysis: a deterministic, scope-aware walk
  over items, statements, and expressions maintaining a
  `SymbolId → BindingState` map (`Immutable` / `Owned` / `Dead`; struct
  bindings track per-field states for `Str`-typed fields). It consumes the
  `Ast`, `SemanticResult` (resolution + symbols), and `TypeResult`
  (expression and symbol types, struct field tables) and emits
  `SemanticError`s with the stable codes of §1.5.
- The driver (`src/driver.rs`) runs ownership analysis between type
  checking and HIR lowering; HIR lowering is gated on a clean ownership
  result, so invalid ownership programs fail before code generation.
- `src/semantics/error.rs` — the two new diagnostic categories
  (`UseOfMovedValue` → `E-S10`, `MutatingImmutableString` → `E-S11`),
  rendered through the existing semantic diagnostic machinery.

### 3.1 Intrinsic conventions

| Intrinsic | `Str` argument mode | Result |
| --------- | ------------------- | ------ |
| `rt_str_alloc` | — | `Str` **Owned** |
| `rt_str_free` | consume (move) | `Unit` |
| `rt_str_len` | read borrow | `Int` |
| `rt_str_byte` | read borrow | `Int` |
| `rt_str_set_byte` | **mutate borrow** (requires Owned) | `Unit` |
| `rt_print_str` | read borrow | `Unit` |

## 4. Conservative decisions and known limitations

- **Parameters are Owned.** `fn f(s) { ... }` owns `s`; calling `f(s)` with
  an Owned argument moves it (Rust semantics), so the argument is unusable
  afterwards. Passing an Immutable (literal) argument copies it and the
  source stays usable. Programs that want to reuse an owned string across
  calls must pass literals or restructure (borrow syntax is a later
  session).
- **Arrays move whole.** Reading an element of an Owned array in a
  transfer position moves the entire array; remaining elements leak
  (`E-R06` at exit — the existing leak check catches it).
- **Struct field moves are partial.** Reading an Owned field moves only
  that field; the struct remains usable for its other fields, but the
  whole struct can no longer be moved until the field is reassigned.
- A function that returns a `Str` with mixed provenance (owned and
  literal paths) has an Owned result.
- `const` bindings are evaluated per use: reading a `const` copies its
  value and never moves it (a `const` initialized from an owned source is
  rejected later by the backend, `E-B05`).

## 5. Validation

- `tests/ownership.rs` — valid ownership programs (literals copy freely,
  owned strings move once, borrows leave the source usable, per-field
  moves, resurrection by assignment, result provenance through calls),
  invalid moves (use-after-move in every position, whole-struct move with
  a dead field, element moves), immutable-mutation rejection (E-S11),
  nested scopes, function calls, aggregates, pointers (unaffected),
  recovery (ownership suppressed when earlier stages error), and
  adversarial/deep inputs.
- Native end-to-end tests in `tests/ownership.rs` build and run valid
  ownership programs and verify the exact output and exit codes.
- Full suite after session 16: **878 tests** (see
  `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md` §13 for the
  per-file breakdown); after session 17 (data-free enums, which own
  nothing and are therefore unaffected by move semantics) the suite is
  **919 tests**; after session 18 (pattern matching, whose scrutinee is
  copied per arm and whose bindings are copies, never moves) it is
  **963 tests**.
- **Aggregate copy rule (implemented):** a struct or array is Copy iff it
  contains no Owned value — `P { name: \"a\", age: 1 }` copies freely
  (`let q = p; let r = p;` both fine), while `P { name: rt_str_alloc(3) }`
  moves as a whole. A whole-struct move is rejected (E-S10) if any field
  was already moved out, even when the remaining fields are Immutable.

## 6. What is deliberately absent

- Borrow syntax, reference types, lifetimes, generics, RAII/drops,
  shared ownership — see §1.7.
