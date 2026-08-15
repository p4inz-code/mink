# MINK — Optimization Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 10 — Optimization Passes

## 1. Purpose

Optimization is the stage between MIR lowering and the native backend. It
rewrites the structurally validated MIR produced by session 09 into an
equivalent, smaller program while never changing observable program
behavior:

    Source → Lexer → Parser → AST → Semantic Analysis → Type Analysis
        → HIR → MIR → Optimization → Backend → Runtime → executable

The optimization stage is a **stable boundary**: it consumes the exact MIR
that `mink check` validates and produces MIR with the same node kinds,
types, spans, and structure, so the backend (session 11,
`NATIVE_BACKEND_IMPLEMENTATION.md`) consumes an already-optimized graph and
needs no knowledge of the passes that ran.

Like every other stage, optimization is:

- **behavior-preserving** — a pass removes or rewrites a node only when its
  elimination is provably unobservable;
- **deterministic** — iteration follows block and statement order and no
  pass output depends on hash iteration order, so identical input always
  yields identical optimized MIR;
- **validated** — the program is structurally validated before the first
  pass and after *every* pass; a pass that breaks an invariant is reported
  as `MirError`s, never a panic;
- **non-speculative** — no transformation crosses a semantic boundary that
  cannot be proven safe, and no observable side effect (call, static
  store, member/index load) is ever removed.

## 2. The MIR Constant Boundary

MIR deliberately does **not** decode literal values: `Int`, `Float`,
`Str`, and `Char` constants carry only their *kind* and span, with the raw
text recovered from the source map. `Bool(bool)` is the **only** constant
whose *value* MIR carries (booleans are `true`/`false` — a two-value
domain with no textual ambiguity).

This constraint defines the folding frontier:

- **Folded:** the boolean algebra — `!true → false`, `true && false →
  false`, `false || true → true`, `true == false → false`, `true != false
  → true`, and any nested combination.
- **Not folded:** arithmetic (`1 + 2`), string/char operations, and any
  other fold that would require decoding a literal value. Folding those
  would either duplicate the source-map decoding the front end already
  owns or fabricate semantics the front end never committed to. This is a
  deliberate, documented boundary, not a missing feature.

Because `&&`/`||` short-circuit through branches and conditions feed
`Branch` terminators, the boolean folds cascade naturally: a folded
condition makes a branch constant, a constant branch simplifies to a
jump, and the now-unreachable arm is eliminated.

## 3. The Pass Pipeline

`mir::optimize(&MirProgram) -> Result<MirProgram, Vec<MirError>>` runs
five passes, in order, to a fixpoint (with a defensive round cap that
guarantees termination — every pass is monotone, only removing nodes or
rewriting them to smaller forms):

| Pass              | Name (reported)     | What it does                                        |
| ----------------- | ------------------- | --------------------------------------------------- |
| `ConstFold`       | `const-fold`        | Folds the `Bool` algebra in every rvalue            |
| `CopyProp`        | `copy-propagation`  | Replaces reads of a local that provably holds a copy of another value with that value; eliminates redundant moves |
| `CfgSimplify`     | `cfg-simplify`      | Folds constant-condition branches into jumps, collapses branches whose targets coincide, threads jumps through empty blocks |
| `UnreachableElim` | `unreachable-elim`  | Removes blocks unreachable from the entry block and renumbers survivors |
| `DeadCodeElim`    | `dead-code-elim`    | Removes stores to locals never read afterwards, when the stored rvalue is provably side-effect-free |

All passes implement the `MirPass` trait (`name()` + `run(&mut MirProgram)
-> bool`), so the pipeline is composable and future passes slot in by
appending to the pass list.

### 3.1 `ConstFold`

Walks every statement rvalue in every block (functions and module
statics). A `Unary(Not)` or `Binary(And | Or | Eq | Ne)` whose operands
are `Bool` constants is replaced by a `Use` of the folded constant,
**preserving the rvalue's span and type**. Operators that never type-check
on `Bool` are left untouched: folding them would fabricate semantics the
front end rejected.

### 3.2 `CopyProp`

Within each block, a statement `t = Use(x)` (where `x` is a local or a
constant) records that `t` holds `x`'s value **until either is
reassigned**. Later reads of `t` in the same block are rewritten to read
`x` directly (the read's own span and type are preserved), after which the
copy statement usually becomes dead and is removed by `DeadCodeElim`.

Kills are conservative:

- reassigning a local invalidates every recorded copy *of* that local
  (their recorded value is the old one);
- reassigning the target invalidates its own record;
- a `Member`/`Index`/`Static` target — whose place semantics or
  reachability cannot be proven — clears all records.

Calls do **not** clear local records: this language has no pointers or
references, so a callee cannot observe or modify another function's
locals.

### 3.3 `CfgSimplify`

- A `Branch` on a constant condition becomes a `Jump` to the taken arm.
- A `Branch` whose two targets coincide becomes a `Jump`.
- A `Jump` through an empty block with exactly one predecessor is threaded
  to the empty block's target.

### 3.4 `UnreachableElim`

Computes reachability from the entry block (block 0) over terminator
edges, removes unreachable blocks, and renumbers the survivors so the
invariant "block at index `i` has id `i`" holds.

### 3.5 `DeadCodeElim`

Removes a store to a local that is never read afterwards when the stored
rvalue is provably side-effect-free (`Use`, `Unary`, `Binary`, `Range`,
`RangeNext`, `RangeFinished`). Anything that can observe state — a `Call`,
a `Member`/`Index` load, or any `Static` store — is never eliminated.
Parameter writes are kept (they are externally observable), and a store is
kept when the local is read by the block's terminator or any later block.

## 4. Soundness Rules

Every transformation must satisfy:

1. **No observable side effect is removed.** Calls, static stores, and
   member/index loads survive unconditionally.
2. **No semantic boundary is crossed.** Folding only applies where the
   type checker already established the operation is legal on `Bool`;
   propagation only applies within a block with provable value identity.
3. **Spans and types survive.** A folded rvalue keeps the original
   rvalue's span and type; a propagated read keeps the read's span and
   type. `SymbolId`/`TypeId` relationships are untouched — optimization
   never invents, drops, or renumbers ids.
4. **Determinism.** All iteration is over ordered vectors; there is no
   hash-map iteration in any pass.
5. **Validation.** `validate` runs before the first pass and after every
   pass. Malformed input is rejected up front with structured `MirError`s,
   never a panic.

## 5. Fixpoint and Termination

The pipeline runs passes to a fixpoint (a full round in which nothing
changed) with a cap of 64 rounds as a defensive bound. Because every pass
is monotone — nodes are only removed or rewritten to strictly smaller
forms — the fixpoint is reached in a handful of rounds on any input, and
the cap guarantees termination unconditionally.

## 6. Tests

Coverage lives in `tests/optimization.rs` (38 tests) plus the unit tests
in `src/mir/optimize.rs`, and CLI tests in `tests/cli.rs` (3 new, 61
total):

- **Constant folding**: boolean algebra (and/or/not/eq/ne), logical-not,
  nested combinations, non-constant operands left untouched, non-foldable
  operators preserved, folded results keep the expression's span and type.
- **CFG simplification**: constant-true/false branches become jumps, dead
  arms are removed, non-constant conditions are preserved, while loops
  with constant conditions simplify without breaking the loop.
- **Unreachable elimination**: blocks after `return` are removed,
  infinite loops survive (they are reachable), module items survive.
- **Dead-code elimination**: unused bindings are removed, used bindings
  are kept, calls/member/index loads/static stores are never eliminated,
  compound-assignment results that are read survive (and the binary is
  never removed), parameter uses survive, loop machinery (`RangeNext`/
  `RangeFinished`) is never eliminated.
- **Copy propagation**: copy chains propagate to constants, propagated
  constants keep the read's span, copies into unused locals are removed,
  reassignment kills stale copies.
- **Statics**: initializers fold, values are preserved, assignment to a
  static is never eliminated.
- **Invariants**: validation passes before and after; `SymbolId`/`TypeId`
  relationships survive; optimization is deterministic (repeated runs
  produce byte-identical output); many-function programs optimize
  deterministically.
- **Adversarial/malformed**: malformed input returns structured errors,
  dangling local references are rejected — never a panic.
- **CLI**: a foldable program passes through `mink check` with exit 0 and
  the success message reporting the optimization stage; an early-stage
  error never claims optimization ran; `mink build` compiles the optimized
  MIR through the native backend (session 11, `NATIVE_BACKEND_IMPLEMENTATION.md`).

## 7. Quality Gates

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo build
    git diff --check

Full suite after session 11: **622 tests** (61 CLI + 50 lexer + 88 parser +
62 parser hardening + 72 semantics + 12 source + 122 typecheck + 25 HIR +
34 MIR + 38 optimization + 21 lib unit + 37 backend), all passing. After
session 12 (native runtime foundation) the suite is **654 tests** (39 lib
unit + 14 runtime end-to-end; the backend image-shape assertions were
updated in place; see `RUNTIME_IMPLEMENTATION.md` §12). After session 13
(string + memory types) the suite is **700 tests** (49 lib unit + 42
backend + 24 runtime end-to-end + 143 typecheck; see
`RUNTIME_IMPLEMENTATION.md` §8). After session 14 (structs + arrays) the
suite is **762 tests** (+59 aggregate), and after session 15 (ownership
analysis) it is **803 tests** (+41 ownership), and after session 16
(references and borrowing) it is **878 tests** (+51 references; see
`NATIVE_BACKEND_IMPLEMENTATION.md` §13).
