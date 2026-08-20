# MINK — While/Loop Expressions and Break Values Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 30 — while/loop as expressions, break with values

## 1. Overview

Session 30 makes `while` and `loop` usable as expressions and allows
`break` to carry a value. This completes the expression-oriented control
flow story started in Session 28 (block expressions and if-as-expression):

- `if cond { a } else { b }` as an expression — Session 28
- `loop { break value; }` as an expression — Session 30
- `while cond { break value; }` as an expression — Session 30

The implementation covers the full compiler pipeline: grammar, parser,
AST, semantic analysis, type checking, HIR, MIR, ownership analysis,
backend, and runtime.

## 2. Grammar Additions

```
Break       := 'break' Expr? ';'
Primary     := ... | WhileExpr | LoopExpr
WhileExpr   := 'while' Expr Block
LoopExpr    := 'loop' Block
```

See `docs/language/CORE_GRAMMAR.md` §23 for the full specification.

## 3. Semantics

- **`break expr;`** evaluates `expr` and stores the value in the loop's
  break-value slot, then transfers control to the loop's exit block.
  The value's type determines the loop expression's type.
- **`break;`** (no value) is unchanged and has type `Unit`. Inside a
  loop expression, `break;` without a value produces diagnostic `E-T36`.
- **`loop { body }`** in expression position repeats `body` until `break`
  transfers control to the exit. The expression's type is the type of the
  `break` values; multiple `break` values must agree (`E-T01` on
  mismatch).
- **`while cond { body }`** in expression position evaluates `cond`,
  then `body` if true, until `break` or condition false. Same typing
  rules as `loop`.
- Loop expressions appear in binding position (`let x = loop { ... }`),
  return position (`return loop { ... }`), and expression statement
  position. They do not appear in arbitrary operand position (function
  arguments, binary operands) — this mirrors the existing `if`-expression
  limitation (the MIR lowering requires CFG construction that `StmtEval`
  cannot provide).
- In statement position, `while`/`loop` are unchanged (no break value,
  type `Unit`).

## 4. AST

New variants in existing enums:

- `StmtKind::Break(Option<Expr>)` — `break;` or `break expr;`
- `ExprKind::WhileExpr { cond, body, span }` — while-expression
- `ExprKind::LoopExpr { body, span }` — loop-expression

Both expression variants use `Box<Block>` to break the infinite-size
recursion with `Expr → ExprKind → Block → Stmt → StmtKind → Break(Option<Expr>)`.

## 5. Parser

- **`parse_break()`** consumes `break`, then checks: if the next token
  is `;`, `}`, a block-terminating keyword, or `Eof`, it produces
  `Break(None)`. Otherwise, it parses an expression and produces
  `Break(Some(expr))`.
- **`parse_while_expr()`** and **`parse_loop_expr()`** parse the
  condition/body and return `ExprKind::WhileExpr` or `ExprKind::LoopExpr`.
- The block-expression parser (`parse_block_expr`) dispatches `while` and
  `loop` keywords to expression parsing (not statement parsing) when they
  appear in trailing expression position.

## 6. Type System

**New error code:** `E-T36` (`BreakValueExpected`) — `break;` inside a
loop expression must carry a value.

**Type checking rules:**

- A loop expression's type is a fresh inference variable.
- Each `break expr;` inside the loop constrains this variable to `expr`'s
  type (`E-T01` on mismatch).
- `break;` without a value inside a loop expression is `E-T36`.
- `break;` without a value in a statement-position loop is unchanged.

## 7. HIR

New variants:

- `HirStmtKind::Break(Option<HirExpr>)`
- `HirExprKind::WhileExpr { cond, body, span }`
- `HirExprKind::LoopExpr { body, span }`

## 8. MIR

Loop expressions use a **merge block** pattern:

```
pre:   jump header
header: cond = <condition>          (while only)
        branch cond → body, merge   (while only)
        jump body                   (loop only)
body:   <body statements>
        [break: store value → break_local, jump merge]
        jump header                 (natural fall-through)
merge:  result = break_local        (read by caller)
```

`FnBuilder::eval_operand` wraps `StmtEval::eval_operand` to intercept
`WhileExpr`, `LoopExpr`, and `Block` with loop-expression trailing
expressions, lowering them through CFG construction.

## 9. Ownership

Loop expressions evaluate their body in a loop scope. `break expr;`
evaluates `expr` in transfer mode (owned values move). The loop
expression itself produces a copy (the value lives in the break-value
local).

## 10. Diagnostics

| Code   | Description |
| ------ | ----------- |
| E-T36  | `break` inside a loop expression must carry a value |

## 11. Known Limitations

- **Loop expressions in arbitrary operand position:** loop expressions
  work in binding, return, and expression-statement positions, but not
  as function arguments or binary operands. This mirrors the existing
  `if`-expression limitation — the MIR lowering requires CFG construction
  that `StmtEval` cannot provide.
- **Uninitialized break-value local:** if a while-expression's condition
  is false on entry, the break-value local is zero-initialized by the
  native backend. This matches the existing behavior for uninitialized
  locals.

## 12. Tests

**1,454 tests passing** (1,415 pre-existing + 39 new).

New test file: `tests/loop_expressions.rs` with:

- 16 parse tests (loop/while expressions, break values, nesting)
- 13 analysis tests (type checking, annotations, constants)
- 2 negative tests (type mismatch)
- 17 native E2E execution tests (basic, looping, arithmetic, nesting,
  function args, continue, tuples, determinism)
