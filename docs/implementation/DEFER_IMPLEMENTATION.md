# Deferred cleanup (`defer`) — implementation record

**Sessions:** 249–253 (scope-exit cleanup), 259–266 (explicit move capture)
**Status:** Implemented for the Windows x86_64 native target; permanent tests
(`tests/defer.rs`, `tests/defer_move.rs`) and five examples.
**Syntax:** `defer { statements }`, statement-only, no trailing semicolon, and
`defer move (a, b) { statements }`, which transfers the listed locals into the
deferred operation when the statement executes.

## Purpose

`defer` gives MINK explicit scope-exit cleanup without exceptions or a
destructor system. A program registers a block, and the compiler runs that
block when execution leaves the lexically enclosing scope. This covers normal
fallthrough, early `return`, and loop `break`/`continue`, including deeply
nested scopes.

A plain `defer` body reads the enclosing bindings when cleanup runs. That is
convenient for copy handles (`defer { rt_mutex_unlock(lock); }`), but unsafe
for owned values: between registration and cleanup the value may be moved,
reassigned, or released, so reading it at scope exit could observe invalid
storage. The original implementation rejected those reads outright (`E-S18`),
which blocked the most useful cleanup shape of all — freeing an owned resource
exactly once.

Session 260 adds the missing mechanism: `defer move (a, b) { ... }` performs an
explicit ownership transfer at registration. The value leaves the original
binding, the cleanup body reads it from a capture slot that nothing else can
reach, and the cleanup is therefore the only owner while it runs.

## Grammar and parsing

```ebnf
Defer       := 'defer' CaptureList? Block
CaptureList := 'move' '(' Ident (',' Ident)* ','? ')'
```

`defer` and `move` are reserved keywords. The parser dispatches `defer` from
statement position and parses a normal braced block with `parse_block_body`.
The body is a statement block: a trailing expression is accepted and
discarded. `defer;`, `defer` followed by anything other than `{`, and
`defer move` without a parenthesized list are parse errors (`E-P11`, `E-P08`).
An empty capture list (`defer move () { }`) is an error (`E-P02`) rather than a
silently meaningless form, and the trailing comma is optional.

`StmtKind::Defer { body, capture }` carries the capture mode through the AST
(`DeferCapture::{None, Move(Vec<Ident>)}`); `HirStmtKind::Defer { body,
capture }` is the HIR form (`HirDeferCapture`, whose `Move` list holds resolved
`HirIdent`s — symbol *and* type — so no later stage re-runs name resolution).
Every exhaustive HIR walker in monomorphization, type checking and ownership
recurses into the body.

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

### Move capture (session 260)

```mink
fn consume(payload: Str) -> Int {
    defer move (payload) {
        rt_print_str("closing");
        rt_str_free(payload);
    }
    return rt_str_len(payload);      // OK: read before the transfer
}
```

Rules, all enforced by the compiler:

- **Transfer at registration.** When the `defer` statement executes, each
  listed local is copied into a fresh capture slot (a hidden MIR local) and the
  original binding becomes moved. The transfer is part of the statement's own
  control flow, so an unexecuted conditional registration transfers nothing,
  and a registration inside a loop transfers once per iteration.
- **The original is dead.** After the statement, reading, moving, or freeing the
  original binding is `E-S10` (use of a moved value). That is what prevents a
  double free: only the capture holds the value.
- **The cleanup is the only owner.** Inside the body the captured names denote
  the capture slots. A value is guaranteed live at cleanup time because nothing
  else can reach it, and consuming it twice inside the body is `E-S10`.
- **Exactly once.** Cleanup runs once per executed registration, and the
  capture is consumed at most once by the body.
- **Copy values snapshot.** A captured `Int` keeps the value it had at
  registration; unlike a plain `defer`, later writes to the original are not
  observed. (The original mutable binding stays usable: the capture copied it.)
- **Immortal values copy.** A string literal binding is copied, not moved: it
  never owns storage, so its original binding stays usable after the capture,
  exactly like an ordinary move of an immutable string.
- **Owned values transfer.** `Str` and ownership-tracked aggregates move: the
  original dies, the capture owns the storage.
- **Plain `defer` is unchanged**, including `E-S18` for names it reads but does
  not capture.

What is *not* captured: a name the body reads that is not in the list keeps the
plain-`defer` rules, so an owned local read without being listed is still
`E-S18`. Listing one value does not weaken the check for any other.

## Static restrictions

### `E-S17` — invalid `defer` control flow or placement

- `return` inside a deferred body.
- `break`/`continue` that would leave the deferred body. Loops entirely
  inside the body may use them.
- `defer` in a block whose trailing expression is the block's value, because
  MIR forms the block value after statement lowering. A function body with
  `defer` cannot also use a trailing expression as its return value; write an
  explicit `return`.

### `E-S18` — deferred owned capture (plain `defer`)

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

Session 260 refined one case without weakening the rule: a binding **declared
inside** the deferred body lives exactly as long as the cleanup, so it cannot be
invalidated by later code and `E-S18` does not apply to it. Without that
refinement a cleanup could not allocate a temporary, and an owned payload could
not be bound by a `match` inside the cleanup — both of which are safe. Outer
owned locals are still rejected, which the regression tests pin down.

### `E-S19` — invalid move capture

`defer move (...)` transfers ownership, so it accepts only values whose move
the ownership pass tracks soundly. Rejected shapes, each with a specific
message:

| Shape | Why |
| --- | --- |
| a reference (`&T`, `&mut T`) | a reference cannot be owned, and the cleanup could outlive the referent |
| a fixed-length array | an array is owned as one value, so reading an element in the cleanup moves the whole array; a multi-element cleanup is not expressible |
| a tuple | tuple bindings are not ownership-tracked, so the transfer could not invalidate the original tuple |
| `Vec`/`Map`/`Set` | copy handles with user-managed lifetimes; use an ordinary `defer` |
| `Ptr<T>` | copy handle with a user-managed lifetime; use an ordinary `defer` |
| a struct/enum containing any of the above | the same reason, reported for the offending component |
| a non-local name (module `let`, `const`, function, intrinsic) | there is no per-execution local value to move |
| a name listed twice in one list | the first entry already took ownership |
| a capture registered inside a deferred block | it would run during that cleanup, after the value could already be released |
| a capture the cleanup never uses | the value would leak (nothing else can free it after the transfer) |

A capture made while the value is borrowed is a borrow conflict (`E-S12`):
moving it would leave the reference dangling. A capture of a value that is
already moved is a use of a moved value (`E-S10`) at the capture name.

## Conditional and loop ownership (session 268)

Every control-flow construct that can reach a join (an `if` arm, a `match`
arm, a block, a loop body) is analyzed from the state at its entry, and the
arms converge on a **may-move join**: a binding that is dead on any arm is
dead afterwards, struct fields join field-wise, structs/enums that are dead in
any arm are dead whole, and borrows take the strongest count observed. A
transfer that happens only on the taken path is therefore visible to the code
after the join, so a free or a second transfer after a conditional capture is
`E-S10` rather than a double free at run time.

A loop body **can run more than once**, which a single pass does not model. The
analysis therefore walks each loop body to a fixpoint over its back edge
(`walk_loop_body`):

1. the first pass runs with its diagnostics discarded and only discovers the
   states that reach the loop head again — the state at the end of the body and
   the state at each `continue`;
2. the loop head becomes the join of the loop entry and those back edges;
3. the second, authoritative pass walks the body from that loop head, so a
   move or a transfer a previous iteration performed is visible and reported;
4. after the loop, the state is the join of the zero-iteration path, every
   `break` state, and the back edges.

A `break` leaves the loop and therefore carries no back edge: a body that
registers a capture and always breaks transfers once and is accepted. This
closes the defect that a `defer move` of a local declared **outside** a loop
re-transferred the same value on the second iteration (compiled cleanly, then
an `E-R04` invalid-free at run time). The same loop head now also rejects the
plain-move form of the same bug (`let t = s;` repeated by a second iteration).

Captures of values declared **inside** the loop body are unaffected: those are
fresh owners each iteration and are the supported pattern.

## Compiler pipeline

- **Lexer:** two keyword entries (`defer`, `move`) and the matching
  `TokenKind` variants. The keyword table stays sorted for its binary search.
- **Parser:** `parse_defer` plus `parse_defer_capture`; `defer` is included in
  the keyword-start statement predicates.
- **AST/HIR:** `Defer { body, capture }` variants and HIR lowering;
  `lower_defer_capture` resolves each listed name through the expression
  resolution path so the HIR list carries symbols and types.
- **Semantics:** `Ctx` tracks `loop_depth`, `in_defer`, and
  `defer_loop_floor`; loop depth is incremented around loop bodies and
  restored afterwards. Placement and control-flow checks produce `E-S17`;
  `analyze_move_capture` resolves the capture list and rejects non-locals,
  duplicates, and registrations inside a deferred block (`E-S19`).
- **Ownership:** loop bodies are walked to a back-edge fixpoint (session 268),
  and `join_flow` is the single may-move join used by `if`, `match` and loops.
  A `defer_depth` counter drives `E-S18` from `eval_ident`. The
  check combines the binding's current state with its declared type and
  mutability so a mutable owning-type binding cannot become an unsafe capture
  after registration. Tuple/collection contents are inspected explicitly.
  `walk_move_defer` validates each capture (`E-S19`/`E-S12`), walks the body
  with the captures treated as live (a second consumption inside the body is
  `E-S10`), reports captures the body never reads, and then marks the original
  bindings moved. Reads of captured names are recorded through a
  `DeferCaptureFrame` stack, so `E-S18` is suspended only for the values the
  cleanup actually owns.
- **Type checking:** both the normal traversal and the deferred-member recheck
  path traverse defer bodies.
- **Monomorphization:** all exhaustive statement matches recurse into the
  defer body, and `reassign_stmt_spans` re-spans the capture list with the rest
  of the clone. Without that, a cloned generic body's capture entry kept the
  original span and resolved to a different instantiation's binding — a real
  defect found by the generic-instantiation test.
- **MIR:** `FnBuilder` keeps `defer_scopes: Vec<Vec<DeferEntry>>`, outermost
  first, where a `DeferEntry` is the body plus `(symbol, capture local)` pairs.
  `lower_block` pushes a scope, lowers statements, then on normal fallthrough
  pops it and inlines its entries in reverse order. `register_defer` declares
  the capture slot and assigns the source local into it *at the registration
  site*, then records the pair. `lower_defer_body` temporarily rebinds each
  captured symbol to its capture slot while lowering the body, and restores the
  previous mapping afterwards. A value-less block expression is routed through
  `lower_block` as well, so its cleanup runs at that block's exit rather than
  leaking to the enclosing scope. `LoopCtx` stores `defer_floor`, the scope
  count outside the loop body, so `break`/`continue` inline exactly the scopes
  opened inside the loop. `return` inlines every remaining scope down to depth
  zero after snapshotting the return value. No backend or runtime special case
  is involved; a capture is an ordinary local copy and the cleanup is ordinary
  MIR control flow.

## Tests and examples

`tests/defer.rs` has 33 permanent tests for plain `defer`:

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

`tests/defer_move.rs` has 74 permanent tests for move capture:

- 6 parser tests: accepted list, trailing comma, empty list, missing list,
  unterminated list, and plain `defer` still parsing.
- 7 semantic tests: registration inside a deferred block, duplicate name,
  unresolved name, `const`, module binding, function name, and accepted
  loop/arm placements.
- 25 ownership tests. Accepted: owned `Str`, owned + copy together, struct,
  enum payload, cleanup-local allocation, `for` variable, partial field use, and
  an immutable string literal (which copies, leaving the original usable).
  Rejected: use after move, read after registration, second capture of the same
  value, return of the captured value, pass of the captured value, reference,
  array, tuple, collection, struct holding a reference, struct holding an array,
  unused capture, double free inside the cleanup, capture while borrowed, and an
  uncaptured owned read. Two further tests pin that plain `defer` still rejects
  owned and tuple captures (`E-S18`).
- 20 native execution tests: single free, two-resource LIFO, cleanup on return,
  per-iteration cleanup, conditional registration, `break`/`continue`,
  `continue` before registration, nested scope order, struct cleanup, enum
  payload cleanup, copy-value snapshot, immutable literal printed twice,
  reassignment after capture, return-value snapshot, break-value snapshot,
  match arm, cleanup-local temporaries, generic instantiation, parameter
  cleanup, and mixed plain/move ordering.
- 16 session 267–270 tests: loop re-registration rejected for `while`/`for`/
  `continue` back edges and for the plain-move form of the same bug; loop-local
  and break-only registrations accepted; use after the loop rejected; capture
  after a branch that freed the value rejected; conditional capture with a
  freeing `else` accepted and executed at branch exit; tuple and array captures
  rejected with their specific reasons; generic captures accepted-with-copy and
  rejected-when-unused; and a closure inside the cleanup freeing the capture
  once.

Both debug and release native runs pass. `cargo check --all-targets` and the
release compiler build are clean.

Examples:

- `examples/defer_resource/main.mink` locks a real runtime mutex, registers
  `rt_mutex_unlock`/`rt_mutex_free` cleanup, and returns early through it.
- `examples/defer_scope_order/main.mink` demonstrates nested loop/`if` scope
  ordering with LIFO cleanup.
- `examples/defer_move_resource/main.mink` transfers an owned string into a
  deferred free and shows the cleanup running once.
- `examples/defer_move_multi/main.mink` transfers several buffers, frees them
  LIFO, and covers normal completion plus an early return out of a loop.
- `examples/defer_move_pipeline/main.mink` is a small record pipeline that
  builds owned lines, hands them over by move, and releases them on both the
  accept and reject paths.

## Aggregate capture investigation (session 269)

Each unsupported aggregate was examined against the ownership model rather than
rejected by default. None is supported yet, and each rejection now states the
actual reason:

| Category | Ownership representation today | Verdict |
| --- | --- | --- |
| Tuple | construction **moves** its owned elements, but the tuple binding has no tracked state, so a tuple-to-tuple move is not tracked and re-using the original is possible | rejected: a transfer could not invalidate the original tuple, so the cleanup and the original could both free the same value |
| Fixed array | one whole-array state (`Shape::Array`); an element read in transfer position moves the *whole* array | rejected: only one element can be consumed before the array is dead, so the multi-element cleanup that motivates arrays is not expressible |
| `Vec`/`Map`/`Set` | copy handles with user-managed lifetimes | rejected: not an owned resource; the ordinary `defer` rules already cover them |
| `Ptr<T>` | copy handle | rejected: same reason |

Supporting tuples or arrays needs element-level ownership state (per-element
liveness, joins, and destructuring transfers) — a change to the general
ownership model, not a capture feature. The capture implementation therefore
continues to reject them with `E-S19` and a message that names the reason.

## Measured cost (session 271)

`benchmarks/defer/measure.sh` regenerates these programs and prints the table;
the numbers below are from a Windows x86_64 debug-host / release-compiler run
(minimum of three runs per case; sizes are the produced `.exe` in bytes, build
time is `mink build` wall time in ms).

| Case | exe bytes | build ms | run ms |
| --- | --- | --- | --- |
| inline free, no `defer` | 53,760 | 33 | – |
| one plain `defer` | 53,760 | 33 | – |
| three move captures | 54,784 | 34 | – |
| three nested scopes, one capture each | 54,272 | 33 | – |
| two return paths | 53,760 | 34 | – |
| four return paths | 54,272 | 33 | – |
| eight return paths | 54,784 | 33 | – |
| 1,000,000 loop iterations, capture | 54,272 | 33 | 43 |
| 1,000,000 loop iterations, inline free | 54,272 | 33 | 42 |

The fixed runtime dominates the executable (53,760 bytes with no `defer` at
all). Above that baseline, each capture or exit path adds a few hundred bytes
of inlined cleanup, in 512-byte section steps: three registrations cost 1,024
bytes, and going from two to eight return paths costs 1,024 bytes. Compile time
is unchanged within noise (33–34 ms). The 1,000,000-iteration loop shows no
meaningful run-time difference between a capture and an inline free (43 ms vs
42 ms, same output), so a capture slot costs the same as the local it replaces.

This is not a reason to add a runtime cleanup stack: the observed growth is
proportional to the number of *exit paths* in one function, which is small in
practice, and a capture is an ordinary stack local.

## Deliberate limitations

- `E-S18` remains for plain `defer`: reading an owned or borrowed local in a
  cleanup body that did not transfer it is still an error. Use
  `defer move (name)` instead.
- Move capture supports ownership-tracked values (`Str`, copy scalars, structs
  and enums of supported fields). Arrays, tuples, collections, pointers, and
  references are rejected with `E-S19` rather than supported unsafely.
- A cleanup may not register another move capture (`E-S19`); a plain `defer`
  inside a cleanup body is still allowed.
- A capture the body never reads is rejected rather than silently leaked.
- Joins are conservative, not path-exact: a binding dead on *any* arm is dead
  after the join. Two complementary registrations
  (`if c { defer move (s) … } if !c { defer move (s) … }`) are therefore
  rejected even though exactly one of them executes, and a registration that
  sits before an unconditional `break` is accepted only because the `break`
  removes the back edge. Conservative rejection never accepts an unsafe
  program; it can reject a safe one, and the fix in that case is to declare or
  transfer the value where it is used.
- A loop whose body can iterate twice must not transfer a value declared
  outside it; the value is already moved on the second iteration. Declare it
  inside the body (the supported pattern) or move the loop into a function
  whose parameter is transferred once.
- Generic functions can contain move captures of concrete locals (verified at
  two instantiations). A *generic-typed* parameter may be captured and copied
  inside the cleanup, but MINK has no generic destructor operation, so such a
  cleanup can leak the value; an unused generic capture is rejected like any
  other unused capture. Instantiating a generic function with an owned string
  argument is still limited by the pre-existing generic-call inference rule.
- Pre-existing and outside the capture subset: tuple *bindings* are not
  ownership-tracked, so moving a tuple copies its fields (`let u = t;` leaves
  `t.0` usable and a double free is possible). This is why tuples cannot be
  move-captured; fixing tuple ownership is a separate change.
- A block expression cannot contain `defer` and also use its trailing
  expression as a value. A value-less block expression does contain its own
  deferred cleanup scope.
- Cleanup bodies are inlined at every exit path, so a scope with many exits and
  many large defers grows code proportionally. There is no runtime registration
  list; a capture slot is an ordinary stack local.
- `defer` does not replace ownership checking for normal values; it is cleanup
  control flow, not a destructor protocol.
- The feature is verified on the Windows x86_64 native target only. Linux
  remains frozen.
