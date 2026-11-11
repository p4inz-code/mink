# MINK ownership and borrowing implementation

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

- Copy types (always copy, never move): `Int`, `Float`, `Bool`,
  `Char`, `Range<T>`, `Unit`, and `Ptr<T>`. Raw pointers are Copy by
  design: every access is validated by the runtime (liveness table,
  alignment, bounds), so aliasing a pointer is safe.
- `Str` values are one of two provenances:
  - Immutable: a string literal: immortal image data. Copying is
    always harmless; copying an Immutable `Str` keeps the source usable.
  - Owned: a heap blob (from `rt_str_alloc`), a function parameter,
    or the result of a call whose result provenance is Owned. There is
    exactly one owner; transferring an Owned `Str` **moves** it and the
    source becomes **dead**.
- Aggregates (structs, arrays) are Copy iff they contain no Owned
  `Str` transitively; otherwise they move as a whole (see §1.3). Structs
  track provenance **per `Str`-typed field**; arrays track whole-array
  provenance (per-element liveness is out of scope).

### 1.2 Borrows (implicit, function-local, no syntax)

- Read borrows: `rt_str_len`, `rt_str_byte`, `rt_print_str`, and the
  operands of equality/other operators *observe* a value: the binding must
  be live and stays live.
- Mutate borrow: `rt_str_set_byte` observes its `Str` argument and
  requires it to be **Owned**, mutating an Immutable (literal) string is
  a compile-time error (E-S11). The binding stays live and owned.
- Consume: `rt_str_free` takes the `Str` by value. The binding is
  moved (dead after). Freeing an Immutable string compiles (the runtime
  already rejects it, `E-R05`); the binding is still marked dead.
- Borrows never escape the function; there is no borrow syntax, no
  reference type, and no lifetime annotation this session.

### 1.3 Transfer positions (MOVE if Owned; COPY if Immutable)

1. `let x = e;` / `let mut x = e;`: the source is dead after if Owned;
   `x` takes `e`'s provenance.
2. `x = e;`: the source is dead after if Owned; `x`'s old value is
   dropped (today this leaks, `E-R06` at exit, unchanged); `x` takes
   `e`'s provenance. Assigning to a dead `x` is legal (resurrection).
3. Struct-literal field initializer `P { f: e }`: `e` is dead after if
   Owned; the field takes `e`'s provenance.
4. Array-literal element `[e]`: `e` is dead after if Owned; the array is
   Owned iff any element is Owned.
5. `return e;`: `e` is dead after if Owned; contributes to the
   function's **result provenance**.
6. User-function call argument `f(e)`: `e` is dead after if Owned;
   parameters are Owned on entry.
7. `rt_str_free(e)`: consume (§1.2).
8. Reading a `Str`-typed field in a transfer position (`let n = p.name;`)
   — if the field is Owned it moves; the **field** becomes dead (partial
   move: other fields remain usable); moving the whole struct then
   errors. If the field is Immutable it copies.
9. Reading an array element in a transfer position (`let x = a[i];`). If
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

- One owning type today. Only `Str` (via `rt_str_alloc`) can own a
  heap resource, so ownership is a closed, testable model. The runtime
  remains the safety backstop: every access is validated (`E-R04/05/07/08/09/10`),
  so even programs that pass ownership analysis but trip a runtime check
  fail with structured errors, never unsafety.
- Conservative where precise tracking is out of scope: parameters are
  assumed Owned; arrays move whole; a struct is Copy iff its *values* are
  Immutable (tracked per field). Conservative decisions are documented in
  §4 and can only reject (never accept) unsafe programs.
- No unsafe Rust. Everything is safe Rust plus emitted machine code.

## 3. Implementation

- `src/ownership/mod.rs`: the analysis: a deterministic, scope-aware walk
  over items, statements, and expressions maintaining a
  `SymbolId → BindingState` map (`Immutable` / `Owned` / `Dead`; struct
  bindings track per-field states for `Str`-typed fields). It consumes the
  `Ast`, `SemanticResult` (resolution + symbols), and `TypeResult`
  (expression and symbol types, struct field tables) and emits
  `SemanticError`s with the stable codes of §1.5.
- The driver (`src/driver.rs`) runs ownership analysis between type
  checking and HIR lowering; HIR lowering is gated on a clean ownership
  result, so invalid ownership programs fail before code generation.
- `src/semantics/error.rs`: the two new diagnostic categories
  (`UseOfMovedValue` → `E-S10`, `MutatingImmutableString` → `E-S11`),
  rendered through the existing semantic diagnostic machinery.

### 3.2 Control flow (Session 126: branch-local + conservative join)

- The walk is **branch-local**. `walk_block`, `walk_if`, `walk_if_expr` and
  `walk_match` return a `bool` ("control flow terminates on all paths"),
  `walk_stmt` returns `Option<bool>` and stops walking after a `return`, and
  `snapshot_flow` / `restore_flow` / `join_flow` (with `join_binding_state`)
  capture a branch's state, restore the pre-branch state before the next arm,
  and merge the arms with a conservative "may have moved" join afterwards.
- Consequences: `rt_str_free(x)` in one arm no longer poisons a sibling arm
  (`E-S10` false positives removed), while a move on *every* arm is still
  reported after the `if`, and a loop body that frees and then uses a binding
  is still reported. Regression tests: the 6 `branch_*`/`match_arms_do_not_*`
  cases in `tests/ownership.rs` (49/49 pass).
- Still **not path-sensitive**: state is not tracked per condition
  (`if flag { free(x) }` followed by `x` is not reported), so the single-exit
  idiom (release each owned string exactly once, on the way to the one exit)
  remains the documented safe style.

### 3.1 Intrinsic conventions

| Intrinsic | `Str` argument mode | Result |
| --------- | ------------------- | ------ |
| `rt_str_alloc` | — | `Str` Owned |
| `rt_str_free` | consume (move) | `Unit` |
| `rt_str_len` | read borrow | `Int` |
| `rt_str_byte` | read borrow | `Int` |
| `rt_str_set_byte` | mutate borrow (requires Owned) | `Unit` |
| `rt_print_str` | read borrow | `Unit` |
| `rt_str_concat` | read borrow (both) | `Str` Owned |
| `rt_str_eq` | read borrow (both) | `Bool` |
| `rt_str_from_int` | — | `Str` Owned |
| `rt_str_from_bool` | — | `Str` Owned |

## 4. Conservative decisions and known limitations

- Parameters are Owned. `fn f(s) { ... }` owns `s`; calling `f(s)` with
  an Owned argument moves it (Rust semantics), so the argument is unusable
  afterwards. Passing an Immutable (literal) argument copies it and the
  source stays usable. Programs that want to reuse an owned string across
  calls must pass literals or restructure (borrow syntax is a later
  session).
- Arrays and tuples are tracked **per element** (Session 273). Each element
  of an array or tuple has its own live/moved state, so `rt_str_free(a[0])`
  moves element 0 only and `a[1]` stays usable. A whole-value transfer moves
  every element; a whole-value transfer of a *partially* moved aggregate is
  rejected (`E-S10`, naming the element), because the moved element would be
  duplicated and freed twice. An element index that is a runtime value
  (rather than a literal) is conservative: reading through it is rejected
  once any element has been moved, and a transfer through it moves the whole
  aggregate. A nested place such as `h.a[0]` (an array inside a struct
  field) has no per-element state, so a transfer through it moves the whole
  containing value — conservative and sound.
  (Audit hardening, Session 21: the element-read path used to push the E-S10
  twice, once from evaluating the dead base and once from the index
  handler's own dead-branch, for a duplicated diagnostic; the redundant
  push was removed.)
- Struct field moves are partial. Reading an Owned field moves only
  that field; the struct remains usable for its other fields, but the
  whole struct can no longer be moved until the field is reassigned.
  Tuple elements and array elements follow the same rule with their own
  per-element state (Session 273), including through destructuring:
  `let (a, b) = t;` moves both elements into the new bindings and leaves
  `t` unusable, while a tuple of copy values still copies.
- Match scrutinees move at the same granularity as transfers (audit
  alignment, Session 21): `match s.e { ... }` where the payload is owned
  consumes only the field `s.e` (the rest of `s` stays usable), matching
  the `let e = s.e;` partial-field-move rule; previously the whole root
  was marked dead. Unit-only enums (no payloads) remain copy scrutinees.
- A function that returns a `Str` with mixed provenance (owned and
  literal paths) has an Owned result.
- **Branch-local, still not path-sensitive (Session 126).** See §3.2: arms no
  longer see each other's moves and the join is conservative, but flag-guarded
  frees are not tracked; `validation/*/record.md` all choose the single-exit
  idiom for that reason.
- `const` bindings are evaluated per use: reading a `const` copies its
  value and never moves it (a `const` initialized from an owned source is
  rejected later by the backend, `E-B05`).

## 5. Validation

- `tests/ownership.rs`: valid ownership programs (literals copy freely,
  owned strings move once, borrows leave the source usable, per-field
  moves, resurrection by assignment, result provenance through calls),
  invalid moves (use-after-move in every position, whole-struct move with
  a dead field, element moves), immutable-mutation rejection (E-S11),
  nested scopes, function calls, aggregates, pointers (unaffected),
  recovery (ownership suppressed when earlier stages error), and
  adversarial/deep inputs.
- Native end-to-end tests in `tests/ownership.rs` build and run valid
  ownership programs and verify the exact output and exit codes.
- Full suite after session 16: 878 tests (see
  `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md` §13 for the
  per-file breakdown); after session 17 (data-free enums, which own
  nothing and are therefore unaffected by move semantics) the suite is
  **919 tests**; after session 18 (pattern matching, whose scrutinee is
  copied per arm and whose bindings are copies, never moves) it is
  **963 tests**; after the Session 21 deep audit (duplicate E-S10 removal
  and match-scrutinee partial-move alignment, +2 in `tests/ownership.rs`)
  the suite is **1048 tests**.
- Aggregate copy rule (implemented): a struct or array is Copy iff it
  contains no Owned value, `P { name: \"a\", age: 1 }` copies freely
  (`let q = p; let r = p;` both fine), while `P { name: rt_str_alloc(3) }`
  moves as a whole. A whole-struct move is rejected (E-S10) if any field
  was already moved out, even when the remaining fields are Immutable.

## 5.1 Element-level aggregate state (Session 273)

Representation: `State::Array(AggregateState)` and `State::Tuple(AggregateState)`,
where `AggregateState { elems: Vec<BindingState>, dead: bool }`. `dead` means the
whole value was moved; an element state of `Dead` means that element was moved
out. A whole move sets every element state to `Dead`, so `dead` is a summary
rather than a substitute for the per-element states.

Rules:

- **Construction.** A tuple or array literal evaluates each element in transfer
  position and records the resulting per-element states on the value
  (`EvalValue::elems`), so ownership follows the literal into the binding. A
  call result or a nested place has no element states; the type supplies
  all-live states (Owned for element types that may own, Immutable otherwise).
- **Element transfer.** `t.1` / `a[2]` in transfer position moves only that
  element, after checking that the aggregate is not borrowed (`E-S12`).
  Reading a moved element is `E-S10` and names the element and its index.
- **Whole transfer.** `let u = t;`, `return t`, passing `t` to a function and
  `defer move (t)` move every element; a partially moved aggregate is rejected
  with a diagnostic naming the first moved element.
- **Observation.** Reading an aggregate without transferring it (an intrinsic
  argument, a condition) copies the value and kills nothing.
- **Joins.** `AggregateState::joined` merges element-wise with dead-wins, and a
  whole move in either arm makes the aggregate dead, so a conditional element
  move is visible after the `if` and a loop's back edge reports a repeated
  element move.
- **Reassignment.** Assigning a whole aggregate rebuilds its element states
  from the new value; assigning one element (`a[0] = ...`) makes that element
  live again, and a runtime index makes every element conservatively live.
- **Copy aggregates.** A tuple or array whose elements cannot own is not
  tracked at all (`Shape::Copy`), so it keeps copying freely.

Destructuring is the same machinery: a `let` tuple pattern binds each element
from the value's element state (so owned elements move and the source dies), and
matching a tuple reads the scrutinee's element states — reachable only if the
type checker ever allows aggregate patterns (today it rejects them, `E-T26`).

Nested aggregates remain limited by the type system: an aggregate of aggregates
by value is rejected as an overflowing size (`E-T18`), so the only nested case is
an array inside a struct, which is handled conservatively (see §4).

## 6. What is deliberately absent

- Borrow syntax, reference types, lifetimes, generics, RAII/drops,
  shared ownership, see §1.7.
