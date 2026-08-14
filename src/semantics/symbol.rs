//! Symbol and scope model for semantic analysis.
//!
//! The analyzer builds a table of **symbols** — one per declaration — and a
//! forest of **lexical scopes** that declares their visibility. Both use
//! lightweight, stable numeric ids so later compiler stages (type checking,
//! HIR lowering, LSP tooling) can reference declarations without re-running
//! name resolution or cloning AST nodes.
//!
//! A [`Symbol`] carries its stable [`SymbolId`], name, declaration kind
//! ([`SymbolKind`]), declaration [`Span`](crate::source::Span), and the
//! [`ScopeId`] it was declared in. The [`SymbolTable`] owns all symbols of a
//! program; the [`ScopeTable`] owns the scope forest, where each scope links
//! to its parent and records the symbols declared directly inside it.
//!
//! Scopes keep an internal name index for constant-time lookup; the public
//! surface exposes only read accessors so the analyzer's implementation can
//! evolve without breaking consumers. The shadowing, duplicate-detection,
//! and declaration-order rules these structures implement are documented in
//! `docs/language/CORE_LANGUAGE.md` §24 and
//! `docs/implementation/SEMANTIC_ANALYSIS_IMPLEMENTATION.md`.

use std::collections::HashMap;

use crate::source::Span;

/// Stable identity of a declared symbol within one analysis result.
///
/// Ids are assigned sequentially as declarations are collected, in
/// deterministic source order, and remain valid for the lifetime of the
/// [`SymbolTable`] that created them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(u32);

impl SymbolId {
    /// The raw numeric value of this id.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Creates an id from its raw numeric value.
    ///
    /// Ids should normally be produced by the analyzer; constructing one
    /// directly is only useful for tests and tooling that manages symbols
    /// itself.
    pub(crate) fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Stable identity of a lexical scope within one analysis result.
///
/// Ids are assigned sequentially as scopes are entered, in deterministic
/// source order, and remain valid for the lifetime of the [`ScopeTable`]
/// that created them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopeId(u32);

impl ScopeId {
    /// The raw numeric value of this id.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Creates an id from its raw numeric value.
    ///
    /// Ids should normally be produced by the analyzer; constructing one
    /// directly is only useful for tests and tooling that manages scopes
    /// itself.
    pub(crate) fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// What kind of declaration a [`Symbol`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    /// A `fn` function declaration.
    Fn,
    /// A function parameter.
    Param,
    /// A `let` binding; `mutable` is true only for `let mut`.
    Let {
        /// Whether the binding is mutable (`let mut`).
        mutable: bool,
    },
    /// A `const` binding.
    Const,
    /// A `for` loop variable.
    ForVar,
    /// A predeclared runtime intrinsic (`rt_alloc`, `rt_free`, …). The
    /// intrinsic names are reserved: declaring a module item with the same
    /// name is a duplicate definition. Intrinsics have no source
    /// declaration; their span is synthetic.
    Intrinsic,
}

impl SymbolKind {
    /// Whether a declaration of this kind can be reassigned.
    ///
    /// Only `let mut` bindings are mutable in the current language; `let`,
    /// `const`, parameters, `for` variables, and function names are not.
    pub fn is_mutable(self) -> bool {
        matches!(self, Self::Let { mutable: true })
    }
}

/// A declared name: the stable identity of a declaration plus the minimum
/// metadata later compiler stages need.
///
/// Symbols are lightweight by design — they store the declaration name and
/// span, not a copy of any AST node, so the table never duplicates
/// declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// The stable identity of this symbol.
    pub id: SymbolId,
    /// The declared name (source spelling).
    pub name: String,
    /// The declaration kind.
    pub kind: SymbolKind,
    /// Span of the declared identifier token.
    pub span: Span,
    /// The scope this symbol is declared in.
    pub scope: ScopeId,
}

/// All symbols declared by one analyzed program.
///
/// Symbols are pushed in deterministic order (module scope first in source
/// order, then function bodies in traversal order), so ids are stable and
/// reproducible for identical input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolTable {
    symbols: Vec<Symbol>,
}

impl SymbolTable {
    /// Creates an empty symbol table.
    pub(crate) fn new() -> Self {
        Self {
            symbols: Vec::new(),
        }
    }

    /// Registers a new symbol and returns its id.
    pub(crate) fn push(
        &mut self,
        kind: SymbolKind,
        name: String,
        span: Span,
        scope: ScopeId,
    ) -> SymbolId {
        let id = SymbolId::new(self.symbols.len() as u32);
        self.symbols.push(Symbol {
            id,
            name,
            kind,
            span,
            scope,
        });
        id
    }

    /// The symbol registered under `id`, if any.
    pub fn get(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id.raw() as usize)
    }

    /// Iterates over all symbols in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.iter()
    }

    /// Number of declared symbols.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Whether no symbols are declared.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// What kind of block a [`Scope`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    /// The module scope holding top-level declarations.
    Module,
    /// A function's declaration scope: parameters and the function body's
    /// own declarations share this scope.
    Function,
    /// A nested block scope (`if`/`else`/loop body, nested `{ ... }`).
    Block,
}

/// A lexical scope: an ordered list of the symbols declared directly in it,
/// plus a name index for lookup.
///
/// Shadowing happens across scopes: a scope may declare a name that already
/// exists in an ancestor, and name resolution finds the innermost
/// declaration. A scope may not declare the same name twice (see
/// `docs/language/CORE_LANGUAGE.md` §24).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// The stable identity of this scope.
    pub id: ScopeId,
    /// The kind of scope.
    pub kind: ScopeKind,
    /// The enclosing scope, if any. The module scope has no parent.
    pub parent: Option<ScopeId>,
    /// Symbols declared directly in this scope, in declaration order.
    symbols: Vec<SymbolId>,
    /// Name index for constant-time lookup within this scope only.
    names: HashMap<String, SymbolId>,
}

impl Scope {
    /// Symbols declared directly in this scope, in declaration order
    /// (not including declarations from ancestor scopes).
    pub fn symbols(&self) -> &[SymbolId] {
        &self.symbols
    }

    /// Looks up a name declared **directly** in this scope.
    ///
    /// The lookup does not walk ancestor scopes; use the analyzer's
    /// resolution for full lexical lookup.
    pub fn lookup(&self, name: &str) -> Option<SymbolId> {
        self.names.get(name).copied()
    }

    /// Registers `symbol` as declared in this scope.
    pub(crate) fn bind(&mut self, name: String, symbol: SymbolId) {
        self.symbols.push(symbol);
        self.names.insert(name, symbol);
    }
}

/// All lexical scopes of one analyzed program, in creation order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeTable {
    scopes: Vec<Scope>,
}

impl ScopeTable {
    /// Creates an empty scope table.
    pub(crate) fn new() -> Self {
        Self { scopes: Vec::new() }
    }

    /// Creates a new scope of `kind` nested inside `parent` (if any) and
    /// returns its id.
    pub(crate) fn push(&mut self, kind: ScopeKind, parent: Option<ScopeId>) -> ScopeId {
        let id = ScopeId::new(self.scopes.len() as u32);
        self.scopes.push(Scope {
            id,
            kind,
            parent,
            symbols: Vec::new(),
            names: HashMap::new(),
        });
        id
    }

    /// The scope registered under `id`, if any.
    pub fn get(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.get(id.raw() as usize)
    }

    /// The scope registered under `id`, for mutation by the analyzer.
    pub(crate) fn get_mut(&mut self, id: ScopeId) -> Option<&mut Scope> {
        self.scopes.get_mut(id.raw() as usize)
    }

    /// Iterates over all scopes in creation order.
    pub fn iter(&self) -> impl Iterator<Item = &Scope> {
        self.scopes.iter()
    }

    /// Number of scopes.
    pub fn len(&self) -> usize {
        self.scopes.len()
    }

    /// Whether no scopes are registered.
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }
}
