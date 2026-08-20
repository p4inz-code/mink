# MINK — Core Grammar (Frozen)

**Status:** Implementation
**Version:** 0.1.0
**Session:** 03 — Parser + AST

## 1. Purpose

This document freezes the MINK core grammar implemented by the parser in
session 03. It is the authority the implementation follows; `CORE_LANGUAGE.md`
describes the language direction and remains authoritative for everything not
settled here.

The grammar was frozen deliberately at this point because the session 02
lexer deliberately kept the keyword and operator set provisional until the
grammar was designed. Any change to this grammar must be justified against the
design rules (`docs/core/DESIGN_DECISION_RULES.md`) and recorded.

## 2. Scope

This milestone freezes **syntax only** for the core language constructs
needed by a minimal function-based program:

- top-level declarations (`fn`, `let`, `const`)
- blocks and statements
- control flow (`if`/`else`, `while`, `for`/`in`, `loop`, `break`,
  `continue`, `return`)
- expressions: literals, identifiers, unary/binary operators, assignment,
  ranges, calls, member access, indexing, grouping

Constructs the specifications describe but that belong to later milestones
are **not** part of this grammar and are rejected with parser diagnostics.
Notable exclusions:

- Type declarations (`type`, `trait`, `impl`) — type-system milestone
  (`struct` and `enum` declarations arrived in sessions 14 and 17; see
  §11 and §12)
- Module system (`mod`, `use`, `pub`) — module-system milestone
- Async/await and unsafe — concurrency/unsafe milestones
- Type annotations on bindings (`let x: Type = expr;`) arrived in
  session 26 — §19 (function parameter and return type annotations
  arrived in session 25 — §17)
- Tuples (see §21)
- Lambdas/closures
- `?` (optional handling), open-ended ranges (`a..`, `..b`), and bare `..`
  operators
- Byte literals, raw strings (not lexed)

These exclusions are additive: later sessions extend the grammar without
redesigning the parser structure.

## 3. Notation

`x?` optional, `x*` zero or more, `x+` one or more, `(a | b)` choice,
`x,` a trailing-comma-tolerant separator.

## 4. Lexical Summary

Trivia (whitespace, `//` line comments, `/* ... */` block comments) separates
tokens and is not part of the grammar. Keywords, literals, and operators are
the token set defined in `docs/implementation/LEXER_IMPLEMENTATION.md`, with
one change made during the grammar freeze: `mut` was added to the keyword set
(see §10).

Identifiers are ASCII `[A-Za-z_][A-Za-z0-9_]*` (Unicode identifiers remain a
documented lexer limitation). Keywords are reserved: they cannot be used as
identifiers.

## 5. Program Structure

```
Program    := Item*
Item       := Fn | LetBinding | ConstBinding
```

A source file is a sequence of top-level declarations. Module scope holds
declarations only; executable statements live inside function bodies.
Empty statements (`;`) between items are accepted silently. Semicolons are
**required** to terminate statements (explicit terminators are preferred
over newline sensitivity for determinism and tooling friendliness).

## 6. Declarations

```
Fn          := 'fn' Ident '(' ParamList? ')' Block
ParamList   := Param (',' Param)* ','?
Param       := Ident

LetBinding  := 'let' 'mut'? Ident '=' Expr ';'
ConstBinding:= 'const' Ident '=' Expr ';'
```

- `let` bindings are immutable by default; `let mut` introduces a mutable
  binding (see `CORE_LANGUAGE.md` §7).
- `const` names a compile-time-constant binding; whether its initializer is
  actually evaluated at compile time is a semantics milestone concern.
- Parameters are bare identifiers in this grammar; type annotations arrive
  with the type-system milestone.

## 7. Blocks and Statements

```
Block       := '{' Stmt* '}'
Stmt        := LetBinding | ConstBinding
             | Return | Break | Continue
             | If | While | For | Loop
             | Expr ';'
Return      := 'return' Expr? ';'
Break       := 'break' ';'
Continue    := 'continue' ';'
If          := 'if' Expr Block ('else' (If | Block))?
While       := 'while' Expr Block
For         := 'for' Ident 'in' Expr Block
Loop        := 'loop' Block
```

- `else` must be followed by a block or a further `if` (else-if chains nest).
- A `;` alone is an empty statement and is accepted silently.
- Expression statements must end with `;`.
- Whether `break`/`continue` appear inside a loop is a semantic question
  (resolved in session 05: they are rejected outside loop bodies, and
  `return` is rejected outside function bodies). The syntax accepts them
  anywhere; see `docs/language/CORE_LANGUAGE.md` §24 and
  `docs/implementation/SEMANTIC_ANALYSIS_IMPLEMENTATION.md`.

## 8. Expressions

```
Expr        := Assign
Assign      := RangeExpr (AssignOp Assign)?          // right-associative
AssignOp    := '=' | '+=' | '-=' | '*=' | '/=' | '%='
RangeExpr   := LogicalOrExpr ('..' RangeExpr)?       // right-associative
             | LogicalOrExpr ('..=' RangeExpr)?
LogicalOr   := LogicalAnd ('||' LogicalAnd)*
LogicalAnd  := BitOr ('&&' BitOr)*
BitOr       := BitXor ('|' BitXor)*
BitXor      := BitAnd ('^' BitAnd)*
BitAnd      := Equality ('&' Equality)*
Equality    := Relational (('==' | '!=') Relational)*
Relational  := Shift (('<' | '<=' | '>' | '>=') Shift)*
Shift       := Additive (('<<' | '>>') Additive)*
Additive    := Multiplicative (('+' | '-') Multiplicative)*
Multiplicative := Unary (('*' | '/' | '%') Unary)*
Unary       := ('-' | '!' | '~') Unary | Postfix
Postfix     := Primary (('(' ArgList? ')') | ('.' Ident) | ('[' Expr ']'))*
ArgList     := Expr (',' Expr)* ','?
Primary     := Int | Float | Str | Char | Bool | Null | Ident | '(' Expr ')'
```

### 8.1 Operator precedence (lowest to highest)

| Level | Operators | Associativity |
| ----- | --------- | ------------- |
| Assignment | `=` `+=` `-=` `*=` `/=` `%=` | right |
| Range | `..` `..=` | right |
| Logical or | `\|\|` | left |
| Logical and | `&&` | left |
| Bitwise or | `\|` | left |
| Bitwise xor | `^` | left |
| Bitwise and | `&` | left |
| Equality | `==` `!=` | left |
| Relational | `<` `<=` `>` `>=` | left |
| Shift | `<<` `>>` | left |
| Additive | `+` `-` | left |
| Multiplicative | `*` `/` `%` | left |
| Unary | `-` `!` `~` (prefix) | — |
| Postfix | call, `.` member, `[...]` index | — |

This follows the conventional C-family ordering, chosen for predictability
(`CORE_LANGUAGE.md` §12). Examples:

- `a + b * c` == `a + (b * c)`
- `(a + b) * c` == grouping, kept as an explicit AST node
- `a - b - c` == `(a - b) - c`
- `a = b = c` == `a = (b = c)`
- `a == b < c` == `a == (b < c)` (equality binds looser than relational)
- `a | b ^ c & d` == `a | (b ^ (c & d))`
- `a << b + c` == `a << (b + c)`
- `1 + 2..3 * 4` == `(1 + 2)..(3 * 4)` (range binds looser than arithmetic)

### 8.2 Assignment targets

The left side of an assignment must be a place: an identifier, a member
access, or an index expression. Anything else is a syntax error
(`E-P04`). Grouped places (`(x) = y`) are not places.

### 8.3 Ranges

Ranges require both operands: `start..end` (exclusive) and `start..=end`
(inclusive). Open-ended ranges are deferred until the collection/type
milestones justify them.

## 9. What Is Not the Grammar

The following are **not** part of the frozen grammar and produce parser
errors if used, until their milestones land: `type`/`trait`/`impl`
declarations (struct and enum declarations arrived in sessions 14 and 17
— see §11 and §12), `mod`/`use`/`pub`, `async fn`/`await`, `unsafe`
blocks, closures, and the `?` operator.
Block expressions and `if` as an expression arrived in session 28;
tuples arrived in session 29 (see §22). (see §21).
Return-type annotations and parameter type annotations arrived in session 25
(see §17).

## 10. Grammar-Freeze Changes to the Lexer

Freezing the grammar required one minimal lexer change:

- **`mut` became a keyword.** `let mut x = 1;` is the frozen mutability
  syntax. `mut` previously lexed as an identifier; the session 02 keyword set
  was explicitly provisional pending the grammar design, so this is the
  intended freeze point. The change is a one-entry addition to the keyword
  table plus a `TokenKind::Mut` variant; identifiers like `mutt` and `_mut`
  are unaffected.

No other lexical forms changed. `:` gained a use in session 14 as the
struct-field type-annotation separator (see §11); `::` gained one in
session 17 as the variant-path separator (see §12); and `=>` gained one in
session 18 as the match-arm separator (see §13). `->` (return type) and
`?` (optional handling) remain lexed but unused by this grammar; the
parser rejects them with diagnostics (verified by regression tests in
`tests/parser_hardening.rs`), as documented in
`docs/implementation/LEXER_IMPLEMENTATION.md`.

## 11. Session-14 Additions: Structs and Arrays

**Session:** 14 — Aggregate types (structs + arrays)

This section extends the frozen grammar additively. The base grammar above
remains authoritative; the new productions are:

```
Item       := Fn | LetBinding | ConstBinding | StructDecl
StructDecl := 'struct' Ident '{' FieldList? '}'
FieldList  := Field (',' Field)* ','?
Field      := Ident ':' Type

Type       := Ident | '[' Type ';' IntLit ']'
```

- **Struct declarations** (`struct P { x: Int, y: Int }`) are top-level
  items. The field list may be empty syntactically (an empty struct is
  rejected by layout checking, `E-T18`). The `:` after the field name is
  the type-annotation token, used here for the first time.
- **Type syntax** has exactly two forms today: a struct name (`P`) and a
  fixed-size array type (`[Int; 3]`). The array length must be an integer
  literal `>= 1` (`E-T16` otherwise). `Type` appears only in struct field
  declarations; parameters and bindings still have no annotations.

Expressions gain two primary forms:

```
Primary     := ... | StructLit | ArrayLit
StructLit   := Ident '{' FieldInitList '}'
FieldInit   := Ident ':' Expr
ArrayLit    := '[' ExprList ']'
```

- **Struct literals** (`P { x: 1, y: 2 }`) are primary expressions that
  resolve `Ident` as a struct type name and require every declared field
  exactly once. Because `Ident '{'` also opens a block, a struct literal
  in a condition/iterable position (`if P { ... }`, `while P { ... }`,
  `for x in P { ... }`) must be parenthesized — `(P { ... })` — to parse
  as a literal.
- **Array literals** (`[1, 2, 3]`, trailing comma allowed) are primary
  expressions; their length is the element count. `[...]` in postfix
  position remains indexing, so `a[0]` is unchanged.
- Member access (`.field`) and indexing (`[i]`) remain postfix operators;
  assignment targets may now be struct members and index expressions (a
  place), matching the base grammar's `E-P04` rule.

The session-13 lexer already produced `:` and `,` tokens; no lexical
changes were required. Exclusions from §2 and §9 that remain (enums,
`type` aliases, generics, parameter/return annotations) are
unchanged.

## 12. Session-17 Additions: Enums

**Session:** 17 — Enum types

This section extends the frozen grammar additively. The base grammar and
session-14 additions above remain authoritative; the new productions are:

```
Item       := ... | EnumDecl
EnumDecl   := 'enum' Ident '{' VariantList? '}'
VariantList:= Variant (',' Variant)* ','?
Variant    := Ident

Primary    := ... | EnumVariantPath
EnumVariantPath := Ident '::' Ident
```

- **Enum declarations** (`enum E { A, B }`) are top-level items. The
  variant list may be empty syntactically (an empty enum has no
  constructible values) and a trailing comma is allowed.
- **Type syntax** now also accepts an enum name (`E`), exactly like a
  struct name; enum names appear in struct field types and resolve to
  the nominal enum type.
- **Variant paths** (`E::A`) are primary expressions: `Ident '::' Ident`.
  The `::` token already existed in the lexer. A path with no variant
  name (`E::`) is `E-P22` (expected a variant name). Only enum types
  have variants; `Struct::Field` is rejected by type analysis (`E-T22`),
  and an undeclared variant is `E-T23`.

The session-17 lexer needed no changes (`::` and `,` already existed).
Exclusions from §2 and §9 that remain (`type` aliases, generics,
parameter/return annotations, data-carrying variants) are unchanged.

## 13. Session-18 Additions: Pattern Matching

**Session:** 18 — Pattern matching

This section extends the frozen grammar additively. The base grammar and
the session-14/17 additions above remain authoritative; the new
productions are:

```
Stmt       := ... | MatchStmt
MatchStmt  := 'match' Expr '{' MatchArm* '}'
MatchArm   := Pattern '=>' Block
Pattern    := IntLit | '-' IntLit | BoolLit | Ident '::' Ident | Ident
```

- **Match statements** (`match e { 1 => { .. } _ => { .. } }`) are
  statements: they dispatch on the scrutinee `Expr` and run the block of
  the first arm whose pattern matches. `=>` is the match-arm separator,
  used here for the first time. Arms are separated by commas (a trailing
  comma is allowed); the body of each arm is a block, so braces are
  required around it.
- **Patterns** have four forms this session: an integer literal (`1`,
  and negative literals `-1`), a boolean literal (`true`/`false`), an
  enum variant path (`E::V`), and a bare identifier, which binds the
  scrutinee (a catch-all `_` binds nothing and matches everything).
  Wildcard `_` is an identifier pattern that never binds a name.
- The scrutinee may be any expression. Type analysis decides whether its
  type is matchable: `Int`, `Bool`, and enums are matchable; every other
  type (structs, arrays, strings, pointers, references) is rejected with
  `E-T26` (see `docs/implementation/PATTERN_MATCHING_IMPLEMENTATION.md`).
- A missing `=>` after a pattern is `E-P24` (expected `=>`); a non-pattern
  token where a pattern is required (e.g. a string literal) is `E-P23`.

The session-18 lexer needed no changes (`=>`, `,`, `{`, `}` already
existed). Exclusions from §2 and §9 that remain (`type` aliases,
generics, parameter/return annotations) are unchanged.

## 14. Session-19 Additions: Data-Carrying Variants

**Session:** 19 — Data-carrying enum variants (sum types)

This section extends the frozen grammar additively. The base grammar and
the session-14/17/18 additions above remain authoritative; the new
productions are:

```
Variant    := Ident | Ident '(' Type ')'

Primary    := ... | EnumVariantPath | EnumVariantCall
EnumVariantCall := Ident '::' Ident '(' Expr ')'

Pattern    := ... | Ident '::' Ident '(' Pattern ')'
```

- **Variant declarations** may carry exactly one payload type:
  `enum Shape { Circle(Int), Nothing }`. `Variant()` with no payload is
  `E-P25` (empty payload); `Variant(A, B)` with more than one payload is
  a parse error (the `,` is `E-P14`, expected `)`). Unit and
  data-carrying variants mix freely, and the trailing-comma rule is
  unchanged.
- **Construction** is the variant path followed by a parenthesized
  payload expression: `E::V(expr)`. `E::V()` is `E-P25`; more than one
  argument is a parse error. The parenthesized form is a construction,
  not a call — a variant is never callable as a function.
- **Payload patterns** are the variant path followed by a parenthesized
  payload pattern: `E::V(x)`, `E::V(_)`, `E::V(5)`, `E::V(E2::X)`, and
  nested combinations. `E::V()` is `E-P25`.

The session-19 lexer needed no changes (`(`, `)`, `,` already existed).
Exclusions from §2 and §9 that remain (`type` aliases, generics,
parameter/return annotations) are unchanged.

## 15. Session-20 Additions: Explicit Discriminants

**Session:** 20 — Explicit enum discriminants

This section extends the frozen grammar additively. The base grammar and
the session-14/17/18/19 additions above remain authoritative; the new
production is:

```
Variant    := Ident ('(' Type ')')? ('=' IntLit)?
IntLit     := Int | '-' Int
```

- **Variant declarations** may declare an explicit discriminant after the
  variant name and any payload type: `enum E { A = 5, B, C(Int) = 10, D }`.
  The value must be an integer literal in the language's literal forms
  (decimal, `0x`/`0o`/`0b`, `_` separators), optionally negated
  (`A = -1`). A missing, non-integer, or float value is `E-P19` (expected
  an integer literal), with recovery to the next variant boundary.
- **Discriminants are constants.** There is no `A = 5 + 1` expression
  syntax; the arithmetic operator is a parse error.
- **Implicit continuation** is not a syntax change: a variant without
  `= n` gets the previous variant's value plus one (starting at 0), so
  `enum E { A, B }` is unchanged from sessions 17/19.

The session-20 lexer needed no changes (`=`, `-`, and integer literals
already existed). Exclusions from §2 and §9 that remain (`type`
aliases, generics, parameter/return annotations) are unchanged.

## 16. Status

This grammar is frozen for the constructs it covers. Statements and
declarations outside it are rejected by the parser with stable diagnostics
(see `docs/implementation/PARSER_IMPLEMENTATION.md` for the error codes).
Future sessions extend this document additively.

## 17. Session-25 Additions: Function Signature Type Annotations

**Session:** 25 — Function signature type annotations

This section extends the frozen grammar additively. The base grammar and
the session-14/17/18/19/20 additions above remain authoritative; the new
productions are:

```
Fn          := 'fn' Ident '(' ParamList? ')' ('->' Type)? Block
Param       := Ident (':' Type)?
```

- **Function declarations** may now carry an optional return-type
  annotation: `fn f() -> Int { ... }`. When present, every `return expr;`
  in the body must produce a value of the annotated type (`E-T01` on
  mismatch). When absent, the return type is inferred from `return`
  expressions (existing behavior).
- **Parameters** may now carry optional type annotations: `fn f(x: Int)
  { ... }`. A parameter with a declared type enforces that the
  parameter's usage matches the declared type (`E-T01` on mismatch). A
  parameter without an annotation has its type inferred from usage
  (existing behavior).
- **Mixed annotations** are supported: `fn f(x: Int, y) -> Int { ... }`
  has `x` declared as `Int`, `y` inferred, and the return type declared
  as `Int`.
- All type forms are accepted in annotations: `Int`, `Float`, `Bool`,
  `Char`, `Str`, `Null`, user-declared struct and enum names, `Ptr<T>`,
  `&T`, `&mut T`, and `[T; N]`.

The session-25 lexer needed no changes (`->`, `:`, and type tokens
already existed). Exclusions from §2 and §9 that remain (`type`
aliases, generics) are unchanged.

## 18. Session-25 Additions: `Null` as a Named Type

**Session:** 25

`Null` is now recognized as a named type in type annotations (`fn f() ->
Null { ... }`). Previously only `Int`, `Float`, `Bool`, `Char`, and `Str`
were recognized as named types in type syntax; `Null` was accessible only
through the `null` literal. This change is additive and does not break
existing programs.

## 19. Session-26 Additions: Let-Binding Type Annotations

**Session:** 26 — Let-binding type annotations

This section extends the frozen grammar additively. The base grammar and
the session-14/17/18/19/20/25 additions above remain authoritative; the new
productions are:

```
LetBinding  := 'let' 'mut'? Ident (':' Type)? '=' Expr ';'
ConstBinding:= 'const' Ident (':' Type)? '=' Expr ';'
```

- **Let bindings** may now carry an optional type annotation:
  `let x: Int = 1;`. When present, the type checker enforces that the
  initializer expression's type matches the declared type (`E-T01` on
  mismatch). When absent (`None`), the type is inferred from the initializer
  (existing behavior).
- **Const bindings** may now carry an optional type annotation:
  `const X: Int = 42;`. The enforcement is identical to let bindings.
- **Mixed annotations** are supported: `let x: Int = 1; let y = 2;`
  has `x` declared as `Int` and `y` inferred.
- All type forms are accepted in annotations: `Int`, `Float`, `Bool`,
  `Char`, `Str`, `Null`, user-declared struct and enum names, `Ptr<T>`,
  `&T`, `&mut T`, and `[T; N]`.

The session-26 lexer needed no changes (`:` and type tokens already
existed). Exclusions from §2 and §9 that remain (`type`
aliases, generics) are unchanged.

## 20. Session-27 Additions: Or-Patterns, Range Patterns, and Guards

**Session:** 27 — Richer match patterns

This section extends the frozen grammar additively. The base grammar and
the session-14/17/18/19/20/25/26 additions above remain authoritative; the
new productions are:

```
MatchArm  := Pattern ('if' Expr)? '=>' Block
Pattern   := OrPattern
OrPattern := AltPattern ('|' AltPattern)*
AltPattern:= IntLit | '-' IntLit | IntLit '..' IntLit | IntLit '..=' IntLit
           | '-' IntLit '..' IntLit | '-' IntLit '..=' IntLit
           | BoolLit | Ident '::' Ident | Ident '::' Ident '(' Pattern ')'
           | Ident
```

- **Or-patterns** combine alternatives with `|`, matching if any
  alternative matches: `1 | 2 | 3`, `E::A(x) | E::B(x)`, `E::A | E::B`,
  and mixed forms. Every alternative must bind the same set of names with
  the same types (`E-T34` on mismatch); the binding is shared across the
  alternatives, so the alternatives' bodies see one binding per name.
- **Range patterns** match an integer within an inclusive (`1..=5`) or
  exclusive (`1..5`) range: `1..=5`, `-3..=2`, `1..10`, `-10..-1`.
  Endpoints must be integer literals (optionally negated); anything else
  is `E-P19` (expected an integer literal). A range is only well-typed on
  an `Int` scrutinee; an exclusive range whose endpoints are equal
  (`1..1`) or inverted (`5..1`) covers nothing and is reported as
  unreachable (`E-T25`) when it is the only pattern covering its space.
- **Guards** are an optional `if Expr` after the pattern, before `=>`:
  `x if x > 3 => { .. }`. The guard is evaluated after the pattern
  matches and before the arm body, with the pattern's bindings in scope.
  The guard must be a `Bool` expression (`E-T09` on mismatch); a guarded
  arm commits no coverage, so a guarded arm never contributes to
  exhaustiveness (`E-T25` reports the uncovered space).

The session-27 lexer needed no changes (`|`, `..`, `..=`, and `if`
already existed). Exclusions from §2 and §9 that remain (`type`
aliases, generics) are unchanged.

## 21. Session-28 Additions: Block Expressions and If-As-Expression

**Session:** 28 — Block expressions and if-as-expression

This section extends the frozen grammar additively. The base grammar and
the session-14/17/18/19/20/25/26/27 additions above remain authoritative;
the new productions are:

```
Block       := '{' Stmt* Expr? '}'
Primary     := ... | IfExpr | BlockExpr
IfExpr      := 'if' Expr BlockExpr 'else' (IfExpr | BlockExpr)
BlockExpr   := '{' Stmt* Expr? '}'
```

- **Block expressions** (`{ stmts; expr }`) evaluate their statements in
  order, then evaluate the trailing expression (the last expression not
  followed by `;`) as the block's value. An empty block or a block whose
  last item ends with `;` has no trailing expression and evaluates to
  `Unit`. Block expressions appear in expression position: `let x = { 1 +
  2 };`.
- **If-as-expression** (`if cond { a } else { b }`) evaluates `cond`,
  then evaluates and returns the value of the taken branch. The `else`
  branch is **required** (an if-expression without `else` is `E-P26`).
  Both branches must be expression blocks with trailing expressions of the
  same type (enforced by the type checker). If-expressions nest: `if a { x
  } else if b { y } else { z }`.
- **Distinguishing group vs tuple vs block**: `(expr)` remains a group
  expression (parenthesized); `{ expr }` is a block expression returning
  `expr`. `if expr { a } else { b }` in statement position is an
  if-statement; in expression position it is an if-expression.

The session-28 lexer needed no changes (`if`, `{`, `}`, `else` already
existed). Exclusions from §2 and §9 that remain (`type` aliases,
generics, lambdas/closures) are unchanged.

## 22. Session-29 Additions: Tuple Types and Expressions

**Session:** 29 — Tuple types, tuple expressions, and tuple field access

This section extends the frozen grammar additively. The base grammar and
the session-14/17/18/19/20/25/26/27/28 additions above remain
authoritative; the new productions are:

```
Type        := ... | TupleType
TupleType   := '(' Type (',' Type)* ','? ')' | '()'
Primary     := ... | TupleExpr | UnitExpr
TupleExpr   := '(' Expr (',' Expr)* ','? ')'   (two or more elements)
UnitExpr    := '()'
Postfix     := ... | TupleFieldAccess
TupleFieldAccess := '.' IntLit
Pattern     := ... | TuplePattern
TuplePattern:= '(' Pattern (',' Pattern)* ','? ')' | '()'
```

- **Tuple types** (`(Int, Bool)`, `(Int,)`, `()`) represent fixed-length
  heterogeneous sequences of types. An empty tuple `()` is the unit type.
  A single-element tuple `(Int,)` is distinct from `Int`. Tuple types
  appear in all type positions: struct fields, let/const bindings,
  function parameters, and return types.
- **Tuple expressions** (`(1, true)`, `(1,)`, `()`) construct tuple
  values. The element types determine the tuple type. `()` produces the
  unit value. A single-element tuple requires a trailing comma `(1,)`.
- **Tuple field access** (`x.0`, `x.1`) reads the element at the given
  non-negative integer index. The index is a literal, not an expression;
  it must be within the tuple's length (`E-T35` out of range).
  Chained access requires grouping: `(x.0).1` because `x.0.1` lexes
  `0.1` as a float literal.
- **Tuple patterns** (`(a, b)`, `()`) destructure tuple values in match
  arms. Each element pattern binds or tests the corresponding tuple
  element. An empty tuple pattern `()` matches only unit values.

The session-29 lexer needed no changes (all tokens already existed).
Exclusions from §2 and §9 that remain (`type` aliases, generics,
lambdas/closures) are unchanged.

## 23. Session-30 Additions: While/Loop Expressions and Break Values

**Session:** 30 — while/loop as expressions, break with values

This section extends the frozen grammar additively. The base grammar and
the session-14/17/18/19/20/25/26/27/28/29 additions above remain
authoritative; the new productions are:

```
Break       := 'break' Expr? ';'
Primary     := ... | WhileExpr | LoopExpr
WhileExpr   := 'while' Expr Block
LoopExpr    := 'loop' Block
```

- **Break with value** (`break expr;`) carries an optional value out of
  a loop. `break;` (no value) is unchanged and evaluates to `Unit`.
  `break expr;` is valid in any loop (statement or expression position).
  The value's type must be compatible with the loop's result type.
- **While-expression** (`while cond { body }`) in expression position
  evaluates the loop body repeatedly while `cond` is `true`. The
  expression's type is determined by `break` values inside the body. A
  `while` statement (followed by `;`) is unchanged and has type `Unit`.
- **Loop-expression** (`loop { body }`) in expression position evaluates
  the loop body repeatedly. The expression's type is determined by
  `break` values inside the body. A `loop` statement (followed by `;`)
  is unchanged and has type `Unit`.
- Loop expressions appear in binding position (`let x = loop { ... }`),
  return position (`return loop { ... }`), and expression statement
  position. They do not appear in arbitrary operand position (function
  arguments, binary operands); this mirrors the existing `if`-expression
  limitation.

The session-30 lexer needed no changes (all tokens already existed).
Exclusions from §2 and §9 that remain (`type` aliases, generics,
lambdas/closures) are unchanged.
