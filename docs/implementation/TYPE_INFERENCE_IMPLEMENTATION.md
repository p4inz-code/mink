# MINK — Type Inference Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 07 — Advanced Type Features / Type Inference

## 1. Objective

Session 06 established the type-system foundation: a canonical type
representation, inference variables, unification, expression typing, and
the `E-T01`…`E-T06` diagnostics. Session 07 strengthens inference:

- inference variables resolve transitively, propagate constraints, detect
  incompatible constraints, and terminate deterministically;
- expected types flow into expressions where the context determines the
  answer (bidirectional checking), so determinable types never leak as
  unresolved;
- function parameter and result types are inferred from usage and return
  expressions, including recursion and mutually constrained calls;
- return paths are validated against each other with precise diagnostics;
- `Error` remains the cascade-control mechanism: unresolved names produce
  no meaningless secondary inference errors.

No generics, structs, enums, traits, closures, HIR/MIR, backend, or
runtime work was introduced. This document records the inference model and
the decisions actually resolved; the underlying type representation and
typing rules remain in `docs/implementation/TYPE_SYSTEM_IMPLEMENTATION.md`.

## 2. Inference Model

The checker is a **single forward pass** over the AST (it never re-runs
name resolution or scope construction). Every declared symbol receives a
type slot before any body is analyzed (`pre_register`): function symbols
get a real `Fn` type whose parameters and result are inference variables,
and every other symbol gets a fresh inference variable. Constraints are
then accumulated by unifying, exactly as session 06 established:

- a declaration's variable unifies with its initializer's type;
- a reference reuses the symbol's variable;
- a call's arguments unify with the callee's parameter variables and the
  call produces the callee's result variable;
- a `return` expression unifies with the enclosing function's result
  variable.

Inference variables form a **union-find structure**: `unify` links an
unconstrained variable to the other type and path-compresses walked chains,
so constraints propagate through arbitrarily long chains in amortized
near-constant time, and the same variable can be constrained from many
places (body, call sites, returns) with all constraints forced to agree.

### 2.1 Type states

| State      | Meaning                                                            |
| ---------- | ------------------------------------------------------------------ |
| Concrete   | A known type (`Int`, `Float`, …, `Range<Int>`, `fn(Int) -> Bool`)  |
| Unresolved | An unconstrained `Infer(None)` — nothing is known yet              |
| Error      | Something already went wrong; operations on it stay quiet          |

`TypeTable::is_resolved(id)` answers whether a type is fully determined
(following resolved variables does not end at an unresolved one); the error
type counts as resolved, since it is a known, deliberate outcome. Tests use
it to assert that no determinable type leaks unresolved.

## 3. Constraint Propagation

Because all constraints unify against shared variables, propagation is
order-independent across the module (declaration types are order
independent like names) and across call sites:

    let a = b; let b = c; const c = 1;   // a → b → c
    fn f() { a + 1; }                    // pins the whole chain to Int

A 200-link declaration chain ending in a concrete type resolves the head
through path compression. Incompatible constraints are detected wherever
they meet:

    fn f(p) { p + 1; }   fn g() { f(1); f(1.5); }

the body pins `p` to `Int`, and the second call site's `Float` argument
conflicts (`E-T01`, span of `1.5`, expected `Int`, found `Float`).

## 4. Bidirectional Checking

The checker types expressions bottom-up (`expr_type`) and, where the
context determines the type, **top-down**: `check_expr_against(expr,
expected)` types the expression and unifies its type with `expected`,
pinning an unconstrained expression and reporting `E-T01` on conflict. The
directions the current language supports:

| Context                    | Expected type                |
| -------------------------- | ---------------------------- |
| `if` / `while` condition   | `Bool`                       |
| `for` iterable             | `Range<T>` (fresh `T`)       |
| `&&` / `\|\|` operands     | `Bool`                       |
| `<<` / `>>` / `&` / `^` / `\|` operands | `Int`            |
| `!` operand                | `Bool`                       |
| `~` operand                | `Int`                        |

These pins mean, for example, that `fn f() { return; } fn g() { if f() { } }`
determines `f`'s result type as `Bool` (the condition is the only
constraint and it is unambiguous), and that `let r = f(); for i in r { i +
1; }` resolves `r` to `Range<Int>` through the pinned element variable.

Genuinely ambiguous positions are **not** pinned — guessing would be
wrong:

- `-` accepts both `Int` and `Float`;
- arithmetic on two unconstrained operands has no determined operand type;
- comparison/equality produce `Bool` regardless of the (any scalar) operand
  types.

These stay unresolved until a real constraint decides (see §5).

## 5. Unresolved-Type Behavior

An unresolved type is the honest answer when nothing determines the type.
It is never fabricated into a concrete type, and it never produces an error
by itself — only a constraint that contradicts a *resolved* requirement
errors. This keeps programs that are merely under-constrained valid while
invalid programs (incompatible constraints) are rejected precisely.

`Error` types are the cascade-control mechanism (session 06 §8): an
unresolved name, an invalid operator result, or an unknown callee poisons
everything downstream, and the pinning paths above skip error types, so the
root semantic/type error is never doubled by secondary inference noise.

## 6. Function and Return Inference

- **Parameter inference.** Parameters are variables shared between the
  body and call sites; any constraint — an operator in the body
  (`p + 1` → `Int`), a condition (`if f(true)` → `Bool`), a call-site
  argument (`f(1.5)` → `Float`) — pins them, and later conflicting
  constraints are `E-T01`/`E-T02`.
- **Result inference.** A function's result variable is unified by every
  typed `return` expression, so `fn f() { return 1; }` is `fn() -> Int`
  and `fn f(c) { if c { return 1; } return 2; }` is `fn(Bool) -> Int`.
  Conflicting returns (`return 1; return 1.5;`) are `E-T01` at the second
  return with the first return's type as expected. Bare `return;` carries
  no value and contributes nothing (session 06 behavior, unchanged).
- **Recursion.** A function's result variable unifies with the result of
  its own calls (`return f(n - 1);`), which is the identity constraint;
  recursion resolves once a base path pins the result (`return 0;`).
  Parameter inference flows through recursive calls exactly like any other
  call.
- **Mutual constraints.** `fn f(p) { return g(p); } fn g(q) { return q; }`
  shares parameter and result variables across both functions, so one
  call-site argument (`f(1)`) resolves both signatures to `fn(Int) -> Int`.

## 7. Tests

Coverage lives in `tests/typecheck.rs` (122 tests total; 28 added in
session 07) and four new CLI tests in `tests/cli.rs`. Session-07 inference
categories:

- **Chains and ordering**: chained declarations resolved through use,
  mutually constrained declarations, a deep 200-link chain (path
  compression).
- **Function inference**: parameter inference from body and arguments,
  return inference from single paths and across branches, recursion,
  mutual recursion, mutually constrained calls, argument-driven and
  result-driven resolution.
- **Returns**: conflicting returns across branches (`E-T01` with
  expected/actual types and exact span).
- **Pins**: unconstrained conditions → `Bool`; pinned conditions
  conflicting with numeric use (`E-T02`); logical/shift/bitwise operands
  → `Bool`/`Int`; `!` → `Bool`; `~` → `Int`; `-` deferral on ambiguous
  operands; unknown iterables → `Range<T>`; pinned ranges propagating to
  loop variables and back.
- **Unresolved behavior**: `no_determinable_type_leaks_unresolved`
  (asserts `is_resolved` for every determinable symbol), genuine ambiguity
  staying unresolved, error-type blocking of further constraints,
  independent inference conflicts all reported.
- **Adversarial**: hand-built ASTs exercising the pin paths with
  unresolved identifiers — no panics, no type noise.

CLI tests verify exit codes and stderr for a recursive well-typed program
(exit 0), incompatible call constraints (exit 1, `E-T01`), conflicting
returns (exit 1, `E-T01`), and pinned-condition conflicts (exit 1,
`E-T02`).

## 8. Known Limitations

- Inference is a single forward pass; ordering is only order-independent
  where the language is (module scope, shared variables). A variable that
  is unconstrained at first use adopts the requirement of its first
  constraint; the pinned bidirectional directions are the only
  expected-type propagation. A general bidirectional constraint solver is
  a later milestone.
- Genuinely ambiguous positions (`-`, arithmetic or comparison/equality on
  two unconstrained operands) stay unresolved rather than being guessed.
- Function signatures are inferred from usage; there is no signature
  syntax, no generics, and no closures.
- A function with no typed `return` keeps an unresolved result type; calls
  through it defer honestly.
- No implicit conversions: incompatible concrete types always conflict
  (`Int` vs `Float`), matching the no-conversion decision of session 06.

## 9. Quality Gates

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo build
    git diff --check

Full suite after session 07: **458 tests** (52 CLI + 50 lexer + 88 parser +
62 parser hardening + 72 semantics + 12 source + 122 typecheck), all
passing.
