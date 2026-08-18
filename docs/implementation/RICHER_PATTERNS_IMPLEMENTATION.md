# MINK Richer Match Patterns Implementation

**Status:** Implemented (Session 27)
**Version:** 0.1.0

This document describes the MINK **richer match patterns**: or-patterns
(`1 | 2 | 3`, `E::A(x) | E::B(x)`), integer range patterns (`1..=5`,
`1..5`, with negated endpoints), and guarded arms (`pat if expr =>`). It
extends the pattern-matching foundation of Session 18
(`docs/implementation/PATTERN_MATCHING_IMPLEMENTATION.md`) and the
data-carrying variants of Session 19
(`docs/implementation/SUM_TYPES_IMPLEMENTATION.md`) additively: every
session-18/19 program compiles and behaves exactly as before.

## 1. Language rules

### 1.1 Or-patterns

- `|` combines alternatives into an or-pattern: `1 | 2 | 3`,
  `E::A | E::B`, `E::A(x) | E::B(x)`, and mixed alternatives like
  `1 | 2..=5`. The pattern matches when any alternative matches.
- Every alternative must bind the **same set of names with the same
  types**; a mismatch is `E-T34` (invalid or-pattern). The alternatives
  share **one binding per name** — the arm body sees a single binding, and
  type inference resolves every occurrence of the name to that one
  declaration.
- Alternatives may be ranges (`1..=5 | 7..=9`) but an or-pattern
  alternative may not itself be an or-pattern (the parser flattens `a | b
  | c` into one alternatives list; `|` cannot nest).
- The `|` token cannot follow a pattern in any other position, so
  `match x { 1 | 2 => { .. } }` needs no parentheses.

### 1.2 Range patterns

- `lo..=hi` is an **inclusive** range (`1..=5` matches 1, 2, 3, 4, 5);
  `lo..hi` is an **exclusive** range (`1..5` matches 1, 2, 3, 4). Both
  endpoints are integer literals, optionally negated (`-3..=2`,
  `-10..-1`). A non-literal endpoint or a chained range (`1..2..3`) is
  `E-P19` (expected an integer literal).
- A range pattern is well-typed only on an `Int` scrutinee; a range on
  any other matchable type is a type mismatch (`E-T01`).
- Coverage is tracked as integer **intervals**: a match of `1..=5` then
  `6..=10` then `11..=20` then `_` is exhaustive, and an uncovered gap is
  reported precisely by `E-T25` (e.g. `match x { 1..=5 => { .. } }`
  reports that the points below 1 and above 5 are uncovered). A range
  that covers nothing (`5..1`, or the exclusive `1..1`) contributes no
  coverage and is reported as unreachable (`E-T25`) when nothing else
  covers its space.

### 1.3 Guards

- A guarded arm is `pattern if expr => { body }`. The guard is evaluated
  **after** the pattern matches and **before** the body, with the
  pattern's bindings in scope: `match x { n if n > 3 => { .. } }` reads
  `n` in the guard.
- The guard must be a `Bool` expression (`E-T09` on mismatch).
- A guarded arm **commits no coverage**: it can still fail, so it never
  makes the match exhaustive and never makes later arms unreachable.
  `match x { 1 if c => { .. }, _ => { .. } }` still requires the `_`
  arm. An arm whose *pattern* an earlier arm already fully covers is
  unreachable (`E-T25`) even when guarded, because the pattern test alone
  decides reachability.

## 2. Parser

- `parse_pattern` parses a pattern atom, then loops on `|` to collect
  or-pattern alternatives (`Pattern::Or { alternatives, span }`). Each
  alternative goes through `parse_pattern_atom`, so `E::A(x) | E::B(x)`
  works and `1 | 2..=5` works (the range is parsed by
  `parse_range_pattern` when `..`/`..=` follows an integer-literal
  pattern).
- `parse_range_pattern` requires the `lo` endpoint to be an integer
  literal (`E-P19` otherwise), consumes `..` (exclusive) or `..=`
  (inclusive), parses the `hi` endpoint (also `E-P19` on a non-literal),
  and rejects a chained range. The result is
  `Pattern::Range { lo, hi, inclusive, span }`.
- `parse_match_arm` parses an optional `if Expr` between the pattern and
  `=>`, producing the arm's `guard: Option<Expr>` field.
- New parse errors: `E-P19` now also covers range endpoints; a `|` or
  range used where the grammar disallows it surfaces as the existing
  pattern errors (e.g. `E-P23` for a non-pattern token).

## 3. Semantic analysis

- Or-pattern alternatives must bind the same names. The analyzer records
  **binding aliases**: every occurrence of a shared name in the later
  alternatives resolves to the first alternative's declaration (one
  declaration per name, so scope release and resolution behave like a
  single `let`).
- The analyzer walks guards after the pattern bindings, so guard
  references resolve in the arm scope (a guard can read the pattern's
  bindings; a guard cannot see the arm body's locals).

## 4. Type checking

- **Or-patterns**: each alternative is checked against the scrutinee
  type; a name bound with different types across alternatives is `E-T34`.
  All alternatives' bindings unify into the shared declaration, and the
  arm's coverage is the union of the alternatives' coverage (an unguarded
  or-pattern arm commits that union).
- **Ranges**: the endpoints unify with `Int`; coverage is an interval
  `[lo, hi]` (`hi` decremented by one for exclusive ranges, with an empty
  result when the decrement underflows `i64::MIN`). Interval coverage is
  kept sorted and disjoint; exhaustiveness for `Int` is full-domain
  coverage `[i64::MIN, i64::MAX]` (a match of `1..=5` + `6..=10` alone
  reports the uncovered space). A range pattern on a non-`Int` scrutinee
  is `E-T01`.
- **Guards**: checked as a condition (`E-T09` on a non-`Bool`); guarded
  arms are excluded from the coverage merge and from the exhaustive
  check, and a guarded arm whose pattern an earlier arm already covers is
  `E-T25` unreachable.
- **Coverage machinery**: `Coverage` holds a sorted disjoint interval
  list plus an `all` flag (a catch-all pattern). Points, ranges,
  or-alternatives, and variant coverage all merge into it. The deferred
  re-type pass re-checks guards too (guards are conditions, resolved like
  `if` conditions).
- New error codes: `E-T34` (invalid or-pattern — alternatives bind
  different names or types).

## 5. HIR

- `HirPattern` gains `Or { alternatives, span }` and
  `Range { lo, hi, inclusive, span }`; `HirMatchArm` gains
  `guard: Option<HirExpr>`. Lowering maps the AST forms directly.

## 6. MIR

- `lower_match` lowers each arm as a test chain. A binding pattern
  assigns the scrutinee value to the binding's local (one local per name,
  shared across or-pattern alternatives via the symbol → local mapping)
  and jumps into the arm; refutable patterns (`int`, `bool`, `variant`,
  `range`, `or`) lower a test with a branch into the arm and an `else`
  block to the next arm's tests.
- A range test is two comparisons: `v >= lo && v <= hi` (inclusive) or
  `v >= lo && v < hi` (exclusive), short-circuited through a two-branch
  chain.
- An or-pattern lowers as a chain of the alternatives' tests, all
  branching into the same arm body.
- A guarded arm allocates a guard block: after the pattern tests branch
  into it, the guard expression is evaluated and `Branch`-ed to the body
  (true) or to the next arm's tests (false). Every guarded arm allocates
  an `else` block, so a guarded irrefutable arm still falls through on a
  failing guard. The final arm's `else` (provably unreachable by the
  checker) terminates in `Return { value: None }`.
- The MIR validator accepts the new terminator shapes unchanged.

## 7. Ownership

- `walk_match` treats an or-pattern alternative that binds an owned
  payload exactly like a plain payload pattern: the payload moves out of
  the scrutinee, and after the match the scrutinee's payload is consumed
  (a later use is `E-S10`). An or-pattern binding with copy provenance
  (`Int`, `Bool`, unit enums) leaves the scrutinee usable.
- Guards are walked in `Observe` mode after the pattern bindings: a guard
  reads the bindings without moving them (an owned payload binding used
  in a guard is a read, like any binding use).

## 8. Backend

No new backend instructions: ranges lower to the existing `Ge`/`Le`/`Lt`
int comparisons, or-patterns reuse the existing test/branch shapes, and
guards reuse the existing condition-branch shape. Builds remain
byte-identical across runs (determinism holds; the richer-pattern tests
pin byte-for-byte image equality for a guarded/or/range program).

## 9. Diagnostics

- `E-T34` — invalid or-pattern: `match x { E::A(a) | E::B(b) => { .. } }`
  binds different names; `E::A(x) | 5 => { .. }` binds a name in one
  alternative only.
- `E-P19` — expected an integer literal: a range endpoint that is not an
  integer literal (`1..x`, `1..2..3`).
- `E-T25` — non-exhaustive/unreachable match, now with interval
  precision: uncovered gaps between range patterns are reported; a
  guarded arm leaves its space uncovered; an arm covered by an earlier
  arm is unreachable.
- `E-T09` — a non-`Bool` guard.

## 10. Test coverage

`tests/richer_patterns.rs` covers: parsing (all or/range/guard forms and
rejections), semantics (shared or-pattern bindings, guard scope),
type checking (interval exhaustiveness, `Int` full-domain coverage,
unreachable arms across points/ranges/or-alternatives, `E-T34` binding
consistency, guarded-arm coverage), HIR/MIR lowering (branch chains,
`Ge`/`Le`/`Lt` range tests, one local per or-pattern binding, guard
blocks), backend lowering and byte-identical determinism, ownership
(owned payload moves through or-patterns and guards; copy payloads stay
usable), and native execution (runtime semantics of or-patterns, ranges,
guards, and their interactions).
