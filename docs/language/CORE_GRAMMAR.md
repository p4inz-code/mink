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

- Type declarations (`type`, `struct`, `enum`, `trait`, `impl`) — type-system
  milestone
- Module system (`mod`, `use`, `pub`) — module-system milestone
- Pattern matching (`match`) — pattern-matching milestone
- Async/await and unsafe — concurrency/unsafe milestones
- Type annotations on parameters, bindings, and return types — type-system
  milestone (the `:` and `->` tokens exist lexically but are not yet part of
  any production)
- Blocks as expressions, `if` as an expression, lambdas/closures
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
- Whether `break`/`continue` appear inside a loop is a semantic (later)
  question; the syntax accepts them anywhere.

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
errors if used, until their milestones land: type annotations and return
types, `struct`/`enum`/`type`/`trait`/`impl` declarations, `mod`/`use`/`pub`,
`match` arms, `async fn`/`await`, `unsafe` blocks, closures, block
expressions, `if` expressions, and the `?` operator.

## 10. Grammar-Freeze Changes to the Lexer

Freezing the grammar required one minimal lexer change:

- **`mut` became a keyword.** `let mut x = 1;` is the frozen mutability
  syntax. `mut` previously lexed as an identifier; the session 02 keyword set
  was explicitly provisional pending the grammar design, so this is the
  intended freeze point. The change is a one-entry addition to the keyword
  table plus a `TokenKind::Mut` variant; identifiers like `mutt` and `_mut`
  are unaffected.

No other lexical forms changed. `:` (type annotation), `->` (return type),
`?` (optional handling), `::` (path separator), and `=>` (match arm) remain
lexed but unused by this grammar; the parser rejects them with diagnostics
(verified by regression tests in `tests/parser_hardening.rs`), as documented
in `docs/implementation/LEXER_IMPLEMENTATION.md`.

## 11. Status

This grammar is frozen for the constructs it covers. Statements and
declarations outside it are rejected by the parser with stable diagnostics
(see `docs/implementation/PARSER_IMPLEMENTATION.md` for the error codes).
Future sessions extend this document additively.
