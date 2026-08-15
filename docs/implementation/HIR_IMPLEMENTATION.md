# MINK — HIR Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 08 — HIR / Compiler IR Foundation

## 1. Purpose

HIR (High-level Intermediate Representation) is the first real compiler IR
layer, sitting between the typed front end and the MIR:

    Source → Lexer → Parser → AST → Semantic Analysis → Type Analysis
        → HIR → MIR → Optimization → Backend → Runtime → executable

The AST answers *"what was written"*, semantic analysis answers *"which
declaration does each name refer to"*, type analysis answers *"what type
does every expression have"*, and HIR answers *"here is the program as a
typed, resolved, owned tree that later compiler stages can consume without
re-reading the source or re-running any front-end analysis"*.

HIR is a **durable output**, not a transient view: it owns all of its data
(names are copied strings, spans and ids are copied values) and carries its
own cloned type table, so it remains valid after the AST and the analysis
results are dropped. Tooling (MIR lowering, diagnostics, a future
formatter/LSP) can rely on it without borrowing the front end.

## 2. AST-to-HIR Boundary

Lowering consumes three inputs and produces one output:

| Input                              | Role                                          |
| ---------------------------------- | --------------------------------------------- |
| `&Ast`                             | the tree being lowered                        |
| `&SemanticResult` (session 05)     | symbols (`SymbolId`), scope forest, resolutions |
| `&TypeResult` (sessions 06–07)     | per-symbol and per-expression types (`TypeId`) |

`hir::lower(&ast, &semantic, &types) -> Result<HirProgram, Vec<HirError>>`

The boundary is strict and one-directional:

- **no re-analysis** — lowering never re-runs name resolution or type
  checking; it only *looks up* the answers the front end already computed;
- **no AST mutation** — the AST is read-only input;
- **no duplicated systems** — HIR references [`SymbolId`] and [`TypeId`]
  rather than redefining symbols or types; the only data it copies is the
  interned [`TypeTable`] (so the program is self-contained) and identifier
  name text (so the tree owns its strings).

### 2.1 Exact expression-type lookup

The type checker records one `(expression span, type)` entry per expression
it visits. Because one expression can be a prefix of another (`1` and
`1 + 2` share a start offset), matching by span start alone is ambiguous.
`TypeResult::expr_type_exact(span)` (added in this session) requires the
full span to match, so every lowered expression node receives the precise
type the checker gave it.

## 3. Node Design

HIR mirrors the AST shape so lowering is direct and control flow is
explicit. Every expression carries `span` **and** `ty`:

| HIR node                  | Notes                                             |
| ------------------------- | ------------------------------------------------- |
| `HirProgram`              | `items: Vec<HirItem>` + cloned `types: TypeTable` |
| `HirItem` / `HirItemKind` | `Fn`, `Let`, `Const` (module scope)               |
| `HirFn`                   | name, params, body, whole-item span, `Fn` type    |
| `HirParam`                | name (symbol), type                               |
| `HirLet` / `HirConst`     | name (symbol), init, binding type, mutability     |
| `HirBlock` / `HirStmt`    | statements in order; `HirStmtKind`                |
| `HirIf` / `HirElseBranch` | cond, then block, else-if chain / else block      |
| `HirExpr` / `HirExprKind` | literals, `Var`, unary, binary, assign, range, call, member, index |

`HirExprKind` has **no `Group`** node: syntax-only parentheses are
eliminated (see §4). It re-exports the operator enums (`UnaryOp`,
`BinaryOp`, `AssignOp`) from the AST so the HIR API is self-sufficient.

## 4. Lowering Invariants

For a program that passed semantic and type analysis, lowering always
succeeds and preserves:

1. **Source order** — items, statements, and expressions lower in source
   order; the program is deterministic for identical input.
2. **Exact spans** — every node carries the span of the source text it was
   parsed from. A parenthesized expression keeps the parentheses' span, so
   a lowered node covers the text *as written* (see Group elimination).
3. **Symbol resolution** — `HirIdent` (declaration names, references,
   parameters, loop variables) carries the exact `SymbolId` the semantic
   analyzer resolved, looked up through `SemanticResult::resolve` for
   references and a span→symbol index built from the symbol table for
   declaration names. `HirName` (member names) carries no symbol: members
   are not declarations until user-defined types exist.
4. **Canonical types** — every expression and binding carries the
   canonical `TypeId` the checker recorded: inference variables are
   resolved to the type they denote (`Int`, `Range<Int>`,
   `fn(Int) -> Int`), so the HIR is immediately readable without
   canonicalization.
5. **Group elimination** — `(expr)` lowers to the inner node with the
   parentheses' span. The inner expression's type is the group's type, so
   no information is lost.
6. **No hidden work** — no name resolution, no scope construction, no type
   unification is performed during lowering.

## 5. Control-Flow Representation

Control flow is represented structurally, exactly as written, in the shape
MIR lowering consumes (session 09):

- nested **blocks** (`HirBlock`) with statements in order;
- **branches** — `HirIf` with an optional `HirElseBranch` (`If` for
  `else if` chains, `Block` for `else { ... }`);
- **loops** — `HirStmtKind::While { cond, body }`,
  `HirStmtKind::For { var, iterable, body }` (the loop variable resolved to
  its `ForVar` symbol), and `HirStmtKind::Loop(body)`;
- **jumps** — `Break`, `Continue`, and `Return(Option<HirExpr>)` (the
  value is `None` for a bare `return;`).

MIR lowering (session 09) consumes this structure to linearize it into
basic blocks: `if`/`else` becomes a conditional branch with two branch
blocks and a shared continuation, `while`/`for`/`loop` become block
machines with explicit break/continue targets, and `break`/`continue`/
`return` become jumps and returns. HIR preserves enough structure (nested
blocks, explicit branches, loops, jumps) that the HIR → MIR boundary needs
no re-analysis.

## 6. Error Handling

Lowering failures are **structured, not panics**. For a clean front end
they cannot occur; they defend against malformed or tooling-produced ASTs
that reach the `lower` API directly. All errors found are collected (the
traversal continues with fallback nodes) and returned in source order:

| Code | Kind                  | Meaning                                       |
| ---- | --------------------- | --------------------------------------------- |
| E-H01| `UnresolvedSymbol`    | an identifier (reference or declaration name) has no resolved symbol |
| E-H02| `MissingType`         | a symbol or expression has no recorded type   |
| E-H03| `InvalidFunctionType` | a function symbol's type is not a `Fn` type   |

`HirError` carries the kind, the exact span, and (for `E-H03`) the
offending type. `CheckError::Hir` renders them uniformly with the other
stages' diagnostics.

## 7. Driver and CLI Integration

`driver::check` now runs HIR lowering when semantic and type analysis
reported no errors:

- a clean front end is lowered; on success `CheckReport.hir` carries the
  [`HirProgram`];
- a lowering failure on a clean front end is an internal compiler error
  and is reported as `E-H01`…`E-H03` (exit 1);
- a front end with semantic or type errors is **not** lowered — lowering an
  inconsistent front end would only add misleading diagnostics;
- `mink check` therefore validates through HIR (and, since session 09,
  through MIR): valid programs report `passed parsing, semantic analysis,
  type checking, HIR lowering, and MIR lowering (N tokens)` and exit 0;
  every error class exits 1;
- `mink build` compiles the optimized MIR through the native backend
  (session 11) and links the generated image against the embedded runtime
  (session 12) — see `NATIVE_BACKEND_IMPLEMENTATION.md` and
  `RUNTIME_IMPLEMENTATION.md`.

## 8. MIR Boundary

HIR is the *last high-level* IR: it still matches the source's structural
shape (one node per source construct, no flattening, no temporaries, no
basic blocks). MIR (session 09, `src/mir/`, `docs/implementation/MIR_IMPLEMENTATION.md`)
consumes this tree and produces a linear, control-flow-graph
representation: basic blocks, statements, and terminators, with
`if`/`else`, `while`, `for`, `loop`, `break`, `continue`, and `return`
lowered into explicit jumps, branches, and returns. The explicit
control-flow nodes and canonical types in HIR are the seam MIR lowering
consumes; nothing in HIR pre-commits to a particular MIR design beyond it.

## 9. Known Limitations

- Literal values are not decoded into the IR (matching the AST): the raw
  text is recovered from the node's span via the source map. Decoding
  belongs to a later milestone.
- Member and index expressions are represented but their types are the
  deferred inference variables the type checker recorded (user-defined
  types do not exist yet).
- `HirIdent.symbol` is fabricated as `SymbolId(0)` on the error path only;
  valid programs always carry real symbol ids.
- No desugaring is performed (no lowering of `for` into `while`, no
  `else if` flattening): the HIR mirrors the source so diagnostics and
  spans stay exact.
- HIR clones the interned type table for self-containment; the clone is
  small (one slot per distinct type) and the cost is bounded by type
  count, not source size.

## 10. Tests

Coverage lives in `tests/hir.rs` (25 tests) plus the internal-failure unit
tests in `src/hir/lower.rs` (2) and the CLI tests in `tests/cli.rs` (58
total):

- **Literals**: every literal kind lowers with its type.
- **Identifiers/declarations**: references preserve `SymbolId` and type;
  `let`/`let mut`/`const` mutability; module items in source order.
- **Operators**: unary/binary operators and assignment operators lower
  with their operands and targets.
- **Calls**: callee symbol, argument list, result type propagation.
- **Functions**: name/parameter symbols, parameter types, `Fn` type.
- **Control flow**: returns (with/without value), if/else and else-if
  chains, while, for (loop-variable symbol + `ForVar` kind), loop,
  break/continue, nested control flow.
- **Ranges**: inclusive flag and element type.
- **Spans**: exact expression/statement/block/function spans; group
  elimination keeps the parentheses' span and removes the group node.
- **Types**: HIR types match the type checker's recorded types; the
  cloned table is usable standalone.
- **Member/index**: nodes lower with names and base/index expressions.
- **Robustness**: empty programs, 200-function scale.
- **Malformed input**: a hand-built AST with an unresolved identifier
  produces exactly one `E-H01` at the identifier span; fabricated
  inconsistent type results produce `E-H02`/`E-H03` — never a panic.

## 11. Quality Gates

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo build
    git diff --check

Full suite after session 09: **537 tests** (58 CLI + 50 lexer + 88 parser +
62 parser hardening + 72 semantics + 12 source + 122 typecheck + 25 HIR +
34 MIR + 14 lib unit tests), all passing. After session 10 (optimization)
the suite is **585 tests**, after session 11 (native backend) it is
**622 tests**, after session 12 (native runtime foundation) it is
**654 tests**, after session 13 (string + memory types) it is
**700 tests**, and after session 14 (structs + arrays, with `HirStruct`
items and struct/array literal expressions) it is **762 tests**, and
after session 15 (ownership analysis, a compile-time gate before HIR
lowering) it is **803 tests** (see
`OPTIMIZATION_IMPLEMENTATION.md` §7).
