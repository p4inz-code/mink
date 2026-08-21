//! Semantic analysis: establishing that a syntactically valid program is
//! semantically coherent according to the rules currently supported by MINK.
//!
//! The analyzer walks the parsed [`Ast`](crate::ast::Ast) once and produces a
//! [`SemanticResult`] containing:
//!
//! - **symbols** — one [`Symbol`] per declaration (`fn`, `let`, `let mut`,
//!   `const`, parameters, `for` variables), with stable ids, declaration
//!   spans, and scope membership;
//! - **scopes** — the lexical scope forest (module → function → blocks) with
//!   parent links and per-scope declarations;
//! - **resolutions** — identifier references mapped to the exact symbols they
//!   resolve to, so later stages (type checking, HIR lowering, LSP) never
//!   re-run name resolution;
//! - **errors** — semantic diagnostics with stable codes (`E-S01`…`E-S07`),
//!   exact spans, and the original declaration span for duplicates.
//!
//! Semantic rules (duplicate definitions, shadowing, declaration order,
//! mutability, control-flow context) are documented in
//! `docs/language/CORE_LANGUAGE.md` §24; the full design record is in
//! `docs/implementation/SEMANTIC_ANALYSIS_IMPLEMENTATION.md`.
//!
//! The pipeline continues from the parser:
//!
//! ```text
//! AST → semantic analysis → SemanticResult → future type analysis
//! ```
//!
//! Type checking, type inference, ownership/borrowing, HIR, and all later
//! stages are explicitly out of scope for this module.

mod analyzer;
mod error;
mod symbol;

use crate::ast::Ast;
use crate::source::Span;

pub use error::{SemanticError, SemanticErrorKind};
pub use symbol::{
    Scope, ScopeId, ScopeKind, ScopeTable, Symbol, SymbolId, SymbolKind, SymbolTable,
};

/// Runs semantic analysis over a parsed `ast`.
///
/// Returns a [`SemanticResult`] regardless of whether the program is valid;
/// validity is determined by [`SemanticResult::has_errors`]. Analysis is
/// deterministic and continues past independent errors (see the module
/// documentation and `docs/language/CORE_LANGUAGE.md` §24).
pub fn analyze(ast: &Ast) -> SemanticResult {
    analyzer::analyze_ast(ast, None)
}

/// Runs semantic analysis with access to a cross-module registry.
///
/// When a `use` import is encountered, the registry is consulted to
/// resolve the imported symbol from another module.
pub fn analyze_with_registry(
    ast: &Ast,
    registry: &crate::module::ModuleRegistry,
    module_name: &str,
) -> SemanticResult {
    analyzer::analyze_ast(ast, Some((registry, module_name)))
}

/// The result of running semantic analysis on one program.
///
/// This is the durable output later compiler stages consume: symbols with
/// stable ids, the lexical scope forest, and every resolved name reference.
/// The result is `Clone` + `PartialEq` + `Eq` so tests can assert exact
/// outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticResult {
    symbols: SymbolTable,
    scopes: ScopeTable,
    /// Resolved references, sorted by span start for deterministic iteration
    /// and binary-search lookup.
    resolutions: Vec<(Span, SymbolId)>,
    /// Or-pattern binding aliases (session 27): span → symbol for every
    /// occurrence of an or-pattern binding after its first. These are the
    /// one place a non-first occurrence of one logical binding is recorded,
    /// so later stages can type and lower every occurrence. Sorted by span
    /// start like `resolutions`.
    binding_aliases: Vec<(Span, SymbolId)>,
    errors: Vec<SemanticError>,
}

impl SemanticResult {
    /// Assembles a result from the analyzer's tables. References **and**
    /// errors are sorted by span start so iteration, lookup, and diagnostic
    /// order are deterministic: module-scope duplicate errors are recorded
    /// during the declaration pre-pass, before body analysis, so an explicit
    /// stable sort restores source order.
    pub(crate) fn new(
        symbols: SymbolTable,
        scopes: ScopeTable,
        mut resolutions: Vec<(Span, SymbolId)>,
        mut binding_aliases: Vec<(Span, SymbolId)>,
        mut errors: Vec<SemanticError>,
    ) -> Self {
        resolutions.sort_by_key(|(span, _)| span.start());
        binding_aliases.sort_by_key(|(span, _)| span.start());
        errors.sort_by_key(|error| error.span().start());
        Self {
            symbols,
            scopes,
            resolutions,
            binding_aliases,
            errors,
        }
    }

    /// Semantic errors, in source order. Empty for a semantically valid
    /// program.
    pub fn errors(&self) -> &[SemanticError] {
        &self.errors
    }

    /// Whether the program produced any semantic errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// All declared symbols.
    pub fn symbols(&self) -> &SymbolTable {
        &self.symbols
    }

    /// All lexical scopes.
    pub fn scopes(&self) -> &ScopeTable {
        &self.scopes
    }

    /// Every resolved name reference, in span order: `(identifier span,
    /// resolved symbol id)`. Declaration names are not references and do not
    /// appear here.
    pub fn resolutions(&self) -> &[(Span, SymbolId)] {
        &self.resolutions
    }

    /// Or-pattern binding aliases, in span order: `(occurrence span,
    /// resolved symbol id)` for every occurrence of an or-pattern binding
    /// after its first. The first occurrence is an ordinary symbol (see
    /// [`Self::symbols`]); these aliases resolve the rest.
    pub fn binding_aliases(&self) -> &[(Span, SymbolId)] {
        &self.binding_aliases
    }

    /// The symbol the identifier at `span` resolves to, if any.
    ///
    /// Answers "which symbol does this identifier refer to?" without
    /// re-running name resolution.
    pub fn resolve(&self, span: Span) -> Option<SymbolId> {
        self.resolutions
            .binary_search_by_key(&span.start(), |(resolved_span, _)| resolved_span.start())
            .ok()
            .map(|index| self.resolutions[index].1)
    }
}
