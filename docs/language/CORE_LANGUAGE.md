# MINK — Core Language Specification

**Status:** Planning / Specification
**Version:** 0.1.0

## 1. Source Model

MINK source files use the `.mink` extension.

A MINK program consists of one or more modules. A source file normally represents one module.

The language should support both tiny single-file programs and large multi-module applications without requiring different language modes.

## 2. Program Structure

A MINK source file may contain:

- Module declarations
- Imports
- Constants
- Type declarations
- Functions
- Variables
- Implementations
- Tests
- Compile-time declarations

The core grammar implemented by the current compiler milestone is frozen in
`docs/language/CORE_GRAMMAR.md`; it covers declarations (`fn`, `let`, `let
mut`, `const`), statements, control flow, and expressions. The remaining
categories above arrive with their dedicated milestones (type system,
module system, pattern matching, and so on).

## 3. Lexical Model

MINK source is Unicode-aware.

Identifiers should support Unicode where technically safe, while ASCII remains the recommended convention for public APIs and interoperability.

Whitespace is generally insignificant except where required to separate tokens.

Comments must support:

- Single-line comments
- Multi-line comments

Documentation comments should be supported as structured metadata for tooling and generated documentation.

## 4. Identifiers

Identifiers represent names of declarations, types, variables, functions, modules, fields, and other language entities.

Identifiers must:

- Be unambiguous to the lexer
- Avoid collisions with reserved keywords
- Preserve source spelling for diagnostics and tooling
- Support predictable Unicode normalization

Public naming conventions should be documented by the standard library rather than enforced unnecessarily by the compiler.

## 5. Keywords

MINK maintains a deliberately small reserved-keyword set.

Keywords are reserved only when their special syntactic meaning justifies
reservation. The keyword list was frozen together with the core grammar in
session 03 (`docs/language/CORE_GRAMMAR.md` §10):

`async await break const continue else enum false fn for if impl in let loop
match mod mut null pub return struct trait true type unsafe use while`

The boolean and null literals (`true`, `false`, `null`) are reserved words
classified as literals rather than keywords.

## 6. Literals

MINK should provide first-class literals for:

- Integers
- Floating-point values
- Booleans
- Characters or Unicode scalar values
- Strings
- Byte sequences
- Arrays/collections where practical
- Null/absence representation

Literal syntax must remain readable and predictable.

Numeric literals should support readable separators and explicit bases where useful.

## 7. Variables and Constants

MINK distinguishes mutable variables from immutable bindings.

The language should prefer immutability by default where doing so improves correctness without creating unnecessary friction.

Mutation must remain straightforward when required.

Constants represent values that can be established at compile time when their expressions are compile-time evaluable.

## 8. Type System

MINK uses a strong, statically checked type system.

The type system must support:

- Primitive types
- User-defined types
- Struct-like data types
- Enumerations
- Interfaces/traits or equivalent abstractions
- Generics
- Function types
- Collection types
- Optional/absence types
- Result/error types
- Type inference

The compiler should infer types whenever the result is unambiguous while allowing explicit annotations whenever developers need clarity or control.

## 9. Type Safety

Invalid type operations should normally be rejected at compile time.

Implicit conversions should be limited to conversions that are predictable and safe.

Potentially lossy or ambiguous conversions should require explicit syntax.

## 10. Null and Absence

MINK should not rely on an unrestricted nullable value model.

Absence should be represented explicitly through an optional/nullable type mechanism.

The compiler should detect unsafe attempts to use an absent value wherever statically possible.

The exact optional syntax will be defined during type-system specification.

## 11. Expressions

Expressions are composable units that produce values or controlled effects.

MINK expressions should support:

- Literals
- Names
- Function calls
- Operators
- Conditional expressions
- Collection construction
- Member access
- Indexing
- Lambda/closure expressions
- Type construction

Expression evaluation should have deterministic semantics.

## 12. Operators

MINK should provide conventional operators for:

- Arithmetic
- Comparison
- Equality
- Boolean logic
- Assignment
- Bitwise operations
- Range construction
- Null/optional handling

Operator overloading should be possible only where it improves readability and remains predictable.

User-defined operators must not be allowed to create arbitrary confusing syntax.

## 13. Functions

Functions are first-class language constructs.

Functions should support:

- Named parameters
- Return types
- Type inference where appropriate
- Default parameters where useful
- Generic parameters
- Closures
- Higher-order functions
- Async functions
- Explicit error propagation

Functions should remain concise for simple tasks while supporting advanced signatures for complex systems.

## 14. Control Flow

MINK should provide clear constructs for:

- Conditional branching
- Pattern matching
- Loops
- Iteration
- Early return
- Error propagation
- Structured resource cleanup

Pattern matching should be designed as a major correctness and expressiveness feature rather than merely syntactic sugar.

## 15. Modules and Imports

MINK uses explicit modules.

Imports should be deterministic, discoverable, and tooling-friendly.

The module system must support:

- Local modules
- Package modules
- Standard-library modules
- External dependencies
- Aliases
- Visibility control

Circular dependencies should be detected and reported with actionable diagnostics.

## 16. Visibility

MINK should provide explicit visibility boundaries for APIs.

The language should distinguish private implementation details from public interfaces.

Visibility should be simple enough for small projects while providing strong encapsulation for large systems.

## 17. Generics

MINK requires generic programming support.

Generics should provide reusable, type-safe abstractions without forcing runtime overhead where the compiler can specialize safely.

The final implementation strategy will be selected during compiler and runtime architecture.

## 18. Collections

MINK should provide efficient standard collection types for common workloads.

At minimum, the standard library should eventually provide equivalents for:

- Dynamic arrays
- Fixed-size arrays
- Maps/dictionaries
- Sets
- Queues
- Stacks
- Ordered collections

Collection APIs should be generic and type-safe.

## 19. Error Model Boundary

Ordinary recoverable failures must be representable as values rather than requiring exceptions for normal control flow.

The exact error/result model will be specified in `ERROR_SYSTEM.md`.

Compiler errors and runtime errors are distinct concepts and must be represented differently.

## 20. Compile-Time Intelligence

MINK should make substantial use of compile-time analysis to prevent errors before execution.

Potential capabilities include:

- Type checking
- Exhaustiveness checking
- Constant evaluation
- Data-flow analysis
- Dead-code detection
- Unreachable-code detection
- Ownership/resource analysis where applicable
- API compatibility analysis
- Security-oriented diagnostics

The exact compiler architecture is intentionally deferred.

## 21. Runtime Semantics

Runtime behavior must be deterministic and precisely specified wherever practical.

Undefined behavior should be minimized aggressively.

If an operation cannot safely produce a valid result, MINK should prefer a defined failure mode or compile-time rejection over silent undefined behavior.

## 22. Low-Level Escape Hatches

MINK must support advanced native/system programming without forcing the entire language into an unsafe programming model.

Controlled unsafe or low-level capabilities may exist behind explicit syntax and tooling boundaries.

Unsafe operations should be visible to developers and analyzable by tooling.

## 23. Source Compatibility

Valid MINK programs should remain source-compatible across language evolution whenever technically possible.

Compiler migrations, deprecation diagnostics, and automated upgrade tools should be part of the long-term ecosystem.

## 24. Declaration and Name Semantics (Session 05)

Resolved in session 05 and authoritative for the implemented milestone; the
full design record is `docs/implementation/SEMANTIC_ANALYSIS_IMPLEMENTATION.md`.

- **Scopes.** Module scope holds top-level declarations. Every function body
  is the function's *declaration scope*: parameters and the body's own
  `let`/`const` declarations share one scope. Every other block (`if`/`else`
  bodies, `while`, `for`, `loop` bodies) introduces a nested block scope.
  `for` loop variables are declared in the loop body's scope.
- **Declaration order.** Module scope is order-independent: a top-level
  declaration is visible throughout its module, before and after its own
  position (functions may call each other in any order). A consequence is
  that a module-level binding is visible in its own initializer
  (`let x = x;` at module scope resolves the initializer to the binding
  itself). All other scopes require declaration-before-use: a name is
  visible from its declaration point to the end of its scope, and a binding
  is not visible in its own initializer.
- **Duplicates.** A scope may not declare the same name twice. A duplicate
  declaration is an error; the first declaration wins for resolution.
  Because parameters share the function scope with body declarations, a
  parameter/local name collision is a duplicate.
- **Shadowing.** A nested scope may declare a name that exists in an
  enclosing scope (shadowing); references resolve to the innermost
  declaration. Same-scope shadowing (redeclaration) is not allowed.
- **Mutability.** `let` bindings are immutable by default; `let mut`
  bindings are mutable. Parameters, `for` variables, `const` bindings, and
  function names are immutable. Assignment (including compound assignment)
  to an immutable or constant name is a semantic error. Assignment through
  member/index targets resolves the base expression but defers target
  writability to the type-system milestone.
- **Control-flow context.** `break` and `continue` are valid only inside a
  loop body (`while`, `for`, `loop`); `return` is valid only inside a
  function body. Out-of-context uses are semantic errors (module-level
  `return` is additionally rejected by the grammar, which allows only
  declarations at module scope).
- **Namespaces.** Functions, bindings, constants, parameters, and loop
  variables share one name namespace per scope at this stage; a type/value
  namespace split arrives with the type-system milestone.

## 25. Specification Status

This document establishes the core language direction. The core grammar for
the implemented milestone — including the keyword list — is frozen in
`docs/language/CORE_GRAMMAR.md`; the semantic rules implemented for the
current milestone are recorded in §24 above, and the type-system decisions
in §26. The remaining language surface (modules, patterns, async, memory
semantics, ABI, runtime behavior, compiler architecture) continues to be
finalized in the dedicated technical specifications before architecture
freeze.

## 26. Type-System Decisions (Session 06)

Resolved in session 06 and authoritative for the implemented milestone; the
full design record is `docs/implementation/TYPE_SYSTEM_IMPLEMENTATION.md`.

- **Core types.** The current milestone defines exactly the scalar types
  `Int`, `Float`, `Bool`, `Char`, `Str`, and `Null`, plus `Range<T>`, the
  pointer type `Ptr<T>`, and function types. `Int` and `Float` are single
  types; exact widths are a runtime/ABI decision. Only `Ptr<Int>` is
  instantiable today (the raw memory intrinsics' word pointer); `Unit`
  types the value-less intrinsic results. No other types exist yet (no
  tuples, structs, enums, generics, optional/result types, …) — they
  arrive with later milestones per `docs/language/TYPE_SYSTEM.md`.
- **Literals.** Integer, floating-point, string, character, boolean, and
  `null` literals have the corresponding types above.
- **No implicit conversions.** MINK defines no implicit numeric
  conversions at this stage: mixed integer/float operations, comparisons,
  equality, and ranges are rejected rather than silently coerced
  (`1 + 2.5` is an error). Conversions arrive with an explicit design.
- **Null.** `null` has its own distinct `Null` type; it is not a bottom
  type and unifies only with itself. The optional/absence mechanism is a
  future milestone.
- **No truthiness.** `&&`, `||`, and `!` require `Bool` operands; `if` and
  `while` conditions must be `Bool`. Non-boolean operands are type errors.
- **Operators.** Arithmetic (`+ - * / %`) requires the same numeric type;
  bitwise and shift (`& ^ | << >>`) require `Int`; comparisons require the
  same numeric type; equality (`== !=`) requires the same scalar type
  (`Int`, `Float`, `Bool`, `Char`, `Str`, `Null`); ranges require the same
  numeric type for both endpoints. Result types are the operand type for
  arithmetic, `Bool` for comparisons/equality/logical, and `Range<T>` for
  ranges.
- **Declarations.** `let`, `let mut`, and `const` bindings are typed from
  their initializers (the grammar has no annotations). Mutability is a
  semantic property and is unchanged by typing. Module-scope declaration
  types are order-independent like their names.
- **Calls.** Calls check that the callee is callable, that the argument
  count matches the declared parameters, and that each argument is
  compatible with its parameter. Function parameter and result types are
  inferred from usage at this stage; there is no signature syntax yet.
- **Strings and pointers (session 13).** A `Str` value is the address of
  a length-prefixed UTF-8 byte blob; string literals are immutable blob
  data, and `rt_str_alloc`/`rt_str_free`/`rt_str_len`/`rt_str_byte`/
  `rt_str_set_byte`/`rt_print_str` operate on them (indices are
  bounds-checked, `E-R09`). `Ptr<Int>` is produced by `rt_alloc` and
  consumed by `rt_free`/`rt_mem_load`/`rt_mem_store`. Pointer arithmetic
  is byte-addressed (`p + n`, `n + p`, `p - n`); pointer equality is
  `Bool`. Strings and pointers are distinct: neither satisfies the other's
  intrinsic parameters (`E-T01`). The integer literal `0` is the null
  pointer constant in pointer-typed argument positions only — a computed
  `Int` is never a pointer. No ownership/borrow checking, no raw pointer
  syntax, and no string concatenation yet.
- **Iteration.** Only ranges are iterable at this stage; a `for` variable
  has the range's element type, and iterating a non-range is a type error.
- **Member/index deferral.** Member access, indexing, and their
  writability depend on user-defined types, which do not exist yet; they
  are deferred (never silently accepted as a specific type, never a
  fabricated error).
- **Type diagnostics.** Type errors use the stable range `E-T01`…`E-T06`
  (mismatch, invalid operator, invalid range, not callable, wrong argument
  count, not iterable). They carry the exact offending span, rendered
  expected/actual types where useful, and a related span for assignments.
- **Cascade control.** An unknown/error type absorbs failed sub-expressions
  so one root error (an unresolved name, an invalid operator) never
  cascades into misleading secondary diagnostics; independent errors are
  still all reported.

### 26.1 Inference Decisions (Session 07)

- **Constraint model.** Inference is constraint-based over the union-find
  inference variables of session 06: declaration chains, recursion, and
  mutually constrained calls resolve transitively and deterministically;
  unification path-compresses chains. There is no separate constraint
  solver.
- **Bidirectional checking.** Where the context determines the type, the
  expected type flows into the expression: `if`/`while` conditions pin
  their expression to `Bool`; `for` iterables pin to `Range<T>`; `&&` and
  `||` pin both operands to `Bool`; `<<`, `>>`, `&`, `^`, and `|` pin both
  operands to `Int`; `!` pins its operand to `Bool`; `~` pins its operand
  to `Int`.
- **No guessing.** Positions with several valid types are never guessed:
  `-`, and arithmetic or comparison/equality on two unconstrained
  operands, stay unresolved until a real constraint decides. An unresolved
  type by itself is never an error; only a constraint that contradicts a
  resolved requirement is.
- **Return inference.** A function's result type is inferred from its
  typed `return` expressions; multiple return paths must agree, and
  conflicting returns are `E-T01` at the conflicting return. Bare `return;`
  carries no value and contributes nothing.
- **Recursion.** Recursive calls unify a function's result with itself
  (an identity constraint); a function's type resolves once any path
  provides a concrete constraint. Mutually constrained functions share
  parameter/result variables and resolve together.
- **Resolution test.** `TypeTable::is_resolved` reports whether a type is
  fully determined (not an unresolved variable). The checker is expected
  to leave no determinable type unresolved: every pinned context above
  resolves its variables.
