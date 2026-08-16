# MINK Pattern Matching Implementation

**Status:** Implemented (Session 18)
**Version:** 0.1.0

This document describes the MINK **pattern matching** foundation: `match`
statements dispatching on scalar values (`match x { 1 => { .. } _ => { .. } }`),
the four pattern forms (integer literals, boolean literals, enum variant
paths, and identifier bindings plus the `_` wildcard), compile-time
exhaustiveness checking, unreachable-arm rejection, and native x86-64
execution. It builds directly on the enum foundation of Session 17
(`docs/implementation/ENUM_TYPES_IMPLEMENTATION.md`) and the type-system
foundation of Sessions 06–07, and deliberately does **not** introduce match
expressions (a `match` producing a value), struct/array destructuring,
ranges or or-patterns, or guards. (Data-carrying variant patterns —
`E::V(x)` — landed in Session 19; see
`docs/implementation/SUM_TYPES_IMPLEMENTATION.md`.)

## 1. Frozen Session 18 rules

### 1.1 Match statements

- A match is a **statement**, not an expression: `match scrutinee {
  pattern => { body }, ... }` evaluates `scrutinee` exactly once and then
  runs the body block of the **first** arm whose pattern matches. `match`
  produces no value.
- The scrutinee is any expression. Its type decides whether the match is
  legal: `Int`, `Bool`, and enums are matchable; every other type (structs,
  arrays, `Str`, pointers, references) is a single `E-T26`
  (invalid match scrutinee) and the arms are not checked, so one root cause
  never cascades into a swarm of arm diagnostics.
- Arms are `pattern => { body }`, separated by commas (a trailing comma is
  allowed, mirroring struct fields and enum variants). The body is a
  block, so braces are required around it.
- `match` arms inherit the enclosing loop/function context: `break`,
  `continue`, and `return` inside an arm behave exactly as they would
  inside any nested block.

### 1.2 Patterns

Four pattern forms, plus the catch-all `_`:

- **Integer literals** — `5` matches an `Int` value equal to `5`;
  `-5` (a leading `-` immediately before the literal) matches `-5`. The
  sign is not part of the literal token, so MIR lowering negates the
  decoded literal when the pattern is negative.
- **Boolean literals** — `true` / `false` match `Bool` values.
- **Enum variant paths** — `E::V` matches the enum value whose
  discriminant is `V`'s position in `E`'s variant list (the same
  `Ident :: Ident` form as the session-17 variant *expression*). The
  pattern's type is the nominal enum type `E`; a path naming a non-enum
  type is `E-T22` (not an enum) and an undeclared variant is `E-T23`
  (unknown variant), exactly as in expressions.
- **Identifier bindings** — `name` matches any value and **binds** it
  (immutably) in the arm's body scope: `match e { x => { rt_print_int(x); } }`
  behaves like `let x = e;`. The binding is a copy of the scrutinee (all
  matchable types are scalars, so binding is copy semantics, never a move).
- **Wildcard** — `_` matches any value and binds nothing. A binding arm
  and a wildcard arm are both **catch-alls**: once one appears, every
  later arm is unreachable.

A non-pattern token where a pattern is required (e.g. a string literal) is
`E-P23` (expected a pattern); a pattern not followed by `=>` is `E-P24`
(expected `=>`). `_` is recognized by spelling in the parser (it lexes as
an identifier) and never binds a name.

### 1.3 Exhaustiveness and unreachable arms

The type checker requires every match to be **exhaustive** and rejects
**unreachable** arms, deterministically, at compile time:

- **Enums.** A match with no catch-all must cover every variant of the
  scrutinee's enum; a missing variant is `E-T24` (non-exhaustive match),
  listing the uncovered variant(s). Covering every variant exactly once is
  exhaustive without a catch-all. An **empty enum** (`enum Empty {}`)
  match with no arms is vacuously exhaustive — there are no values.
- **`Bool`.** Without a catch-all, both `true` and `false` must be
  covered (`E-T24` otherwise).
- **`Int`.** Integer values cannot all be listed, so an `Int` match
  without a catch-all is always `E-T24`; `_` or a binding arm is the way
  to write a complete `Int` match.
- **Unreachable arms.** An arm after a catch-all (`_` or a binding) can
  never run (`E-T25`); so can a duplicate pattern an earlier arm already
  matches (`match x { 1 => {..} 1 => {..} }` — the second `1` is `E-T25`).
  The type checker's coverage set is per-pattern-value, so
  `match x { 1 => {..} _ => {..} 2 => {..} }` reports the `2` arm as
  unreachable even though `2` was not listed before the catch-all.

Pattern/scrutinee type mismatches are ordinary `E-T01` mismatches with the
exact pattern span (a `Bool` pattern on an `Int` scrutinee, an `E::A`
variant pattern on `F::B`'s enum, etc.). An unresolved (inference) scrutinee
is pinned by its first refutable pattern: `match e { Direction::North => {..} }`
gives `e` the type `Direction`.

### 1.4 MIR lowering

A match lowers to a **chain of equality branches** over the scrutinee
value, evaluated exactly once:

```
test0:   cond = scrutinee == <arm0 const>   (refutable arms only)
         branch cond → arm0, test1
...
testn:   cond = scrutinee == <armn const>
         branch cond → armn, unreachable
armk:    [binding copy] <arm body>          (jumps to `after`)
unreachable: return                         (never reached)
after:   ...
```

- Irrefutable arms (`_`, `name`) skip the test and jump straight into
  their body; a binding pattern copies the scrutinee value into a fresh
  local first (a `Use` of the scrutinee operand into the binding's local).
- Enum patterns compare the scrutinee against the compiler-computed
  **discriminant constant** (`MirConstantKind::Enum { variant }` from
  session 17); integer patterns compare against the decoded literal
  (negated via a unary `Neg` rvalue when the pattern is negative); bool
  patterns against `MirConstantKind::Bool`.
- The defensive `unreachable` block exists only when the last arm is
  refutable: its else path is provably impossible (the type checker
  guarantees exhaustiveness, `E-T24`), so it is never executed. It ends
  in a bare `return` so the graph stays structurally valid.
- Because a match is a statement with block bodies, the lowered graph is
  ordinary CFG shape: the optimizer's existing passes (constant folding,
  copy propagation, CFG simplification, unreachable-block elimination,
  dead-code elimination) apply unchanged — every arm body is a normal
  block, and duplicate constants fold identically to hand-written
  `if (e == 1) { .. }` chains.

## 2. Design boundaries

- **Matching is a statement.** Arms are statement blocks like `if`
  bodies; `match` yields no value, so there is no `match` expression, no
  type for a match result, and no match-in-expression syntax this session.
- **Scalar scrutinees only.** `Int`, `Bool`, and enums are the matchable
  types — the types whose values are a single word. Structs, arrays,
  strings, pointers, and references are rejected (`E-T26`), and
  destructuring is future work.
- **Patterns are refutable values.** A pattern names a *value* (a literal,
  a discriminant), not a shape; exhaustiveness is exact for enums and
  `Bool` because their value sets are closed and finite.
- **Bindings copy.** An identifier pattern binds a copy of the scrutinee —
  the natural rule when every matchable type is a scalar that copies
  freely. Ownership analysis treats the binding as a copy (no move, no
  ownership state), so a bound name may be used freely and the scrutinee
  remains usable after the match.
- **No unsafe Rust.** Everything is safe Rust plus emitted machine code.
- **Deterministic.** Identical sources produce identical diagnostics,
  identical MIR, and byte-identical images.

## 3. Implementation architecture

- `src/ast/mod.rs` — `StmtKind::Match(MatchStmt)`, `MatchStmt { scrutinee,
  arms, span }`, `MatchArm { pattern, body, span }`, and `Pattern` (the
  `Wildcard`/`Binding`/`EnumVariant`/`Bool`/`Int` enum). Literal values
  are not decoded into the tree: an integer pattern keeps its `ExprKind::Int`
  node (span over the digits) plus a `negative` flag, matching the
  expression convention that the backend decodes literal text.
- `src/parser/mod.rs` — `parse_match` (scrutinee under `in_block_context`
  so `Ident {` in a scrutinee is the arm block, never a struct literal;
  arm loop with comma/trailing-comma/brace termination and
  `skip_to_arm_boundary` recovery), `parse_match_arm`, and `parse_pattern`
  (the four forms plus `_` by spelling and `E::V` reusing the `E-P22`
  expected-variant rule). `parse_statement` dispatches on the `Match`
  token.
- `src/parser/error.rs` — `ParseErrorKind::ExpectedPattern` (`E-P23`),
  `ExpectedFatArrow` (`E-P24`).
- `src/semantics/analyzer.rs` — `analyze_match`: the scrutinee is analyzed
  in the enclosing scope; each arm's body runs in its own block scope that
  additionally declares the arm's pattern binding (immutable, like a
  `let`). Arms inherit the enclosing loop/function context, so
  `break`/`continue`/`return` resolve as in any nested block.
- `src/typecheck/checker.rs` — match statements are checked after
  declarations (a deferred pass), so the scrutinee's type — and every
  function call in it — is resolved before exhaustiveness runs. Per-arm
  pattern checking (`E-T01` mismatches, `E-T22`/`E-T23` variant paths,
  binding declarations unified with the scrutinee type), coverage
  recording with duplicate rejection (`E-T25`), catch-all tracking, and
  the final exhaustiveness check (`E-T24`) for enum/`Bool`/`Int`/empty
  scrutinees. Unresolved scrutinees are pinned by their patterns.
- `src/typecheck/error.rs` — `TypeErrorKind::NonExhaustiveMatch`
  (`E-T24`), `UnreachableMatchArm` (`E-T25`), `InvalidMatchScrutinee`
  (`E-T26`).
- `src/hir/mod.rs`, `src/hir/lower.rs` — `HirStmtKind::Match(HirMatch)`,
  `HirMatch`, `HirMatchArm`, and `HirPattern` (mirroring the AST; the
  binding carries its resolved `HirIdent`, and the enum variant carries
  `HirName`s). The scrutinee is lowered as a normal expression.
- `src/mir/mod.rs`, `src/mir/lower.rs` — `lower_match` (branch chain,
  binding copies, defensive `unreachable` return) and `pattern_test`
(equality comparison against the pattern's constant, with a unary `Neg`
rvalue for negative literals). `validate` and `optimize` treat the match
lowering as ordinary CFG — no new validation rules were needed.
- `src/ownership/mod.rs` — `walk_match`: the scrutinee is observed
  (matchable types are scalars that never move), each arm body is walked
  as its own block scope, and a pattern binding is registered as a copy.
  No ownership rules apply to `match` itself.
- `src/backend/` — no changes: a lowered match is ordinary branches,
  word equality, constants, and copies, all already supported. (The
  backend tests confirm match-lowered programs lower through the backend
  and emit byte-identical images.)

## 4. Conservative decisions and known limitations

- **No match expressions.** `match` is a statement; `let x = match e { .. };`
  is a parse error. Value-producing matches are future work.
- **No destructuring.** Patterns match scalar values only — no struct
  patterns, array patterns, or tuple forms; no nested patterns.
- **No ranges, or-patterns, or guards.** `1 | 2 =>`, `1..=3 =>`, and
  `x if cond =>` are all rejected (the first is a parse error; ranges and
  guards have no syntax this session).
- **Bindings are whole-value copies.** There is no `name @`-style
  sub-pattern binding. (Session 18 matched payload-free enums only;
  Session 19 added payload patterns `E::V(x)` with payload extraction and
  recursive sub-coverage — see
  `docs/implementation/SUM_TYPES_IMPLEMENTATION.md`.)
- **No match on references.** `match r` where `r: &Int` is `E-T26`;
  matching through a deref (`match *r`) works and matches the pointed-to
  scalar.

## 5. Examples

```mink
enum Direction { North, South, East, West }

fn describe(d) {
    // Enum match: every variant covered, no catch-all needed.
    match d {
        Direction::North => { rt_print_str("north\n"); }
        Direction::South => { rt_print_str("south\n"); }
        Direction::East  => { rt_print_str("east\n"); }
        Direction::West  => { rt_print_str("west\n"); }
    }
    return 0;
}

fn main() {
    let d = Direction::East;
    describe(d);

    // Int match with a binding catch-all; the binding copies the value.
    let n = 42;
    match n {
        0 => { rt_print_int(0); }
        42 => { rt_print_int(42); }
        other => { rt_print_int(other); }   // prints 42 (first match wins)
    }

    // Bool match covering both values, and a negative literal.
    let flag = false;
    match flag {
        true => { rt_print_int(1); }
        false => { rt_print_int(0); }
    }
    let t = -5;
    match t {
        -5 => { rt_print_int(55); }
        _ => { rt_print_int(0); }
    }
    return;
}
```

A missing arm is rejected at compile time:

```mink
fn main() {
    let d = Direction::East;
    match d {
        Direction::North => { rt_print_int(1); }
        Direction::South => { rt_print_int(2); }
        // E-T24: `East` and `West` are not covered (no `_` arm).
    }
    return;
}
```

## 6. Validation

- `tests/pattern_matching.rs` (44 tests) — parser (match statements,
  trailing commas, comma-less arms, all four pattern forms, structured
  `E-P23`/`E-P24` errors, recovery to the next arm); semantics (pattern
  binding resolution and shadowing, immutability of bindings,
  `break`/`continue` inside arms reaching the enclosing loop); type
  checking (scrutinee type recording, enum/int patterns pinning unresolved
  scrutinees, `E-T01` pattern mismatches, `E-T22`/`E-T23` variant paths,
  `E-T26` non-matchable scrutinees, `E-T24` exhaustiveness for
  enum/`Bool`/`Int`/empty-enum cases, `E-T25` unreachable arms after
  catch-alls and duplicates, binding type = scrutinee type); HIR lowering
  (`HirStmtKind::Match`, resolved binding identifiers); MIR lowering
  (enum-discriminant comparisons, integer equality, binding copies);
  backend lowering and determinism (identical sources → identical MIR,
  byte-identical images); and native end-to-end programs (all-variant enum
  matches, catch-all enum matches, first-match-wins semantics, negative
  integer patterns, bool matches, binding copies, matches through struct
  members, matches inside loops, nested matches, exit codes from int
  matches, and a many-arm match staying word-sized) with exact output and
  exit codes.
- `tests/parser.rs`, `tests/semantics.rs`, `tests/parser_hardening.rs` —
  the structural walkers were extended to cover `StmtKind::Match` and
  patterns (spans, bindings), and the former "`match` is rejected"
  regression was updated: `match` is now a supported construct (its
  malformed forms still error with `E-P23`/`E-P24`).
- Existing suite (919 tests) remained green; full suite after session 18:
  **963 tests** (+44 in `tests/pattern_matching.rs`), all passing (see
  `NATIVE_BACKEND_IMPLEMENTATION.md` §13 for the per-file breakdown).

## 7. Status

Frozen for the constructs it covers. Statements and declarations outside
it are rejected with stable diagnostics. Later sessions extend this
document additively.
