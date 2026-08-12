# MINK — Type System Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 06–07 — Type System Foundation & Type Inference

## 1. Objective

The parser establishes *"this source is syntactically valid"*, semantic
analysis establishes *"this syntactically valid program is semantically
coherent"* (names, scopes, mutability, control-flow context), and the type
system establishes *"this program is well-typed"*: every expression has a
deterministic type, declarations and identifiers carry types, operators and
assignments are checked against real typing rules, and calls are checked
for arity and argument compatibility.

At the end of this session the following works end to end:

    MINK source → SourceFile → Lexer → Parser → AST
        → Semantic Analysis → Type Analysis → diagnostics

`mink check <path>` now performs lexical, syntactic, **semantic**, and
**type** analysis. The type result — the type of every expression and every
symbol, plus type diagnostics — is a durable structure for the future HIR
stage. No HIR, MIR, code generation, or runtime work was introduced.

## 2. Architecture

The type checker lives in `src/typecheck/`:

| File        | Role                                                            |
| ----------- | --------------------------------------------------------------- |
| `mod.rs`    | `check(&Ast, &SemanticResult) -> TypeResult`; public result API |
| `error.rs`  | `TypeErrorKind` (`E-T01`…`E-T06`) + `TypeError`                 |
| `ty.rs`     | `TypeId`, `TypeKind`, `TypeTable` (arena, interning, unification) |
| `checker.rs`| Single-pass traversal: expression typing, operator rules, calls  |

The pipeline:

    AST ────────────────────────────┐
    Semantic Result (SymbolId, spans, resolutions) ─┐
                                                 v
                                         Type Checker
                                           +-- Symbol type pre-registration
                                           +-- Literal typing
                                           +-- Expression typing
                                           +-- Operator typing
                                           +-- Assignment / call checking
                                           +-- Range / control-flow typing
                                           +-- Type diagnostics
                                                 v
                                            Type Result
                                                 |
                                                 v
                                         Future HIR

The checker consumes the session-05 [`SemanticResult`] directly and never
re-runs name resolution or scope construction. The semantic/type boundary
is strict: names, scopes, declarations, mutability, and control-flow context
belong to semantic analysis; the type checker only reads that information.

## 3. Type Representation

Types live in a small arena ([`TypeTable`]) addressed by stable numeric
ids ([`TypeId`]). [`TypeKind`] is the kind of a type:

| Kind                  | Meaning                                                    |
| --------------------- | ---------------------------------------------------------- |
| `Int`                 | Integer values (a single integer type; widths are a runtime/ABI decision, `docs/language/TYPE_SYSTEM.md` §3) |
| `Float`               | Floating-point values                                      |
| `Bool`                | Boolean values                                             |
| `Char`                | A single Unicode scalar value                              |
| `Str`                 | A string of Unicode scalar values                          |
| `Null`                | The `null` literal; a distinct concrete type               |
| `Range(element)`      | `start..end` / `start..=end` over values of `element`      |
| `Fn { params, result }` | A function type                                          |
| `Infer(Option<TypeId>)` | An inference variable; `Some(target)` once resolved       |
| `Error`               | The unknown/error type (cascade control, §8)               |

Concrete types are **interned**: pushing `Int` twice returns the same id,
so type identity is cheap and two occurrences of the same literal type
compare equal by identity. Inference variables are never interned — each
is a distinct mutable slot.

`TypeTable` is the single owner and exposes the canonical view:

- `canonical(id)` follows resolved inference variables to the type they
  currently denote;
- `kind(id)` returns the canonical kind;
- `display(id)` renders a human-readable name (`Int`, `Range<Int>`,
  `fn(Int) -> Bool`, `unresolved`, `unknown`);
- `unify(a, b)` is the single authoritative compatibility mechanism (§5).

## 4. Core Types

The core type catalog is exactly what the current milestone requires
(`docs/language/CORE_LANGUAGE.md` §26). No speculative types were added:
`Option`/`Result`, tuples, structs, enums, generics, traits, union types,
ownership types, and every other advanced form in `docs/language/TYPE_SYSTEM.md`
remain future milestones.

Type identity is the interned [`TypeId`]; equality is canonical identity
through [`TypeTable::canonical`] and [`TypeTable::unify`]. Compatibility is
defined by the operator and unification rules below.

### 4.1 Null

`null` is a literal in the frozen grammar with its own distinct `Null`
type. It is **not** a bottom type: it unifies with nothing except itself.
MINK's optional/absence mechanism is a future type-system milestone; `Null`
is the honest type of the literal that exists today, and operations on it
follow the ordinary rules (only `null == null` / `null != null` are
well-typed).

## 5. Type Equality, Compatibility, and Unification

[`TypeTable::unify`] is the single authoritative type-comparison mechanism;
operators and the checker never scatter ad-hoc comparisons. Unify answers:

- *are these types identical?* — canonical ids equal;
- *are these types compatible?* — unify succeeds;
- *can this value be assigned to this type?* — the assignment path unifies
  the value with the target's type.

Rules, in order:

1. Identical canonical types unify to themselves.
2. The **error type absorbs any type**: the result is the error type, and
   an unconstrained inference variable is linked to the error type so
   later uses of that variable stay silently unknown (cascade control, §8).
3. An **inference variable adopts the other type** (union-find style);
   two unconstrained variables link to each other.
4. Structurally equal composite types unify recursively: `Range` against
   `Range` unifies the element types; `Fn` against `Fn` requires the same
   parameter count and unifies parameters pairwise plus the results.
5. Anything else is a conflict and returns the two canonical types, which
   the caller renders as a diagnostic.

Unification is deterministic and guarded (never panics on valid ids);
recursion is bounded by type nesting depth, which the current grammar
cannot make deep (nested ranges are rejected by the range rule, and
function types do not nest).

## 6. Inference Variables and the Inference Boundary

The current grammar has no type annotations, so the checker performs the
**minimum inference the language requires** (§28 of the session brief):

- every declared symbol starts with a fresh inference variable;
- a declaration's variable unifies with its initializer's type;
- a reference simply reuses the symbol's variable, so constraints propagate
  across uses;
- function parameters are fresh variables shared between the body (usage
  constraints) and call sites (argument unification);
- a function's result is a variable unified by its `return` expressions;
- an unconstrained operand of a binary/range/unary operation adopts the
  operator's requirement when the other operand fixes it (`x + 1` makes an
  unknown `x` an `Int`; `y && true` makes an unknown `y` a `Bool`);
- two unconstrained binary operands link to each other and produce the
  operator's result type: the linked variable for operand-typed operators
  (arithmetic, shift, bitwise) and `Bool` for comparison, equality, and
  logical operators, whose result is `Bool` regardless of operand types.

Session 07 added the **bidirectional direction** (expected types flowing
into expressions) where the context determines the answer — see
`docs/implementation/TYPE_INFERENCE_IMPLEMENTATION.md`:

- `if`/`while` conditions are checked against the expected type `Bool`
  (`check_expr_against`), so an unconstrained condition is pinned to
  `Bool` instead of leaking unresolved;
- `for` iterables are pinned to `Range<T>` with a fresh element variable;
- `&&`/`||` pin both operands to `Bool`; `<<`/`>>`/`&`/`^`/`|` pin both
  operands to `Int` (both-unknown operands no longer leak);
- `!` pins its operand to `Bool` and `~` to `Int`;
- `-` and arithmetic/comparison/equality on unconstrained operands are
  genuinely ambiguous and stay unresolved until a real constraint decides
  (documented limitation, §23).

Three type states are clearly distinguished:

| State      | Meaning                                                       |
| ---------- | ------------------------------------------------------------- |
| Concrete   | A known type (`Int`, `Float`, …, `Range<Int>`, `fn(Int) -> Bool`) |
| Unresolved | An unconstrained `Infer(None)` — nothing is known yet         |
| Error      | Something already went wrong; operations on it stay quiet     |

The full future inference engine (bidirectional inference, generic
unification with type parameters, constraint solving) is out of scope; the
variable/unification design deliberately leaves room for it without
rewriting the checker.

## 7. Type Environment

The type environment is `symbol_types`, a vector indexed by
[`SymbolId::raw()`] matching the session-05 [`SymbolTable`]: one type slot
per declared symbol, so `SymbolId → Type` is answered in O(1) without
reconstructing declaration information. The symbol table itself is never
duplicated.

| Consumer                        | API                                   |
| ------------------------------- | ------------------------------------- |
| Symbol → type                   | `TypeResult::symbol_type(SymbolId)`   |
| Expression span → type          | `TypeResult::expr_type(Span)` (binary search over a span-sorted map) |
| All expression types            | `TypeResult::expr_types()` (span order) |
| Type identity / display         | `TypeResult::types()` (`TypeTable`)   |

Expression types are keyed by the expression's exact source span: every
expression node covers a unique span within a file, so spans are stable
node identities without mutating the AST.

## 8. The Unknown/Error Type

The `Error` type is the cascade-control mechanism:

- unresolved identifiers, calls through unknown callees, and failed
  sub-expressions produce `Error`;
- every operation with an `Error` operand silently produces `Error`;
- declarations initialized from `Error` become `Error`;
- assignments into `Error` targets silently unify.

The result: one root error (for example `E-S01` from an unresolved name, or
a single invalid operator) never cascades into a swarm of meaningless
secondary diagnostics, while invalid programs can never silently pass
beyond their actual errors — every *root* violation is still reported, and
dependent constructs are marked unknown for later stages.

## 9. Literal Typing

| Literal   | Type    |
| --------- | ------- |
| Integer   | `Int`   |
| Float     | `Float` |
| String    | `Str`   |
| Char      | `Char`  |
| `true`/`false` | `Bool` |
| `null`    | `Null`  |

The checker answers *"what type does this literal have?"* directly from the
AST node kind; the literal's raw text is not needed (no decoding, matching
the AST design). There are no implicit conversions at this stage.

## 10. Declaration Typing

For `let x = e;`, `let mut x = e;`, and `const x = e;` the binding's
variable unifies with the type of `e`; mutability remains entirely a
semantic-analysis property ([`SymbolKind::Let { mutable }`]). Because
variables are pre-registered for every symbol before any body is analyzed,
module-scope order independence carries over to types: `let b = a; const a
= 1;` gives both `a` and `b` the type `Int` regardless of order, and
mutually recursive functions type-check.

`for` variables unify with the iterable's element type (`for i in 0..10`
makes `i` an `Int`).

## 11. Identifier Typing

An identifier expression is typed through [`SemanticResult::resolve`],
which maps the identifier's span to its [`SymbolId`] — name resolution is
never re-run. The type is the symbol's environment slot, so `let x = 10; x;`
gives the second `x` exactly the type the declaration established, and
shadowing resolves to the innermost symbol's own slot.

## 12. Expression Typing

Every expression is typed and recorded under its span:

| Expression      | Rule                                                            |
| --------------- | --------------------------------------------------------------- |
| Literals        | §9                                                              |
| Identifiers     | §11                                                             |
| Group `(e)`     | the inner expression's type                                     |
| Unary `-`       | numeric operand → operand type; else error                      |
| Unary `!`       | `Bool` → `Bool`; else error                                     |
| Unary `~`       | `Int` → `Int`; else error                                       |
| Binary          | §13                                                             |
| Assignment      | §15                                                             |
| Range           | §16                                                             |
| Call            | §17                                                             |
| Member / Index  | §18 (deferred)                                                  |

## 13. Operator Typing

The checker groups the frozen operator set into categories with exactly one
rule each (`docs/language/CORE_LANGUAGE.md` §26):

| Category     | Operators        | Valid operands          | Result  |
| ------------ | ---------------- | ----------------------- | ------- |
| Arithmetic   | `+ - * / %`      | same numeric type       | operand |
| Shift        | `<< >>`          | `Int` and `Int`         | `Int`   |
| Bitwise      | `& ^ \|`         | `Int` and `Int`         | `Int`   |
| Comparison   | `< <= > >=`      | same numeric type       | `Bool`  |
| Equality     | `== !=`          | same scalar type        | `Bool`  |
| Logical      | `&& \|\|`        | `Bool` and `Bool`       | `Bool`  |

Scalar types are `Int`, `Float`, `Bool`, `Char`, `Str`, and `Null`. Range
and function types are not comparable. An unconstrained operand adopts the
operator's requirement when the other operand fixes it (§6); where both
operands are unconstrained, the logical operators pin them to `Bool` and
the shift/bitwise operators pin them to `Int` (session 07), while
comparison/equality (result `Bool`) and arithmetic (result is the operand
type) cannot pin and leave the linked operands unresolved. An `Error`
operand poisons the result silently (§8).

### 13.1 Numeric mixing — documented decision

MINK defines **no implicit numeric conversions at this stage**. Mixed
integer/float operations (`1 + 2.5`), comparisons, and equality are
rejected rather than silently coerced. This follows the session brief's
instruction (§17) and the specification's conservatism about implicit
conversions (`docs/language/TYPE_SYSTEM.md` §18): no conversion rules are
defined yet, so none are invented. Integer/float arithmetic will land with
an explicit conversion design in a later milestone.

### 13.2 Logical operations

There is **no truthiness**: `&&`, `||`, and `!` require `Bool` operands
and reject everything else (`1 && true` is an error). This matches
`CORE_LANGUAGE.md` §26 and avoids inventing truthiness semantics.

## 14. Control-Flow Typing

- `if` and `while` conditions are checked against the expected type `Bool`
  (`E-T01` on conflict); an unconstrained condition is **pinned to `Bool`**
  (session 07), while unknown/error conditions defer silently.
- `return expr;` unifies the expression with the enclosing function's
  result variable, so a function's result type is inferred from its return
  expressions and multiple paths must agree; conflicting return types are
  `E-T01`. Bare `return;` contributes nothing.
- `for` iterables must be `Range` types (`E-T06` otherwise); an
  unconstrained iterable is **pinned to `Range<T>`** with a fresh element
  variable (session 07), and the loop variable unifies with the element
  type.
- `break`/`continue` are control-flow only; they have no type.

## 15. Assignment Typing

The semantic stage owns target writability (§19 of the session brief) and
the type checker adds **only** type compatibility:

- for an identifier target, the target's writability is read from the
  semantic symbol; a non-mutable target (immutable `let`, `const`,
  parameter, `for` variable, function name) **skips the type check
  entirely** so the immutable-assignment diagnostic (`E-S03`/`E-S04`) is
  never doubled by a misleading cascade;
- `=` requires the value to unify with the target's type (`E-T01` with the
  expected type from the target, the actual type from the value, the value
  span as primary, and the target span as a related location);
- compound assignments (`+= -= *= /= %=`) apply the corresponding binary
  rule and report the **compound** symbol (`E-T02`, e.g. `+=`);
- member/index targets are deferred (§18) — their base and index
  expressions are still typed.

## 16. Range Typing

Both endpoints must be the same numeric type (`E-T03` otherwise); the
result is `Range<endpoint>`. Mixed numeric endpoints (`0..1.5`) and
non-numeric endpoints (`0.."a"`) are rejected. Open-ended ranges, iterator
semantics, and collection semantics were deliberately not added.

## 17. Call Typing

Call checking is real but bounded by what the current type model supports:

- the callee must resolve to a function type, otherwise the call is either
  `E-T04` (a known non-function value, e.g. `x(2)` where `x: Int`), or
  deferred (an unresolved/unknown callee — the semantic stage already
  reports unresolved names, and unknown callees produce a fresh
  unconstrained result instead of a fabricated one);
- the argument count must match the declared parameter count (`E-T05`);
- each argument unifies with its parameter (`E-T01` on conflict) — because
  parameters are inference variables shared with the body, a call can
  conflict with a constraint the body imposed (`fn f(p) { p + 1; }` then
  `f(true)` is an error);
- the call's type is the function's result, propagated to the caller.

Function types carry parameter and result types; there is no closure,
generic, or higher-order function typing at this stage.

## 18. Member / Index Boundary

Member access and indexing depend on user-defined types, which do not
exist yet. Both are **deferred honestly**: the base (and index) expressions
are typed, and the member/index expression itself gets a fresh
unconstrained inference variable — never silently accepted as a specific
type, never a fabricated error. The existing session-05 deferral of
member/index assignment writability therefore remains in force, and
`o.f = 2; arr[0] = 3;` type-checks without inventing collection semantics.

## 19. Diagnostics

Type diagnostics follow the established model (stable code, human-readable
message, exact span) and reserve the next stable range after the semantic
range: `E-T01` … `E-T06`.

| Code | Kind               | Message (example)                                        |
| ---- | ------------------ | -------------------------------------------------------- |
| E-T01| `TypeMismatch`     | `expected \`Int\`, found \`Bool\`` (+ related target span on assignment) |
| E-T02| `InvalidOperator`  | `cannot apply operator \`+\` to types \`Int\` and \`Float\`` |
| E-T03| `InvalidRange`     | `cannot construct a range with operands of types \`Int\` and \`Str\`` |
| E-T04| `NotCallable`      | `cannot call a value of type \`Int\``                    |
| E-T05| `WrongArgumentCount` | `expected \`1\` arguments, found \`2\``                |
| E-T06| `NotIterable`      | `cannot iterate over a value of type \`Int\``            |

Every error carries the exact offending span; `E-T01` on assignment also
carries the target span as a related location, which the CLI renders as
`note: related location is here`. Diagnostics are deterministic and source
ordered (errors are sorted by span start in the result, and the driver
merges lexical/syntax/semantic/type errors into one source-ordered
report).

### 19.1 Cascade control

- Unresolved names produce exactly their semantic `E-S01` and poison the
  type of everything downstream (`E-T*` silence);
- an immutable-assignment semantic error is never doubled by a type
  mismatch;
- a chain of dependent invalid operations reports its root (`true + true +
  true + true` reports one `E-T02`);
- independent errors are all reported (two independent incompatible
  assignments report two `E-T01`s);
- lexical or syntax errors suppress semantic and type analysis entirely.

## 20. Semantic Integration

`driver::check` runs type analysis whenever parsing succeeded, whether or
not semantic analysis reported errors: the checker consumes the semantic
result directly, and its `Error` type keeps semantic errors from producing
type noise. `CheckReport` now carries `semantic: Option<SemanticResult>`
and `types: Option<TypeResult>`; `CheckError` gained a `Type(TypeError)`
variant rendered uniformly by the CLI. Exit codes: valid → `0`; lexical,
syntax, semantic, or type error → `1`; unreadable file → `1`; `mink build`
remains `NotImplemented` (exit `2`). The CLI success message is now
`passed parsing, semantic analysis, and type checking (N tokens)`.

## 21. Performance

- One forward pass; every node visited exactly once.
- Symbol→type lookup is O(1) via the `SymbolId`-indexed vector; expression
  lookup is O(log n) binary search over a span-sorted vector.
- Concrete types are interned, so literal/type identity is cheap and the
  arena stays small.
- Canonicalization follows at most one union-find chain per lookup and
  `unify` path-compresses the chains it walks (session 07), so long
  declaration/inference chains (hundreds of linked variables) resolve in
  amortized near-constant time.
- No AST mutation, no global state, no unsafe code.

## 22. Security and Robustness

Compiler input is untrusted. The checker:

- never panics on parser-produced ASTs (all lookups guarded) and tolerates
  unusual hand-built AST shapes (literal/group assignment targets, literal
  callees);
- has no unbounded recursion beyond AST depth (nested ranges and function
  types cannot grow deep under the current grammar);
- allocates only in proportion to source size;
- keeps no global mutable state.

## 23. Known Limitations

- No implicit numeric conversions: mixed integer/float operations are
  rejected until a conversion design lands (documented decision, §13.1).
- Member/index typing is deferred until user-defined types exist (§18).
- Function signatures are inferred from usage; there is no explicit
  signature syntax, no generic function typing, no closures.
- A function with no typed `return` keeps an unresolved result type; calls
  through it defer honestly.
- Inference is a single forward pass: an unconstrained variable adopts the
  requirement of its first constraint, and the bidirectional directions
  (conditions → `Bool`, iterables → `Range<T>`, boolean/integer operators
  → their operand type) are the only expected-type propagation. Genuinely
  ambiguous uses — `-`, arithmetic or comparison/equality on two
  unconstrained operands — stay unresolved until a later constraint or a
  future constraint solver decides (§26 of `TYPE_INFERENCE_IMPLEMENTATION.md`).
- Equality is allowed for the six scalar types only; range and function
  equality are rejected until their semantics are specified.
- `null` is its own concrete type, not an optional/absence mechanism
  (deferred).

## 24. Tests

Coverage lives in `tests/typecheck.rs` (122 tests) and the type CLI smoke
tests in `tests/cli.rs` (16 type tests). Session 07 added the inference
categories in `docs/implementation/TYPE_INFERENCE_IMPLEMENTATION.md` §7.
Categories:

- **Literals**: integer/float/string/char/bool/null typing and expression
  recording.
- **Identifiers**: declaration typing, reference typing, nested and
  shadowed references, unresolved → error type, module binding visibility.
- **Declarations**: inferred `let`, `let mut`, `const`, propagation,
  module order independence, `for` variable element typing.
- **Operators**: valid and invalid arithmetic, logical, comparison,
  equality, bitwise, shift, and unary operations, including the
  mixed-numeric rejection and no-truthiness rule.
- **Assignment**: valid, incompatible (`E-T01` with expected/actual/related
  spans), immutable/const single-error behavior, compound assignment,
  chains, member/index deferral.
- **Calls**: valid calls, arity (`E-T05`), non-callable values (`E-T04`),
  argument conflicts (`E-T01`), result propagation, recursion, deferred
  unknown callees, unresolved callee silence.
- **Control flow**: consistent and conflicting returns, condition typing,
  unknown-condition deferral, non-range iterables (`E-T06`).
- **Ranges**: int/float/inclusive range types, mixed and non-numeric
  endpoint rejection.
- **Error cascades**: unknown symbols, unknown symbols inside expressions,
  multiple independent errors, semantic+type combinations, error-type
  propagation, bounded operator chains.
- **Type environment**: symbol→type map, identity of equal types, distinct
  types, inference resolution, expression lookup by span.
- **Robustness**: deep nesting, 200 functions, 300-term chains, hand-built
  unusual ASTs, unresolved names everywhere — no panics.
- **Inference (session 07)**: chained/mutually constrained declarations,
  deep 200-link chains, parameter/return inference, recursion, mutual
  recursion, argument-driven and result-driven resolution, conflicting
  returns and incompatible constraints, pinning (conditions, iterables,
  logical/shift/bitwise/unary operands), no-leak assertions via
  `is_resolved`, genuinely ambiguous deferral, error-type blocking, and
  hand-built pin-path ASTs.

The CLI tests verify actual exit codes and stderr content for valid
programs (exit 0), type errors (exit 1), mixed semantic+type sources, and
the updated success message; session 07 added inference CLI tests for a
recursive program, incompatible call constraints, conflicting returns, and
pinned-condition conflicts.

## 25. Quality Gates

As before:

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo build
    git diff --check

Total suite after session 07: **458 tests** (52 CLI + 50 lexer + 88 parser
+ 62 parser hardening + 72 semantics + 12 source + 122 typecheck), all
passing.

## 26. Later Milestones

The type-system foundation (sessions 06–07) deliberately stops before
advanced features. Cleanly deferred to later milestones:

- implicit conversions and numeric promotion rules;
- user-defined types (structs, enums, tuples) and member/index typing;
- generics, traits/interfaces, type aliases, optional/result types;
- a general bidirectional inference engine beyond the pinned
  expected-type directions of session 07 (see
  `docs/implementation/TYPE_INFERENCE_IMPLEMENTATION.md` §6);
- pattern matching and exhaustiveness;
- HIR/MIR lowering, optimization, code generation, runtime.
