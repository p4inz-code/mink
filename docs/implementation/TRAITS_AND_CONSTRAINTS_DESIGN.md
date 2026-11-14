# Traits and constraints: design record

**Status:** design only — nothing in this document is implemented
**Gate:** M7 Phase C, Gate B (Session 279)
**Normative text:** `docs/language/TYPE_SYSTEM.md` §12 (generic constraints) and
§13 (interfaces and traits)
**Current implementation:** monomorphization, `docs/implementation/GENERICS_IMPLEMENTATION.md`

Gate B asked for a trait/constraint design, not a partial implementation. This
record answers one question — *what would have to be true for MINK to have
constraints, and which of those things are already true* — and records the
decision to defer. It deliberately does not propose a syntax to build now.

---

## 1. What exists today

| Capability | Status | Evidence |
| --- | --- | --- |
| Generic functions, structs, enums over type parameters | Implemented | `src/parser`, `src/monomorphize`, `tests/generics.rs` (42 tests) |
| Instantiation by substitution | Implemented | `Monomorphizer::instantiate_fn`, `instantiate_struct`, `instantiate_enum` |
| Type-argument inference | Partial: literals, typed locals, calls to non-generic functions | `Monomorphizer::infer_type_from_expr` |
| Checking of generic bodies | Per instantiation, after substitution | `E-T02` on `addg(true, false)` (`tests/constraints_conformance.rs`, `B-CHECK`) |
| Bounds, traits, `impl`, `dyn`, where-clauses | None | Parser rejects `trait`/`impl` (`E-P01`) and rejects `<T: Bound>` |
| Uninstantiated generic bodies | Not checked at all | `B-GAP` in `tests/constraints_conformance.rs` |

The important structural fact: **the pipeline is monomorphize-then-check.**
`driver::check_single_module` and `check_multi_module` both run
`monomorphize_reported` on the AST before semantics and type checking, and the
generic declarations are removed from the AST by that pass. There is no stage
that ever sees an abstract `T` in a function body.

## 2. What that means for correctness

Soundness does not depend on constraints in the current design:

- every executed function body has concrete types, so operator, field, and call
  rules apply to real types (the `Bool + Bool` case is caught);
- a call that cannot choose its type arguments is rejected (`E-T44`), so nothing
  reaches the backend with unresolved types;
- the only hole is that a generic function that is *never instantiated* is never
  checked. That is a usability and API-contract gap, not a soundness gap: there
  is no way to call the unchecked body.

This is why Gate B is a design gate and not a defect backlog. The defects the
audit did find were in the *diagnostics* around monomorphization, and they were
fixed (`E-T44`, §5).

## 3. What a constraint system would have to provide

Ranked by what the language already promises:

1. **Checking a generic body once, against its constraints** (§11's "generic
   code must remain statically checked"). Today the body is checked N times for
   N instantiations, and zero times if N is zero. Checking once requires a
   representation of "an unknown type that has these operations".
2. **Expressing required operations** (§12). The two operations the current
   tests need are arithmetic (`+`) and member access (`p.x`). A constraint must
   be able to say "T has `+`" and "T has field `x`".
3. **Polymorphism over unrelated types** (§13). Requires either dynamic dispatch
   (vtable) or a static dispatch mechanism; both need a type-erased or
   dictionary representation that the flat stack machine does not currently
   have.
4. **API contracts** (§13). Requires a stable notion of "public surface" —
   which is exactly what Gate A found missing (`pub` is not enforced and there
   are no re-exports). Constraints without visibility have nothing to attach a
   contract to.
5. **Interoperability** (§13). The runtime boundary is `kernel32` imports with a
   stack convention; nothing there needs traits to work.

## 4. Options considered

| Option | What it buys | Cost | Verdict |
| --- | --- | --- | --- |
| **A. Stay with monomorphization, no constraints** | Everything shipped keeps working; `E-T44` makes the inference limit explicit | Uninstantiated bodies unchecked; no way to state a required operation; error messages can only appear per instantiation | **Chosen for now** |
| **B. Structural constraints (duck typing), checked at instantiation** | No new runtime representation: a constraint is a set of required operations, checked on the concrete copy | Still N checks; a body is only validated for the instantiations that exist; needs a syntax for "has field x"/"has +" | Plausible next step, but it does not close hole 1 and needs syntax the grammar does not have |
| **C. Nominal traits with static dispatch (monomorphized dictionaries)** | Closest to §13; static checking once; fits the no-runtime-overhead requirement in §11 | Large: trait declarations, impl coherence, orphan rules, bound syntax, dictionary passing or specialization in the backend, error codes, migration of stdlib signatures | Real milestone, not a phase gate |
| **D. Traits with dynamic dispatch (vtables)** | §13's polymorphism | Runtime representation the value model does not have (no heap-allocated objects with type tags beyond enums), plus a calling-convention change | Larger than C, and contradicts the current "no hidden allocation" model |
| **E. Partial implementation in this phase** | — | Explicitly excluded by the gate, and would leave the language with syntax that does nothing | Rejected |

Option A is not a claim that MINK should never have traits. It is the position
that a half-built constraint system is worse than none: it would either be
unchecked (syntax that lies) or checked per instantiation (which today's
pipeline already does implicitly, with no syntax to maintain).

## 5. Decision

**Defer traits and constraints to a later milestone. Do not add syntax now.**

Consequences, all recorded so they are not rediscovered as surprises:

- `trait`, `impl`, `dyn`, and `<T: Bound>` stay rejected by the parser. They are
  not silently accepted, so no program depends on them meaning anything.
- The inference limit is documented and diagnosed: `E-T44` names the function
  and asks for explicit `::<Type>` arguments instead of reporting "cannot find
  name".
- The unchecked-uninstantiated-body hole stays documented (`TYPE_SYSTEM.md`
  §12, `GENERICS_IMPLEMENTATION.md`, `B-GAP` test) rather than being papered
  over.
- The prerequisite that is *language-visible* — a public surface to attach API
  contracts to — is the visibility model from Gate A, which is itself deferred
  by design (`CORE_GRAMMAR.md` §30). Constraints and visibility should be
  designed together, not separately.

## 6. Preconditions for revisiting

A future milestone should not start with syntax. It should start by answering:

1. Is the unit of abstraction a *type* (traits) or a *value* (interfaces)? §13
   leaves both open on purpose.
2. Does dispatch have to be static? §11 requires avoiding unnecessary runtime
   overhead, which rules out boxing by default but not dictionary passing.
3. How does a constraint interact with the flat namespace and the missing
   visibility model? An `impl` needs a home for coherence; a single flattened
   namespace has none.
4. What happens to `E-T44`? With bounds, inference failure becomes either a
   defaulted parameter or a clearer "no impl" error; the current diagnostic is
   the placeholder for that.
5. Which error codes are introduced? `E-T*` is the type-error range; a new range
   (`E-C*`?) should be reserved before the first code is emitted.

## 7. What this gate actually changed

Nothing in the language. One diagnostic defect was confirmed and fixed:

| Finding | Kind | Disposition |
| --- | --- | --- |
| A generic call whose type arguments cannot be inferred was reported as `cannot find name \`f\`` — a false statement about a function that exists, with no hint about the fix | Defect (misleading diagnostic) | Fixed: `E-T44` (`TypeErrorKind::CannotInferTypeArguments`) at the call site, suppressions limited to the duplicate unresolved-name error at the same span |
| `trait`/`impl`/bounds rejected with generic parser errors | Not a defect: the syntax is reserved and unimplemented | Documented here and in `TYPE_SYSTEM.md` |
| Uninstantiated generic bodies are not checked | Known limitation | Documented and pinned by test |

The fix does not move MINK toward a trait system; it removes a diagnostic that
would have made a future one harder to explain.
