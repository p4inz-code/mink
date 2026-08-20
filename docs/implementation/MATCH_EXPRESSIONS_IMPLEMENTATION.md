# MINK — Match Expressions Implementation

**Status:** Implemented (Session 33)
**Version:** 0.1.0

This document describes the MINK **match expression**: `match scrutinee {
pattern => expr, ... }` dispatching on a scalar value and producing a
result. It extends the pattern-matching foundation of Session 18
(`PATTERN_MATCHING_IMPLEMENTATION.md`) and the richer patterns of Session
27 (`RICHER_PATTERNS_IMPLEMENTATION.md`) additively: every session-18/27
program compiles and behaves exactly as before.

## 1. Language rules

### 1.1 Match expressions

- A `match` expression evaluates `scrutinee` once, then evaluates and
  returns the value of the first arm whose pattern matches. Every arm's
  expression must produce a compatible result type; mismatched types are
  `E-T01`.
- The scrutinee may be any expression. Its type decides whether the match
  is legal: `Int`, `Bool`, and enums are matchable; every other type is
  `E-T26`.
- Arms are `pattern => expr`, separated by commas (trailing comma
  allowed). The body is an expression, so `=> 1` works directly and
  `=> { stmt; expr }` works through block expressions.
- A guard (`pattern if expr => expr`) is evaluated after the pattern
  matches and before the body expression. A guarded arm does not commit
  its pattern's coverage.
- The match must be exhaustive: a missing variant or uncovered space is
  `E-T24`; an unreachable arm is `E-T25`.
- Match expressions appear in binding position (`let x = match ...`),
  return position (`return match ...`), expression statement position,
  block trailing position, and as operands to other expressions. They do
  **not** appear in arbitrary operand position (function arguments, binary
  operands) — this mirrors the existing `if`-expression limitation.

### 1.2 Type semantics

- A fresh inference variable is the match expression's result type.
- Each arm's expression type is unified with this variable; a mismatch is
  `E-T01`.
- Exhaustiveness is checked after all arms have pinned the scrutinee type
  (same rules as the match statement).

### 1.3 Backward compatibility

- The match **statement** (`match e { pat => { block }, ... }`) is
  unchanged: arms are statement blocks, the expression produces no value.
- The match **expression** (`match e { pat => expr, ... }`) is new: arms
  are expressions that produce the result value.
- Existing match-statement programs compile and behave identically.

## 2. AST

New types in `src/ast/mod.rs`:

```rust
pub struct MatchExpr {
    pub scrutinee: Expr,
    pub arms: Vec<MatchExprArm>,
    pub span: Span,
}

pub struct MatchExprArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}
```

New variant in `ExprKind`:

```rust
MatchExpr(Box<MatchExpr>),
```

## 3. Parser

- `parse_match_expr()` in the primary expression parser handles
  `match scrutinee { pattern => expr, ... }`.
- `parse_match_expr_arm()` parses `pattern ('if' Expr)? => Expr` where
  the body is an expression (not a block).
- `TokenKind::Match` is added to the `is_expr_trailing` set in
  `parse_block_expr()` so match expressions work as trailing expressions
  in block expressions.
- Recovery uses the existing `skip_to_arm_boundary()` infrastructure.

## 4. Semantic analysis

- `ExprKind::MatchExpr(m)` in `analyze_expr()`: the scrutinee is
  analyzed in the enclosing scope; each arm creates a block scope with
  pattern bindings; the arm body is analyzed as an expression.

## 5. Type checking

- `check_match_expr()` types the scrutinee, checks patterns against it
  (reusing `check_arm_pattern()` and `check_or_arm()`), checks guards,
  types each arm's expression, and unifies all arm types with a fresh
  inference variable. Exhaustiveness is checked using the same
  `Coverage` machinery as the match statement.
- `resolve_deferred_expr()` handles deferred re-typing of match
  expressions.

## 6. HIR

New variant in `HirExprKind`:

```rust
MatchExpr {
    scrutinee: Box<HirExpr>,
    arms: Vec<HirMatchArm>,
    span: Span,
}
```

Arm bodies are expression blocks (synthetic `HirBlock` with no statements
and the expression as the trailing result).

## 7. MIR

`lower_match_expr()` builds a CFG identical in structure to the match
statement lowering, but with a result local and merge block:

```text
pre:     scrutinee = <scrutinee value>     (evaluated once)
         result = <zero>                   (result local)
         jump test0
test0:   cond = scrutinee == <arm0 const>  (refutable arms only)
         branch cond → arm0, test1
...
armk:    [binding copy] <arm body>
         result = <arm body value>         (jumps to merge)
...
merge:   ...                               (result = result local)
```

Each arm body's trailing expression is evaluated and stored in the result
local. After all arms converge on the merge block, the result local is
the expression's value.

Zero-arm match (empty enum): vacuously exhaustive, returns a defensive
zero constant.

## 8. Ownership

`ExprKind::MatchExpr(m)` in `eval_expr()`: the scrutinee is observed,
each arm's pattern bindings are registered (copies for scalars, payload
moves for data-carrying variants), guards are observed, and arm bodies
are evaluated. Payload moves are tracked identically to the match
statement. The match expression itself produces a copy.

## 9. Backend

No new backend instructions: the match expression lowers through the same
CFG shapes as the match statement (branch chains, word equality,
constants, copies). The result local is a normal MIR local read after
the merge block.

## 10. Diagnostics

| Code   | Description |
| ------ | ----------- |
| E-T01  | Mismatched arm result types |
| E-T24  | Non-exhaustive match expression |
| E-T25  | Unreachable arm in match expression |
| E-T26  | Invalid match scrutinee type |

No new error codes were introduced; existing diagnostics apply to both
statement and expression forms.

## 11. Known limitations

- **Match expressions in arbitrary operand position:** match expressions
  work in binding, return, expression-statement, and block-trailing
  positions, but not as function arguments or binary operands. This
  mirrors the existing `if`-expression limitation.
- **Match expressions inside block expressions in binding position:**
  supported via `lower_block_trailing_expr()`.

## 12. Tests

**1,563 tests passing** (1,522 pre-existing + 41 new).

New test file: `tests/match_expressions.rs` with:
- 15 parse tests (basic, block arms, trailing position, return, if
  nesting, guard, or-pattern, range, enum, payload, nested, binding,
  bool, negative, binary operand)
- 4 type-checking tests (mismatched types, non-exhaustive, unreachable
  arm, binding mismatch)
- 22 native E2E execution tests (basic int, catch-all, bool, binding,
  negative pattern, return value, enum, payload, nested, or-pattern,
  range, guard, if branch, block trailing, multiple arms, statements in
  arm, function arg, arithmetic, determinism, empty enum)
