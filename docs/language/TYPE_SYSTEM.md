# MINK — Type System Specification

**Status:** Planning / Specification
**Version:** 0.1.0

## 1. Goals

MINK uses a strong, statically checked type system designed to provide high correctness without unnecessary verbosity.

The type system must balance:

- Safety
- Expressiveness
- Compile-time error detection
- Runtime performance
- Readability
- Interoperability
- Developer ergonomics

## 2. Type Inference

MINK should infer types whenever the compiler can determine the result unambiguously.

Explicit annotations remain available when they improve readability, API clarity, generic constraints, or compiler diagnostics.

Inference must never silently change the semantic meaning of a program.

## 3. Primitive Types

MINK should provide well-defined primitive types for:

- Boolean values
- Signed integers
- Unsigned integers
- Floating-point values
- Unicode scalar values
- Bytes

The exact widths and aliases will be finalized with the runtime and ABI specification.

## 4. Numeric Safety

Numeric conversions should be explicit when they can lose information, overflow, or change interpretation.

The compiler should detect statically provable numeric errors where practical.

Runtime arithmetic behavior must be defined rather than silently relying on undefined behavior.

## 5. Composite Types

MINK must support user-defined composite types.

The core model should support:

- Struct-like records
- Enumerations
- Tagged unions / sum types
- Tuples
- Generic collections
- Function types

Composite types should have predictable memory and equality semantics.

## 6. Structs

Struct-like types represent related data fields under one type.

They should support:

- Named fields
- Methods
- Visibility control
- Generic parameters
- Construction
- Pattern matching where appropriate

## 7. Enumerations

Enumerations should represent a closed set of named alternatives.

**Implemented (sessions 17, 19, and 20):** an enum is a nominal type
declared `enum E { A, B }` whose values are named alternatives
constructed with variant paths (`E::A`) and, for data-carrying variants,
variant calls (`E::B(expr)`). Variants may declare an **explicit
discriminant** (`A = 5`, Session 20); otherwise the discriminant is the
previous variant's value plus one, starting at 0. A unit-only enum value
occupies a single word, copies freely, and never participates in
ownership/move analysis; an enum with a data-carrying variant (Session
19) is a tagged union — a discriminant word plus a payload area sized for
the largest variant — whose payloads may own heap values and therefore
move on transfer. Enum equality (`==`/`!=`) requires the same enum type
and produces `Bool` for unit-only enums; tagged-union equality is
`E-T30`. There is no ordering, arithmetic, or `Int` conversion. Enum
names share the type namespace with struct names. Diagnostics: duplicate
enum `E-S15`, duplicate variant `E-S16`, variant path on a non-enum
`E-T22`, undeclared variant `E-T23`, invalid payload type `E-T27`, payload
mismatch `E-T28`, payload arity `E-T29`, tagged-union equality `E-T30`,
duplicate discriminant `E-T31`, discriminant overflow `E-T32`. See
`docs/implementation/ENUM_TYPES_IMPLEMENTATION.md` (Session 17),
`docs/implementation/SUM_TYPES_IMPLEMENTATION.md` (Session 19), and
`docs/implementation/DISCRIMINANTS_IMPLEMENTATION.md` (Session 20).

**Deferred:** enum-to-`Int` conversion, deriving, and generics over
enums.

## 8. Sum Types

MINK should provide a type-safe mechanism for representing values that may belong to one of several alternatives.

**Implemented (session 19):** data-carrying enum variants are the
mechanism: `enum Option { Some(Int), None }` declares a closed set of
alternatives, exactly one of which a value holds at a time — the
compiler-computed discriminant word selects the active variant and a
shared payload area holds its payload. Construction is `Option::Some(5)`;
pattern matching integrates directly with compiler exhaustiveness
analysis (a variant is covered only when its payload's alternatives are
covered, `E-T24`/`E-T25`); owned payloads participate in ownership/move
analysis. See `docs/implementation/SUM_TYPES_IMPLEMENTATION.md`.

**Deferred:** tuple payloads (multiple payload values) and tagged-union
equality. (Explicit discriminants are implemented — session 20, see
`docs/implementation/DISCRIMINANTS_IMPLEMENTATION.md`.)

## 9. Optional Types

Absence is represented explicitly by an optional type.

Optional values must not be implicitly treated as valid non-optional values.

The compiler should diagnose unsafe unwrapping whenever statically possible.

Optional handling should have concise syntax so safe code remains ergonomic.

## 10. Result Types

MINK should provide a standard result abstraction for operations that may succeed or fail.

A result should represent at least:

- Successful value
- Failure/error value

The language should provide concise propagation syntax so error-aware code does not become excessively verbose.

## 11. Generics

Generics are a fundamental part of MINK rather than an optional advanced feature.

Generic code must remain statically checked.

The compiler should select an implementation strategy appropriate to the target and optimization requirements.

The implementation must avoid unnecessary runtime overhead where specialization or other safe optimization is possible.

## 12. Generic Constraints

MINK should support constraints on generic parameters.

Constraints must allow generic code to express the operations and capabilities it requires without depending on concrete implementations.

The final constraint mechanism may use interfaces, traits, protocols, or another equivalent abstraction.

## 13. Interfaces and Traits

MINK should provide a mechanism for describing shared capabilities across unrelated concrete types.

The mechanism must support:

- Generic constraints
- Polymorphism
- API contracts
- Interoperability
- Compile-time checking

The final implementation model remains an architecture decision.

## 14. Type Aliases

MINK should support aliases for existing types where aliases improve API readability or domain modeling.

An alias should not accidentally create a distinct incompatible type unless explicitly declared as a newtype or equivalent construct.

## 15. Newtypes

MINK should provide a lightweight mechanism for creating semantically distinct types based on an existing representation.

Newtypes should allow developers to prevent accidental mixing of values that share the same underlying representation.

Example conceptual use cases include:

- User identifiers
- File paths
- Currency values
- Network ports
- Database identifiers

## 16. Function Types

Functions are values and may be stored, passed, returned, and composed.

Function types must include their parameter and result types.

Closures must be able to capture values according to the language memory/resource model.

## 17. Type Compatibility

Two types should be compatible only when their relationship is explicitly defined by the type system.

Structural compatibility, nominal compatibility, or a combination may be used where justified.

The final model must prioritize predictability and long-term maintainability.

## 18. Implicit Conversions

Implicit conversions must be conservative.

Conversions that may lose information or introduce ambiguity should require explicit syntax.

Safe, obvious conversions may be implicitly supported if doing so improves ergonomics without weakening type safety.

## 19. Type Narrowing

The compiler should be able to refine a value type after proven checks.

For example, checking an optional value for presence should allow subsequent code to operate on the proven-present value where control flow guarantees that state.

Type narrowing must remain sound across branches, loops, closures, and asynchronous boundaries.

## 20. Pattern Matching

Pattern matching is a core type-system feature.

It should support:

- Literal patterns
- Enum patterns
- Sum-type patterns
- Destructuring
- Guards where appropriate
- Optional/result matching

The compiler should detect non-exhaustive matches when exhaustive analysis is possible.

## 21. Type Safety and Runtime Checks

MINK should reject statically provable invalid operations at compile time.

Runtime type checks remain available when information is unavailable statically, especially at interoperability and dynamic-data boundaries.

Runtime checks must produce structured failures rather than undefined behavior.

## 22. Type Erasure and Dynamic Values

MINK may provide controlled dynamic-value mechanisms for interoperability, reflection, serialization, scripting boundaries, and external data.

Dynamic values must remain clearly distinguishable from statically typed values.

Crossing from dynamic data into strongly typed structures should provide validation and actionable diagnostics.

## 23. Variance

Generic variance rules must be explicit and sound.

The compiler must prevent unsafe substitution of generic types.

The exact variance model will be finalized alongside the generic implementation strategy.

## 24. Type Reflection

MINK should provide controlled type metadata capabilities where useful for serialization, tooling, debugging, dependency injection, and framework development.

Reflection must not impose unavoidable overhead on programs that do not use it.

## 25. ABI and FFI Interaction

Foreign-function interfaces must provide explicit type mappings between MINK and external representations.

Unsafe or lossy mappings must be visible and diagnosable.

The C ABI will be a primary interoperability boundary.

## 26. Type Diagnostics

Type errors are a major part of the MINK diagnostic experience.

Diagnostics should identify:

- The invalid operation
- The expected type
- The received type
- Where each type originated
- Relevant generic constraints
- The most likely root cause
- Practical fixes

When multiple errors share one root cause, the compiler should identify the root error and suppress or group misleading cascading errors where possible.

## 27. Type-System Performance

Type checking must remain scalable for large codebases.

The compiler architecture should support incremental analysis and caching where practical.

Complex generic code should not cause disproportionate compile-time costs without clear diagnostics.

## 28. Type-System Stability

Type semantics are part of MINKs long-term compatibility contract.

Once stabilized, fundamental type behavior should evolve conservatively.

## 29. Open Technical Decisions

The following remain architecture-level decisions:

- Exact primitive widths
- Nominal vs structural type composition
- Trait/interface implementation model
- Generic specialization strategy
- Runtime representation of dynamic values
- Reflection implementation
- ABI representation
- Exact optional syntax
- Exact result syntax

These must be resolved before architecture freeze.
