# MINK — Semantic Analysis Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 05 — Semantic Analysis Foundation

## 1. Objective

The parser establishes *"this source is syntactically valid"*. Semantic
analysis establishes *"this syntactically valid program is semantically
coherent according to the semantic rules currently supported by MINK"*.

At the end of this session the following works end to end:

    MINK source → SourceFile → Lexer → Parser → AST → Semantic Analysis → diagnostics

`mink check <path>` now performs lexical, syntactic, **and** semantic
analysis. Name resolution and the semantic result are durable structures
consumed by the type checker (session 06) and HIR lowering (session 08);
sessions 09–12 added MIR, optimization, the native backend, and the runtime
foundation on top.

## 2. Architecture

The analyzer lives in `src/semantics/`:

| File        | Role                                                        |
| ----------- | ----------------------------------------------------------- |
| `mod.rs`    | `analyze(&Ast) -> SemanticResult`; public result API        |
| `error.rs`  | `SemanticErrorKind` (`E-S01`…`E-S07`, `E-S08`, `E-S10`, `E-S11`, `E-S15`, `E-S16`) + `SemanticError` |
| `symbol.rs` | `Symbol`, `SymbolKind`, `SymbolTable`, `Scope`, `ScopeTable`|
| `analyzer.rs` | One-pass traversal: scopes, symbols, resolution, validation |

The pipeline:

    AST
      |
      v
    Semantic Analyzer
      |  +-- Scope construction
      |  +-- Symbol collection
      |  +-- Name resolution
      |  +-- Duplicate-definition detection
      |  +-- Mutability validation
      |  +-- Control-flow validation
      |  +-- Semantic diagnostics
      v
    Semantic Result
      |
      v
    Future Type Analysis

The analyzer performs a single forward pass over the AST (module declarations
are collected in one pre-pass so module scope is order-independent). It never
mutates the parser AST: all output lives in the [`SemanticResult`], keeping
the AST/semantic boundary clean.

## 3. Symbol Model

Every declaration becomes one [`Symbol`]:

- **`SymbolId`** — stable numeric identity, assigned in deterministic source
  order; valid for the lifetime of the result.
- **`name`** — the declared identifier spelling.
- **`kind`** — [`SymbolKind`]: `Fn`, `Param`, `Let { mutable }` (`let` vs
  `let mut`), `Const`, `ForVar`.
- **`span`** — exact declaration span (the identifier token).
- **`scope`** — the [`ScopeId`] the symbol is declared in.

Symbols store names and spans only — never a clone of an AST node — so the
table stays lightweight. The `SymbolTable` owns all symbols; consumers use
`get(id)`, `iter()`, and `len()`. `SymbolId`s are the currency for the
resolved-reference map (§8) and for future type checking.

Mutability per kind: only `Let { mutable: true }` is writable. `let`, `const`,
parameters, `for` variables, and function names are immutable (see §6).

## 4. Scope Model

Scopes are created from the lexical structure of the program and form a
forest linked by parent pointers:

| Scope kind   | Created for                                                       |
| ------------ | ----------------------------------------------------------------- |
| `Module`     | The whole file; holds top-level declarations. No parent.          |
| `Function`   | Each `fn`; **the function body block is the function's declaration scope** — parameters and the body's own `let`/`const` declarations share it. |
| `Block`      | Every other block: `if`/`else` bodies, `while`, `for`, `loop` bodies, and any future nested `{ ... }`. The `for` loop variable is declared in the loop body's scope. |

Each [`Scope`] records its kind, parent, the symbols declared directly inside
it (declaration order), and an internal name index. The public surface is
read-only accessors (`symbols()`, `lookup(name)`, `parent`, `kind`) so the
internal index can evolve without breaking consumers.

### 4.1 Declaration order

- **Module scope is order-independent**: all top-level names are collected
  before any item body is analyzed, so a declaration is visible throughout
  its module — functions may call each other in any order, and a `let`/`const`
  initializer may reference a binding declared later.
- **All other scopes require declaration-before-use**: a name is visible from
  its declaration point to the end of its scope. A binding is not visible in
  its own initializer (`let x = x;` refers to an outer `x` or is unresolved),
  and a `for` loop variable is not visible in its own iterable expression.

### 4.2 Duplicates and shadowing

- A scope may not declare the same name twice; the second declaration is a
  **duplicate-definition error** (`E-S02`) pointing at the duplicate and
  recording the original declaration's span. The first declaration wins for
  resolution, which prevents cascading unresolved-name errors.
- Because parameters share the function scope with body declarations, a
  parameter/local name collision is a duplicate-definition error.
- A **nested scope may shadow** an enclosing name; references resolve to the
  innermost declaration. Same-scope shadowing (redeclaration) is not allowed.
- Functions, bindings, constants, parameters, and loop variables share one
  namespace per scope at this stage (there are no type/value namespaces yet).

## 5. Name Resolution

The resolver walks the scope chain outward from the current scope and returns
the innermost declaration. Lookup is O(1) per scope via the name index, so
total analysis work is linear in source size.

- **Distinguishes declarations from references**: declarations bind symbols;
  references (identifier expressions, assignment targets, call callees) are
  resolved.
- **Reports unresolved identifiers** (`E-S01`) with the exact reference span.
  Every unresolved use is an independent diagnostic (not a cascade).
- **Retains spans and symbol identities** in the resolved-reference map (§8).
- **Call targets**: a call whose callee is a plain identifier resolves that
  name. Member names are field selectors and are **not** resolved as scope
  names (they belong to the type system). Whether a resolved symbol is
  callable, and whether argument counts/types match, is deliberately deferred
  to the type-system milestone.
- No type inference is performed to resolve names.

## 6. Mutability and Assignment

`let` bindings are immutable by default (`CORE_LANGUAGE.md` §7). The analyzer
validates assignment targets semantically (the parser already validates that
targets are syntactically places, `E-P04`):

- **Identifier targets** are resolved and checked for writability:
  - assignment to an immutable `let`, parameter, `for` variable, or function
    name → `E-S03` (assignment to immutable);
  - assignment to a `const` → `E-S04` (assignment to constant);
  - assignment to a `let mut` binding → accepted.
  - Compound assignment operators (`+=` `-=` `*=` `/=` `%=`) follow the same
    writability rule.
- **Member/index targets**: the base expression is resolved (an unresolved
  base is `E-S01`), but whether the target itself is writable depends on the
  base's type and is **deferred to the type-system milestone** — no type
  semantics are invented here.
- An unresolved target reports only `E-S01` (never a second mutability
  error).

## 7. Control-Flow Context

The analyzer carries a context of `{ scope, in_loop, in_function }` through
the traversal:

- `break;` outside any `while`/`for`/`loop` body → `E-S05`.
- `continue;` outside any loop body → `E-S06`.
- `return;` outside a function body → `E-S07`. This check is currently
  defensive: the frozen grammar only allows statements inside function bodies
  and functions cannot nest, so `E-S07` is unreachable from parser-produced
  ASTs. A module-level `return;` is rejected by the parser (`E-P01`).
- Nested loops and `if` bodies inside loops keep the correct context: a
  `break` inside an `if` inside a `loop` is valid.

No control-flow-graph construction, definite-assignment analysis, or
unreachable-code analysis is performed (later milestones).

## 8. Semantic Result

[`SemanticResult`] is the durable output later compiler stages consume:

- `errors()` — semantic diagnostics in source order; `has_errors()`.
- `symbols()` — the [`SymbolTable`].
- `scopes()` — the [`ScopeTable`].
- `resolutions()` — every resolved reference as `(identifier span, SymbolId)`,
  sorted by span start.
- `resolve(span) -> Option<SymbolId>` — answers *"which symbol does this
  identifier refer to?"* via binary search, without re-running name
  resolution.

References are keyed by the identifier token's exact span (each identifier
token has a unique span within a file). Declaration names are not references
and do not appear in the resolution list. The result is `Clone` + `PartialEq`
+ `Eq` so tests and tooling can assert exact outcomes.

## 9. Diagnostics

Semantic diagnostics follow the established lexer/parser model (stable code,
human-readable message, exact span) and reserve the `E-S*` range:

| Code | Kind                    | Message (example)                                   |
| ---- | ----------------------- | --------------------------------------------------- |
| E-S01| `UnresolvedName`        | `cannot find name \`missing\` in this scope`        |
| E-S02| `DuplicateDefinition`   | `duplicate definition of \`x\`` (+ original span)   |
| E-S03| `AssignmentToImmutable` | `cannot assign to \`x\`: it is not mutable`         |
| E-S04| `AssignmentToConstant`  | `cannot assign to \`x\`: it is a constant`          |
| E-S05| `BreakOutsideLoop`      | `` `break` outside of a loop ``                     |
| E-S06| `ContinueOutsideLoop`   | `` `continue` outside of a loop ``                  |
| E-S07| `ReturnOutsideFunction` | `` `return` outside of a function ``                |
| E-S10| `UseOfMovedValue`        | `cannot use \`s\`: value was moved`                  |
| E-S11| `MutatingImmutableString`| `cannot mutate \`s\`: it is immutable`               |
| E-S15| `DuplicateEnum`          | `duplicate definition of enum \`E\`` (+ original)    |
| E-S16| `DuplicateVariant`       | `duplicate variant \`A\` in enum declaration` (+ original) |

`SemanticError` carries the offending name (for name-related kinds) and, for
duplicates, the original declaration span, which the CLI renders as a note
(`note: previous declaration is here`). Existing codes `E-L01`…`E-L08` and
`E-P01`…`E-P24` are untouched (sessions 17–18 added `E-P22`…`E-P24` in the
parser).

Ownership diagnostics (`E-S10`/`E-S11`) come from the dedicated ownership
stage (`src/ownership/`, session 15 — see
`docs/implementation/OWNERSHIP_IMPLEMENTATION.md`), which runs between type
analysis and HIR lowering and gates code generation on a clean result.

### 9.1 Recovery

Analysis continues after independent errors:

- every unresolved use is reported (two unresolved names → two `E-S01`s);
- after a duplicate, the first declaration wins, so later references resolve
  without cascading;
- an unresolved assignment target reports only unresolved;
- bindings are still entered after an error, so later declarations and
  references are still analyzed;
- lexical or syntax errors suppress semantic analysis entirely (the driver
  runs it only on a valid token stream and tree), so no cross-stage cascades.

The analyzer never panics on structurally valid ASTs (all lookups are
guarded), including unusual shapes the parser would reject (e.g. a literal
assignment target).

## 10. Driver / CLI Integration

`driver::check` now runs the pipeline:

    source → lexer → parser → AST → semantic analysis → diagnostics

- Semantic analysis runs **only** when the source is lexically and
  syntactically valid; otherwise the previous error behavior is preserved.
- `CheckError` gained a `Semantic(SemanticError)` variant; `code()`, `span()`,
  `Display`, and a new `related_span()` (the original declaration span for
  duplicates) render uniformly in the CLI.
- `CheckReport` now carries `semantic: Option<SemanticResult>`, present when
  analysis ran — consumable by tooling and tests without re-parsing.
- Exit codes: valid → `0`; lexical, syntax, or semantic error → `1`;
  unreadable file → `1`. `mink build` compiles the optimized MIR through
  the native backend (since session 11). The CLI success message is now
  `passed parsing and semantic analysis (N tokens)`.

## 11. Performance

- One forward pass; every node visited exactly once.
- Per-scope `HashMap` name index: lookup walks only the scope chain
  (O(depth) per reference, constant per scope), never rescanning
  declarations — linear in ordinary source size.
- No AST cloning; symbols store names and spans only; identifier names are
  cloned once into the symbol table and once into name-related errors (small,
  bounded by error count).
- Resolved references are stored in a sorted vector with binary-search lookup
  (deterministic iteration order, O(log n) queries).

## 12. Future Type-System Boundary

The analyzer deliberately stops before type checking. Cleanly deferred:

- type checking, inference, generics, traits, overload/arity checking;
- callable-ness of resolved call targets;
- member/index target writability;
- ownership, borrowing, lifetimes, move analysis;
- HIR/MIR, optimization, code generation, runtime.

The boundary is explicit: the semantic result already exposes everything the
type system needs (symbol kinds, spans, scopes, resolved references) so
session 06 can consume it without re-running name resolution.

## 13. Known Intentional Limitations

- Module-scope bindings are visible in their own initializer (`let x = x;`
  at module scope resolves the initializer to the binding itself) as a
  consequence of order-independent module scope; block-scope bindings are
  not (the initializer is analyzed before binding). This asymmetry is
  deliberate and regression-tested.
- `E-S07` (`return` outside a function) is implemented defensively but is
  unreachable through the current grammar, which forbids statements at module
  scope and nested functions.
- Calling a resolved non-function name is not diagnosed (deferred to type
  checking).
- Member/index assignment writability is enforced: assigning through a
  member/index target whose base binding is immutable is
  `AssignmentToImmutable`, exactly like a plain assignment (session 14).
- Struct and enum names live in the type namespace (sessions 14/17):
  duplicate struct names are `E-S08`, duplicate fields `E-S09`, duplicate
  enum names are `E-S15`, duplicate variants `E-S16`, and a type name
  never resolves as a value. Other advanced types (tuples, data-carrying
  enums, generics) still do not exist.
- Duplicate triplets (`let x; let x; let x;`) report each redeclaration after
  the first as a duplicate of the original — deliberate, not a cascade of the
  same root cause.

## 14. Tests

Coverage lives in `tests/semantics.rs` (71 tests) and the semantic CLI smoke
tests in `tests/cli.rs` (11 new). Categories:

- **Valid**: declarations, resolution, nested/outer lookup, function calls,
  parameters, for-loop variables, mutable assignment, control-flow context,
  nested shadowing, order-independent module scope, member/index assignment
  through mutable and immutable bases, duplicate struct names (`E-S08`),
  duplicate fields (`E-S09`), and struct literals never resolving their
  name as a value.
- **Invalid**: unresolved names, duplicate definitions (module, function,
  block, parameters, parameter/local collision), immutable/const/param/loop-
  variable/function-name assignment, `break`/`continue` outside loops,
  use-before-declaration, block-scope escapes.
- **Recovery**: multiple unresolved names, duplicate + unresolved,
  mutability + unresolved, control-flow + resolution, errors in nested
  scopes, analysis continuing after errors.
- **Resolution identity**: stable/unique symbol ids, exact declaration spans,
  scope nesting and kinds, per-scope declarations, innermost-shadow
  resolution, deterministic resolution ordering.
- **Driver**: semantic result exposure, semantic errors through `check`,
  semantics skipped on parse/lex errors.
- **Robustness**: deep nesting, 200-function programs, long expression
  chains, and manually constructed unusual AST shapes (literal/group
  assignment targets) must not panic.

## 15. Quality Gates

As before:

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo build
    git diff --check

Total suite after session 05: **319 tests** (36 CLI + 50 lexer + 88 parser
+ 62 parser hardening + 71 semantics + 12 source), all passing.
