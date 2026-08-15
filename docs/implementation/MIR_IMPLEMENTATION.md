# MINK — MIR Implementation

**Status:** Implementation
**Version:** 0.1.0
**Session:** 09 — MIR / Control-Flow IR + Compiler Pipeline

## 1. Purpose

MIR (Mid-level Intermediate Representation) is the second compiler IR
layer, sitting between the typed HIR and the optimization/backend stages:

    Source → Lexer → Parser → AST → Semantic Analysis → Type Analysis
        → HIR → MIR → Optimization → Native Backend → executable

Where HIR mirrors the source's structural shape (nested blocks, one node
per source construct, no flattening), MIR is **linear and
control-flow-explicit**: every function is a directed graph of *basic
blocks*, each block is an ordered list of *statements* ending in exactly
one *terminator*, and every construct — `if`/`else`, `while`, `for`,
`loop`, `break`, `continue`, `return` — has been lowered into jumps,
branches, and returns.

MIR is a **durable output**, not a transient view: like the HIR it was
lowered from, it owns all of its data (names are copied strings, spans and
ids are copied values) and carries its own cloned type table, so it remains
valid after the HIR and the front end are dropped. Tooling (optimization,
the native backend, diagnostics) can rely on it without borrowing anything
upstream.

## 2. HIR-to-MIR Boundary

Lowering consumes one input and produces one output:

| Input                  | Role                                   |
| ---------------------- | -------------------------------------- |
| `&HirProgram` (session 08) | the typed, symbol-resolved, owned tree |

`mir::lower(&hir) -> Result<MirProgram, Vec<MirError>>`

The boundary is strict and one-directional:

- **no re-analysis** — lowering never re-runs name resolution, type
  checking, or HIR lowering; it only *consumes* the answers HIR already
  carries;
- **no duplicated systems** — MIR references `TypeId`s from the HIR's
  cloned `TypeTable` (re-cloned here so the program is self-contained) and
  `SymbolId`s where useful (module-item references, locals bound to source
  declarations); it defines its own `LocalId`/`BlockId` identity;
- **exact spans** — every node preserves the source span of the construct
  it was lowered from;
- **deterministic output** — items, locals, and blocks are produced in
  source order, so identical input always yields identical MIR;
- **structured failures** — internal inconsistencies are reported as
  `MirError`s (`E-M01`…`E-M11`) instead of panicking.

## 3. Node Design

| MIR node             | Notes                                             |
| -------------------- | ------------------------------------------------- |
| `MirProgram`         | `items: Vec<MirItem>` + cloned `types: TypeTable` |
| `MirItem`/`MirItemKind` | `Fn`, `Let`, `Const` (module scope)            |
| `MirFn`              | name, params (local ids), locals, blocks, `Fn` type |
| `MirLocal`           | name (empty for temporaries), optional symbol, type, mutability, span |
| `MirBlock`           | id, ordered statements, exactly one terminator, span |
| `MirStmt`/`MirStmtKind` | `Assign { target, rvalue }`                  |
| `MirTarget`/`MirTargetKind` | `Local`, `Static` (module storage), `Member`, `Index` |
| `MirRvalue`/`MirRvalueKind` | `Use`, `Unary`, `Binary`, `Call`, `Range`, `RangeNext`, `RangeFinished`, `Member`, `Index` |
| `MirOperand`/`MirOperandKind` | `Local` load, `Constant`, `Static` reference |
| `MirConstantKind`   | `Int`, `Float`, `Str`, `Char`, `Bool`, `Null`, `Enum { variant }` |
| `MirTerminator`      | `Return`, `Jump`, `Branch`                       |
| `MirStatic`          | module-level `let`/`const`: locals, statements, final value |

Key points:

- **Blocks are the unit of control flow.** A function's block list is
  ordered by id: the block at index `i` has id `i`, and the entry block is
  always block `0`. `MirFn::entry()` returns it.
- **Exactly one terminator per block — by construction.** Blocks are only
  produced by the lowering builder, which ends each block exactly once the
  moment it is terminated. A builder that somehow leaves a block
  unterminated reports `E-M06` (a structured error, never a panic).
- **Statements have no control flow.** The current language needs exactly
  one statement form — assignment into a target — which covers `let`/`const`
  bindings, plain and compound assignments, and temporary-producing
  expression evaluation. The enum is kept so later milestones (storage
  markers, debug info) can extend it.
- **Literals keep their raw text.** Like the AST and HIR, literal *values*
  are not decoded into the IR: the raw source text is recovered from the
  constant's span via the source map. Decoding belongs to a later
  milestone. The one exception is the session-17 enum-variant constant
  (`MirConstantKind::Enum { variant }`): the discriminant is computed by
  the compiler from the enum's variant table, so it is carried as a value
  and decodes directly to a word.
- **Member/index nodes are structural.** The memory-model milestone that
  defines their place semantics does not exist yet; base (and index) values
  are evaluated to operands and the place is preserved so valid programs
  lower cleanly.
- **`for` loop variables are mutable in MIR.** The loop machinery writes
  the loop-variable slot each iteration, so MIR marks it mutable even
  though source-level reassignment is still rejected by semantic analysis.
- **Module `let mut` is assignable.** A module-scope `let mut` binding can
  be assigned from any function; the `Static` target kind carries the
  declaration's `SymbolId` so references resolve across functions. When
  module-scope initialization runs is a backend concern.

## 4. Control-Flow Lowering

### 4.1 `if` / `else if` / `else`

    cond block:   cond = <condition>
                  branch cond → then, else
    then block:   <then statements>
                  jump after
    else block:   <else statements>
                  jump after
    after block:  ...

Both arms join at a **shared continuation block**. Nested `else if` nodes
receive the *same* continuation slot, so an `else if` chain joins at one
block instead of cascading through intermediates. When an arm diverges
(`return`/`break`/`continue`), no dead continuation block is produced: the
continuation is created lazily on first fall-through.

### 4.2 `while cond { body }`

    header block: cond = <condition>          ← continue target, loop-back target
                  branch cond → body, exit
    body block:   <body statements>
                  jump header
    exit block:   ...

The preceding block jumps into the header. `continue` and the natural
loop-back both jump to the header, which re-evaluates the condition;
`break` jumps to the exit.

### 4.3 `for var in iterable { body }`

A `for` loop lowers to a range iteration with four blocks:

    init block:   iter = <iterable value>
                  jump header
    header block: done = RangeFinished(iter)   ← continue target
                  branch done → exit, body
    body block:   var = RangeNext(iter)
                  <body statements>
                  jump header
    exit block:   ...

`continue` jumps to the header (which re-checks completion and lets the
body fetch the next element); `break` jumps to the exit. A syntactically
written range keeps its inclusive flag in the `Range` construction;
iteration over a range *value* defers inclusive-ness to the backend.

### 4.4 `loop { body }`

    header block: <body statements>            ← continue target, loop-back
                  jump header
    exit block:   ...

`break` jumps to the exit block. `loop` has no condition, so the body
block is its own header.

### 4.5 `break` / `continue`

`break` and `continue` lower to unconditional jumps to the enclosing
loop's exit and continue targets. The targets are tracked on a stack
(innermost loop last) as blocks are built, so nested loops resolve
correctly. Semantic analysis rejects `break`/`continue` outside a loop; if
an internally inconsistent HIR reaches lowering anyway, `E-M01`/`E-M02` is
reported and lowering continues.

### 4.6 `return`

`return;` and `return expr;` lower to a `Return` terminator with an
optional value operand. Falling off the end of a function body is a bare
return. A `return` (or `break`/`continue`) that follows a terminator is
unreachable dead code and is skipped rather than starting a new block.

### 4.7 Assignments

A plain assignment stores the (already evaluated) value into the target.
A compound assignment (`x += v`) loads the target's current value, applies
the corresponding binary operator, and stores the result. The value is
evaluated before the target, deterministically.

## 5. Deterministic Block Ordering

Blocks are *allocated* when their construct is entered but *finalized*
(pushed into the block list) when they are terminated. A loop's exit block
— allocated before the body's nested blocks — can therefore be finalized
*after* them. A final renumbering pass assigns every finalized block its
final id in list order and rewrites every terminator target, restoring the
invariant that the block at index `i` has id `i`, with the entry block
first. The pass is deterministic for identical input, and validation
(`E-M10`) guards the invariant for hand-built or tooling-built MIR.

## 6. Validation

`mir::validate(&program) -> Result<(), Vec<MirError>>` checks structural
integrity before the MIR is trusted by later stages and tooling:

- **valid block references** — every terminator target exists (`E-M07`);
- **valid local references** — every statement, operand, and parameter
  references an existing local (`E-M08`);
- **valid type references** — every `TypeId` resolves in the program's
  type table (`E-M09`);
- **deterministic block ordering** — the block at index `i` has id `i`, so
  the entry block is first (`E-M10`);
- **valid parameters** — parameters are the first locals, in order
  (`E-M11`).

Missing terminators are impossible by construction (see §3); a builder
that somehow leaves a block unterminated reports `E-M06`. Lowering always
produces valid MIR, so validation exists to defend the pipeline and
tooling against malformed hand-built or mutated programs, and every
problem found is reported in deterministic order instead of panicking.

## 7. Error Handling

| Code | Kind                   | Meaning                                        |
| ---- | ---------------------- | ---------------------------------------------- |
| E-M01| `BreakOutsideLoop`     | `break` has no enclosing loop                  |
| E-M02| `ContinueOutsideLoop`  | `continue` has no enclosing loop               |
| E-M03| `NonRangeForIterable`  | a `for` loop iterates a non-range value        |
| E-M04| `UnresolvedLocal`      | an identifier is neither a local nor a module item |
| E-M05| `InvalidAssignmentTarget` | an assignment target is not a place        |
| E-M06| `MissingTerminator`    | a block was left without a terminator          |
| E-M07| `InvalidBlockReference`| a terminator references a block that does not exist |
| E-M08| `InvalidLocalReference`| a statement/operand references an unknown local |
| E-M09| `InvalidTypeReference` | a node references a type not in the table      |
| E-M10| `BlockIdMismatch`      | the block at index `i` has a different id      |
| E-M11| `ParamLocalMismatch`   | parameter locals are not the first locals      |

`MirError` carries the kind, the exact span, and (for the validation
errors) a rendered detail. `CheckError::Mir` renders them uniformly with
the other stages' diagnostics.

## 8. Driver and CLI Integration

`driver::check` now runs MIR lowering and validation when HIR lowering
succeeded:

- a clean front end is lowered to HIR, then to MIR, then structurally
  validated; on success `CheckReport.mir` carries the [`MirProgram`];
- a lowering or validation failure on a clean pipeline is an internal
  compiler error and is reported as `E-M01`…`E-M11` (exit 1);
- a front end with semantic, type, or HIR errors is **not** lowered to
  MIR — lowering an inconsistent pipeline would only add misleading
  diagnostics;
- `mink check` therefore validates through MIR: valid programs report
  `passed parsing, semantic analysis, type checking, HIR lowering, and MIR
  lowering (N tokens)` and exit 0; every error class exits 1;
- `mink build` compiles the optimized MIR through the native backend (see
  `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`) and writes an
  executable; `mink check` still stops after optimization.

## 9. Backend Boundary

The native backend (`src/backend/`) consumes the **optimized** MIR graph:
control flow is explicit (basic blocks, terminators), values are
operand/rvalue pairs, and calls, ranges, and range iteration are distinct
rvalue forms. The optimization passes (see
`docs/implementation/OPTIMIZATION_IMPLEMENTATION.md`) rewrite the graph in
place and the backend lowers it into a portable instruction representation
before emission. Nothing in MIR pre-commits to a particular backend; the
explicit CFG and canonical types are the intended seam.

## 10. Known Limitations

- Literal values are not decoded into the IR (matching the AST and HIR):
  the raw text is recovered from the node's span via the source map.
  Decoding belongs to a later milestone.
- `RangeNext`/`RangeFinished` iteration semantics (order, effect on an
  exhausted range) are backend semantics; MIR preserves the structure only.
- Member and index places are structural (evaluated base/index values, no
  memory-model semantics); the memory-model milestone defines their place
  semantics.
- Module-scope `Static` targets reference declarations by `SymbolId`; MIR
  carries no symbol table (the front end guarantees existence), and when
  module-scope initialization runs is a backend concern.
- Optimization is implemented (session 10, `OPTIMIZATION_IMPLEMENTATION.md`)
  and is conservative: boolean folds, copy propagation, CFG cleanup, and
  dead-code elimination run. Code generation consumes the optimized graph
  (session 11, `NATIVE_BACKEND_IMPLEMENTATION.md`).

## 11. Tests

Coverage lives in `tests/mir.rs` (34 tests) plus the internal-failure unit
tests in `src/mir/lower.rs` (5), the validation unit tests in
`src/mir/validate.rs` (7), and the CLI tests in `tests/cli.rs` (3 new, 61
total):

- **Functions/locals**: parameter locals come first; bindings, temporaries,
  mutability, and local types.
- **Assignments**: plain and compound (`+=`) lowering, targets, stores.
- **Calls and arithmetic**: unary/binary rvalues, call rvalues with
  callee/argument operands.
- **Control flow**: if/else, nested branches (else-if chains join at one
  continuation), while, for (init/header/body/exit blocks, `RangeNext`/
  `RangeFinished`, inclusive ranges), loop, break, continue, returns.
- **Nested control flow**: loops inside branches, branches inside loops,
  deep control-flow programs.
- **Block references**: terminator targets exist; block ordering (the block
  at index `i` has id `i`, entry is block 0).
- **Terminators**: return/jump/branch shapes, bare vs. valued returns.
- **Spans and types**: every node preserves the construct's span; all types
  resolve in the cloned table.
- **Malformed/internal cases**: dangling block references, dangling local
  references, unknown type references, unordered block ids, misplaced
  parameter locals, statics — every structural class produces exactly the
  expected `E-M07`…`E-M11`, never a panic.
- **CLI**: valid programs report the message through MIR lowering; MIR
  errors are not claimed on failing front ends.

## 12. Quality Gates

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
**700 tests**, and after session 14 (structs + arrays, with member/index
rvalues and the multi-step place representation) it is **762 tests**,and after session 15 (ownership analysis, a compile-time gate before MIR
lowering) it is **803 tests**, and after session 16 (references and
borrowing) it is **878 tests** (see `OPTIMIZATION_IMPLEMENTATION.md` §7),
and after session 17 (data-free enums, with the `Enum` variant constant)
it is **919 tests** (see `OPTIMIZATION_IMPLEMENTATION.md` §7).
