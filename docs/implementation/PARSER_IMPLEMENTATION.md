# MINK — Parser and AST Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 03 — Parser + AST

## 1. Objective

The parser converts a MINK token stream into an abstract syntax tree with
accurate source spans, reporting syntax problems as structured diagnostics
and recovering so that several independent errors in one file are reported in
a single run. The AST is the durable syntax representation for later
semantic analysis, type checking, HIR lowering, and tooling.

At the end of this session the following works end to end:

    MINK source → SourceFile → Lexer → Parser → AST → diagnostics

`mink check <path>` now performs lexical **and** syntax analysis. Semantic
analysis, type checking, and code generation remain future milestones.

## 2. Grammar

The grammar implemented by this parser is frozen in
`docs/language/CORE_GRAMMAR.md`, which is authoritative. Highlights:

- Top-level items: `fn`, `let`, `let mut`, `const`.
- Statements: bindings, `return`, `break`, `continue`, `if`/`else`,
  `while`, `for`/`in`, `loop`, expression statements.
- Expressions: literals (int, float, string, char, bool, null),
  identifiers, unary (`- ! ~`), binary operators with C-family precedence,
  right-associative assignment (`= += -= *= /= %=`), right-associative
  ranges (`..`, `..=`), calls, member access, indexing, and parenthesized
  grouping.
- One lexer change was required by the freeze: `mut` joined the keyword set
  (see `docs/language/CORE_GRAMMAR.md` §10).

## 3. Parser Architecture

The parser lives in `src/parser/` and is a hand-written recursive-descent
parser over the pull-based lexer API (`Lexer::new` + `Lexer::next_token`);
tokens are `Copy` and are consumed exactly once through a single-token
lookahead window. Nothing is re-lexed or reparsed.

Entry point:

```rust
mink::parser::parse(&SourceFile) -> ParseOutput
```

`ParseOutput` carries the `Ast`, the number of tokens consumed (excluding
`Eof`), the lexer's lexical errors, and the parser's syntax errors, so the
driver can report all problems in one run without lexing twice.

Parser structure mirrors the grammar:

- `parse_program` → `parse_item` (`fn` / `let` / `const`)
- `parse_statement` → bindings, control flow, expression statements
- `parse_expression` → a precedence-climbing chain
- `parse_primary` / `parse_postfix` → atoms, calls, members, indexes

### 3.1 Expression parsing

Each binary precedence level is one small method built on a shared helper
(`parse_binary_level`) that takes the next level plus the level's operator
table. Precedence and associativity therefore live in one auditable place
per level; the full table is in `docs/language/CORE_GRAMMAR.md` §8.1.
Assignment and ranges are right-associative; all binary levels are
left-associative. Grouping keeps an explicit `Group` node so tooling can
distinguish written parentheses from parser-imposed association.

### 3.2 Spans

Every AST node carries the exact `Span` (half-open byte range) it was parsed
from, using the shared source span type — no second span system. Literal
nodes store no decoded value or copied text: the raw source spelling is
recovered through `SourceFile::span_text`. Identifiers store their name text
(small, and the currency of name resolution) plus their span. This avoids
duplicating potentially large source strings in the tree.

## 4. AST

The AST lives in `src/ast/` and mirrors the frozen grammar:

- `Ast { items: Vec<Item> }`
- `Item` / `ItemKind::Fn(FnItem) | Let(LetItem) | Const(ConstItem)`
- `FnItem { name, params: Vec<Param>, body: Block }`
- `Block { stmts: Vec<Stmt> }`
- `Stmt` / `StmtKind` — bindings, `Return(Option<Expr>)`, `Break`,
  `Continue`, `If(IfStmt)`, `While`, `For`, `Loop(Block)`, `Expr(Expr)`
- `IfStmt { cond, then_block, else_branch: Option<ElseBranch> }` with
  `ElseBranch::If(Box<IfStmt>) | Block(Block)` for else-if chains
- `Expr` / `ExprKind` — literals (unit variants), `Ident(Ident)`, `Unary`,
  `Binary`, `Assign`, `Range { inclusive }`, `Call`, `Member`, `Index`,
  `Group`
- Operator enums `UnaryOp`, `BinaryOp`, `AssignOp` with `symbol()` for
  tooling

Nodes are plain data structures with public fields (exhaustive
pattern-matching is the expected consumption style), derive `Debug`, `Clone`,
`PartialEq`, `Eq`, and the whole tree is arena-free (owned `Box`/`Vec`).

## 5. Error Handling and Recovery

Syntax errors live in `src/parser/error.rs`, mirroring the lexer's model:

- `ParseErrorKind` — 16 stable categories with codes `E-P01` … `E-P16`
  (e.g. `E-P03` expected an expression, `E-P06` expected `;`,
  `E-P14` unclosed `{`), each with a human-readable message.
- `ParseError { kind, span }` — `Copy`, with `kind()`, `span()`, and
  `Display`.

The parser never panics on malformed input. Recovery is panic-mode:

- Statement-level errors skip tokens to the nearest `;`, `}`, or `Eof`
  (consuming a `;` if one is found), so the next statement still parses.
- Item-level errors skip to the next `fn`/`let`/`const` or `Eof`, so later
  declarations survive a broken one.
- `{ ... }` groups encountered during a skip are consumed as units so a
  malformed statement cannot swallow a following block.
- A stack of open delimiters reports the innermost unclosed `(`/`{`/`[` at
  end of input (one report per root cause; outer open delimiters are not
  cascaded onto the same failure).
- Recovered constructs keep the surrounding parse consistent (for example, a
  missing function body yields an empty block plus one `E-P11` error rather
  than a poisoned parser state).

Assignment-target validation (`E-P04`) is done syntactically: the left side
must be an identifier, member, or index expression.

## 6. Driver and CLI Integration

`driver::check` now loads the file, runs `parser::parse`, and merges lexical
and syntax errors into a single source-ordered `Vec<CheckError>` report
(`CheckError::Lex(LexError) | Parse(ParseError)`, with `code()` and `span()`).

`mink check <path>`:

- valid input → `passed parsing (N tokens)` on stdout, exit 0;
- invalid input → every diagnostic (`mink: error[E-XX]: message` plus a
  `--> path:line:col` location) on stderr, exit 1;
- unreadable file → I/O error, exit 1;
- never panics.

`mink build` remains `NotImplemented` (exit 2): compilation, type checking,
and code generation are explicitly out of scope. The rendering in `cli.rs`
is still the minimal ad-hoc formatter; the structured diagnostic engine
remains a later milestone (`docs/language/ERROR_SYSTEM.md`), and the parser's
error kinds are designed to feed it rather than be replaced.

## 7. Design Decisions

- **Grammar freeze scope.** Only constructs the session's milestone requires
  are implemented; everything else (types, modules, match, async, unsafe,
  closures, block expressions) is rejected with diagnostics and documented
  as a later milestone in `docs/language/CORE_GRAMMAR.md`. See §8 for the
  full exclusion list.
- **Recursive descent over a table-driven binary chain.** Simple, auditable,
  and matches the grammar's fixed operator set; no parser-generator
  dependency (the project remains dependency-free).
- **Explicit `Group` node.** Tooling (formatter, LSP, diagnostics) can tell
  written parentheses from precedence association.
- **Semicolons are required.** Explicit terminators beat newline-sensitive
  parsing for determinism, tooling, and AI legibility (design rules:
  durability, fewer errors).
- **Module scope holds declarations only.** Matches `CORE_LANGUAGE.md` §2;
  executable statements live in function bodies.
- **Empty statements are accepted silently** (`;` alone), matching the
  forgiving C/Rust family behavior.
- **Only the innermost unclosed delimiter is reported at EOF**, to minimize
  cascades from one root cause.
- **Literals keep spans, not values.** Value decoding belongs to the
  semantics/literal-interpretation milestone.

## 8. Known Intentional Limitations

- **Recursion depth** is bounded by the call stack (a nested-expression
  chain costs roughly 30 frames per level). Arbitrarily deep nesting is not
  a goal of this milestone; pathological depth is a future robustness item.
- **No semantic validation** — for example `break` outside a loop, duplicate
  names, or invalid assignment targets that are not obviously non-places are
  not diagnosed (semantics milestone).
- **Type annotations, `->` return types, and generics syntax are rejected**
  as syntax errors until the type-system milestone defines them.
- **Unicode identifiers** remain a lexer limitation (ASCII only).
- **Error spans** point at the offending token or the unclosed delimiter's
  opener; explanations and machine-readable output await the diagnostic
  engine.

## 9. Future Extension Points

- Adding `name: Type` parameters and `-> Type` return types extends
  `parse_params`/`parse_fn` without restructuring.
- Adding match, struct/enum/type/trait/impl, mod/use/pub, async/await, and
  unsafe extends `parse_item`/`parse_statement` dispatch — the parser was
  shaped so each construct is one method plus a dispatch arm.
- Block/`if` expressions extend `parse_primary`.
- The delimiter stack and error kinds are ready to feed the structured
  diagnostic engine (severity, related spans, machine-readable output).

## 10. Performance Notes

- Single forward pass; every token is pulled exactly once through a
  one-token lookahead window (no token buffer, no re-lexing).
- No allocations per token; AST nodes allocate exactly once each.
- No string duplication except identifier names.
- Constant-time dispatch per level; total work is linear in token count.

## 11. Tests

Parser and AST coverage lives in `tests/parser.rs` (88 tests) and covers:

- program shape, declarations, bindings, parameters
- every literal kind, identifiers, unary and binary operators
- precedence and associativity for every level, grouping, ranges
- calls, member access, indexing, mixed postfix chains
- assignment (including every compound operator) and assignment-target
  validation
- all control-flow statements and else-if chains
- comments around and between syntax
- invalid input for every error category, EOF during constructs, multiple
  independent errors, and recovery behavior (later items/statements survive)
- exact span assertions for items, statements, expressions, and groups
- a deterministic pseudo-random malformed-input corpus (2,000 inputs) plus a
  targeted malformed corpus asserting the invariants: never panics, error
  spans in bounds, AST node spans in bounds and non-inverted
- depth and scale (deep grouping, a 500-function program with token-count
  checks)

CLI behavior for parser diagnostics is covered in `tests/cli.rs`
(syntax-error exit codes, multiple errors, combined lexical+syntax
reporting, representative programs). The lexer keyword-freeze change is
covered in `tests/lexer.rs`.

## 12. Quality Gates

As before:

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo build
    git diff --check
