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

## 24. Specification Status

This document establishes the core language direction. The core grammar for
the implemented milestone — including the keyword list — is frozen in
`docs/language/CORE_GRAMMAR.md`; the remaining language surface (types,
modules, patterns, async, memory semantics, ABI, runtime behavior, compiler
architecture) continues to be finalized in the dedicated technical
specifications before architecture freeze.
