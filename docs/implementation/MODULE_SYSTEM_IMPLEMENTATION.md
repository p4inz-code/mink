# Module System / Multi-File Compilation — Session 34

## Overview

Session 34 implements the MINK module system: multi-file compilation
with `mod` declarations, `use` imports, and `pub` visibility. The module
system enables real multi-file MINK projects for the first time.

## Architecture

### Flatten-and-Compile Approach

The multi-module pipeline uses a **flatten-and-compile** strategy:

1. **Module discovery**: `discover_modules` recursively follows `mod name;`
   declarations from the root file, loading each child module from the
   filesystem as `name.mink` in the same directory as the parent.

2. **AST flattening**: All items from child modules are collected and
   appended to the root module's item list. `mod` and `use` declarations
   are stripped from the combined AST.

3. **Single-compilation-unit analysis**: The combined AST is run through
   the standard single-module pipeline: semantic analysis, type checking,
   ownership analysis, HIR lowering, MIR lowering, and optimization.

This approach avoids the complexity of cross-module SymbolId and TypeId
merging by treating all modules as a single compilation unit. Each
module's items become top-level declarations in the root scope.

### Why Not Separate Compilation?

Cross-module compilation requires unified symbol tables and type tables.
Independent per-module semantic analysis creates different SymbolIds for
the same logical entity (e.g., an import and its definition). Merging
these across compilation boundaries requires deep changes to every
compiler stage. The flatten approach achieves correct multi-file semantics
without invasive pipeline changes.

### Module Discovery

```
Root file (main.mink)
├── mod math;     → loads math.mink from same directory
├── mod greet;    → loads greet.mink from same directory
└── (recursive)   → each child can also declare mod items
```

- Circular modules are detected via canonicalized path deduplication.
- Missing module files produce a semantic error diagnostic.
- Module names are derived from file stems (no extension).

### Visibility

- `pub fn`, `pub struct`, `pub enum`, `pub let`, `pub const` are
  public and visible across module boundaries.
- Non-public items are included in the flattened AST (they may be
  needed by public items that reference them).
- V1 does not enforce private-vs-public access across module boundaries;
  all items from all modules are included in the combined AST.

### Imports (`use`)

- `use mod_name::item_name;` brings `item_name` into scope.
- `use mod_name;` is a no-op (module paths in expressions are a later
  milestone).
- Resolved by the semantic analyzer via the cross-module registry.
- `use` declarations are stripped from the combined AST during flattening.

## Files Changed

| File | Changes |
|------|---------|
| `src/driver.rs` | Multi-module pipeline: discovery, flattening, compilation |
| `src/module/mod.rs` | Module system data structures (ModuleTree, ModuleRegistry, etc.) |
| `src/ast/mod.rs` | `ModuleDecl`, `UseDecl`, `PubItem`, `ItemKind::Module/Use/Pub` |
| `src/parser/mod.rs` | `parse_mod_decl`, `parse_use_decl`, `parse_pub_item` |
| `src/semantics/analyzer.rs` | `resolve_imports` for `use` declarations |
| `src/semantics/mod.rs` | `analyze_with_registry` entry point |
| `src/typecheck/mod.rs` | `ExternalFnSig`, `ScalarType` for cross-module type sharing |
| `src/typecheck/checker.rs` | `pre_register` handles `pub fn` in fn_info, external types |
| `src/hir/mod.rs` | `imported_symbols` field, `lower_with_imports` export |
| `src/hir/lower.rs` | `lower_with_imports` for cross-module imported symbols |
| `src/mir/lower.rs` | imported symbols in `module_symbols` |
| `src/ownership/mod.rs` | `Pub` item handling |
| `src/typecheck/ty.rs` | `TypeTable::merge_from` for type table merging |

## Pipeline Flow

```
Source files
  ↓
discover_modules (recursive mod declarations)
  ↓
Parse each module
  ↓
Flatten: combine all items into one AST, strip mod/use
  ↓
Semantic analysis (single pass over combined AST)
  ↓
Type checking (single pass)
  ↓
Ownership analysis
  ↓
HIR lowering
  ↓
MIR lowering + optimization
  ↓
Backend compilation → native executable
```

## Test Results

- 1,603 tests passing (1,596 pre-existing + 7 new module tests)
- All quality gates clean (fmt, clippy, test, build, release)

## Known Limitations

- **Module paths in expressions**: `module::item` syntax in expression
  context is not supported (only `use` imports bring names into scope).
- **Nested inline modules**: `mod name { ... }` with inline blocks is
  not supported; only file-based `mod name;` is implemented.
- **Private access enforcement**: V1 does not enforce private-vs-public
  access across module boundaries. All items from all modules are
  included in the flattened AST.
- **`pub use` re-exports**: Re-exporting imported items via `pub use`
  is not yet supported.
