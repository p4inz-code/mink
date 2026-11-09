# Deferred cleanup (`defer`) — implementation record

**Sessions:** 249–253
**Status:** Implemented for the Windows x86_64 native target; permanent tests and
two examples.
**Syntax:** `defer { statements }`, statement-only, no trailing semicolon.

## Purpose

`defer` gives MINK explicit scope-exit cleanup without exceptions or a
destructor system. A program registers a block, and the compiler runs that
block when execution leaves the lexically enclosing scope. This covers normal
fallthrough, early `return`, and loop `break`/`continue`, including deeply
nested scopes.

## Grammar and parsing

```ebnf
Defer := 'defer' Block
```

`defer` is a reserved keyword. The parser dispatches it from statement
position and parses a normal braced block with `parse_block_body`. The body is
a statement block: a trailing expression is accepted and discarded. `defer;`
and `defer` followed by anything other than `{` are parse errors. There is no
`defer` expression.

`StmtKind::Defer(Block)` carries the body through the AST;
`HirStmtKind::Defer(HirBlock)` is the HIR form. Every exhaustive HIR walker in
monomorphization, type checking and ownership recurses into the body.

## Semantics

### Registration and scope

- A `defer` statement appends its body to the innermost lexical block's
  registration list. Registration is a compile-time event in statement order:
  a `return` that occurs before a later `defer` does not run it.
- Conditional registrations follow control flow. A `defer` in the untaken
  branch of an `if` never runs. A `defer` reached in several loop iterations is
  registered each time and runs each time the body scope exits.
- A nested block registers into its own scope. Leaving that block runs its
  registrations; leaving the outer block later runs the outer ones.

### Ordering

Cleanup is LIFO within a scope. Scopes run innermost first: all registrations
of an inner scope, last-registered first, then the enclosing scope's, and so
on. Nested `defer` blocks are ordinary blocks with their own scope, so a
`defer` inside a deferred body is cleaned up when that body ends before the
next registration runs.

### Exit paths

Cleanup runs on:

- normal fallthrough of a block;
- `return`, including a `return` inside nested loops and blocks;
- `break` from any loop, leaving every scope opened inside the loop body but
  not scopes outside it;
- `continue`, leaving the iteration's inner scopes before jumping to the
  condition/update.

Cleanup does not run when a scope was never entered. On a diverging path the
cleanup blocks are emitted into the still-live graph, then the path terminates
with the original terminator.

### Values

Deferred bodies read variables when cleanup runs, not when the `defer` is
registered. Writing an ordinary local after registration therefore changes
what cleanup observes.

A value already computed for `return expr;` or `break expr;` is snapshotted
into a temporary before cleanup. Cleanup writes to the source local cannot
change the returned or broken value. This was a real defect found during
verification: without the snapshot, `let mut x = 1; defer { x = 9; } return x;`
returned `9`; it now returns `1`, and `tests/defer.rs` covers both return and
break values.

## Static restrictions

The semantic analyzer and ownership pass reject three unsafe shapes.

### `E-S17` — invalid `defer` control flow or placement

- `return` inside a deferred body.
- `break`/`continue` that would leave the deferred body. Loops entirely
  inside the body may use them.
- `defer` in a block whose trailing expression is the block's value, because
  MIR forms the block value after statement lowering. A function body with
  `defer` cannot also use a trailing expression as its return value; write an
  explicit `return`.

### `E-S18` — deferred owned capture

A deferred body may not name a binding whose type may own or borrow storage.
The rejected kinds are an owned `Str`, any array binding, a mutable binding of
an owning type (even when its current value is an immutable literal), an owned
or reference-bearing struct, an enum holding owned/reference state, any
reference binding, and a tuple or collection carrying an owned value. These
values may be moved, reassigned, or released after registration and before
cleanup, so reading them at scope exit could observe invalid storage. Copy
values (`Int`, `Float`, `Bool`, `Char`, copy-only tuples, copy-only structs)
and immutable locals without references are allowed. A `const` is a fresh copy
at each use and is also allowed.

The type check is important: a `let mut s = "literal"` binding is allowed to
be assigned an owned allocation later, so its registration-time provenance is
not enough. The analyzer rejects the mutable binding up front. The tuple check
is separate because the general ownership pass does not otherwise track tuple
fields; a `(Str, Int)` deferred read must still be rejected.

The restriction is deliberately conservative and lexical: the check runs
wherever a name is read while analysis is inside a deferred body, including
inside a closure in that body. Rejecting all arrays rather than only arrays
with owned elements keeps the rule easy to reason about.

## Compiler pipeline

- **Lexer:** one keyword entry and `TokenKind::Defer`.
- **Parser:** `parse_defer`; `defer` is included in the keyword-start
  statement predicates.
- **AST/HIR:** `Defer(Block)` variants and HIR lowering.
- **Semantics:** `Ctx` tracks `loop_depth`, `in_defer`, and
  `defer_loop_floor`; loop depth is incremented around loop bodies and
  restored afterwards. Placement and control-flow checks produce `E-S17`.
- **Ownership:** a `defer_depth` counter drives `E-S18` from `eval_ident`. The
  check combines the binding's current state with its declared type and
  mutability so a mutable owning-type binding cannot become an unsafe capture
  after registration. Tuple/collection contents are inspected explicitly.
- **Type checking:** both the normal traversal and the deferred-member recheck
  path traverse defer bodies.
- **Monomorphization:** all exhaustive statement matches recurse into the
  defer body.
- **MIR:** `FnBuilder` keeps `defer_scopes: Vec<Vec<HirBlock>>`, outermost
  first. `lower_block` pushes a scope, lowers statements, then on normal
  fallthrough pops it and inlines its bodies in reverse order. A value-less
  block expression is routed through `lower_block` as well, so its cleanup
  runs at that block's exit rather than leaking to the enclosing scope.
  `LoopCtx` stores `defer_floor`, the scope count outside the loop body, so
  `break`/`continue` inline exactly the scopes opened inside the loop.
  `return` inlines every remaining scope down to depth zero after snapshotting
  the return value. No backend or runtime special case is involved; the
  cleanup is ordinary MIR control flow.

## Tests and examples

`tests/defer.rs` has 33 permanent tests:

- 4 parser tests: empty body, nested statements/`defer`, missing block,
  unterminated statement.
- 12 semantic, ownership and type tests: valid placement, inner loop control
  flow, `return`, `break`, `continue`, trailing-expression placement, an error
  inside the body, owned capture rejection, mutable-capture rejection,
  tuple-owned capture rejection, copy-only tuple acceptance, and allowed
  copy/immutable capture.
- 17 native execution tests: LIFO on normal exit, cleanup on return, break and
  continue, per-iteration cleanup, nested scopes inside-out, nested `defer`,
  return inside nested loops, conditional registration, continue before
  registration, return-value, break-value and struct-return snapshots,
  value-less block-expression scope, nested loop floors, match-arm cleanup,
  inner-loop control flow in a deferred body, and nested statements in a
  deferred body.

Both debug and release native runs pass. `cargo check --all-targets` and the
release compiler build are clean.

Examples:

- `examples/defer_resource/main.mink` locks a real runtime mutex, registers
  `rt_mutex_unlock`/`rt_mutex_free` cleanup, and returns early through it.
- `examples/defer_scope_order/main.mink` demonstrates nested loop/`if` scope
  ordering with LIFO cleanup.

## Deliberate limitations

- Deferred bodies cannot capture owned locals; use a copy handle (as the mutex
  example does with its `Int` handle) or perform the move explicitly before
  registration.
- A block expression cannot contain `defer` and also use its trailing
  expression as a value. A value-less block expression does contain its own
  deferred cleanup scope.
- Tuple/collection values carrying owned storage are rejected; collection
  handles without owned elements remain copy handles with user-managed
  lifetimes.
- Cleanup bodies are inlined at every exit path, so a scope with many exits and
  many large defers grows code proportionally. There is no runtime registration
  list.
- `defer` does not replace ownership checking for normal values; it is cleanup
  control flow, not a destructor protocol.
- The feature is verified on the Windows x86_64 native target only. Linux
  remains frozen.
