//! Type analysis: the AST → type-checking stage of the compiler pipeline.
//!
//! The type checker consumes the session-05 [`SemanticResult`] and the
//! parsed [`Ast`] and produces a [`TypeResult`] containing the type of
//! every expression, the type of every symbol (indexed by [`SymbolId`]),
//! and type diagnostics. It never re-runs name resolution or scope
//! construction: identifier references are resolved through
//! [`SemanticResult::resolve`].
//!
//! The pipeline continues from semantic analysis:
//!
//! ```text
//! AST → semantic analysis → type analysis → TypeResult → future HIR
//! ```
//!
//! The type representation — core types, inference variables, the
//! unknown/error type, and unification — and every typing rule are
//! documented in `docs/implementation/TYPE_SYSTEM_IMPLEMENTATION.md` and
//! `docs/language/CORE_LANGUAGE.md` §26. The inference behavior —
//! constraint propagation, bidirectional checking, function/return
//! inference, recursion, and unresolved-type handling — is documented in
//! `docs/implementation/TYPE_INFERENCE_IMPLEMENTATION.md`.

mod checker;
mod error;
mod ty;

use crate::ast::Ast;
use crate::semantics::{SemanticResult, SymbolId};
use crate::source::Span;

pub use error::{TypeError, TypeErrorKind};
pub use ty::{TypeId, TypeKind, TypeTable};

/// Runs type analysis over `ast`, consuming the session-05 semantic result.
///
/// Returns a [`TypeResult`] regardless of whether the program is
/// well-typed; validity is determined by [`TypeResult::has_errors`].
/// Analysis is deterministic and continues past independent errors: failed
/// sub-expressions receive the unknown/error type so one root error does
/// not cascade into misleading secondary diagnostics.
pub fn check(ast: &Ast, semantic: &SemanticResult) -> TypeResult {
    checker::check_ast(ast, semantic)
}

/// The result of running type analysis on one program.
///
/// This is the durable output later compiler stages (HIR lowering,
/// tooling) consume: the type of every declared symbol (indexed by
/// [`SymbolId`]), the type of every expression (keyed by its exact span),
/// and the type diagnostics. The result is `Clone` + `PartialEq` + `Eq` so
/// tests and tooling can assert exact outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeResult {
    errors: Vec<TypeError>,
    /// Per-symbol types, indexed by `SymbolId::raw()`.
    symbol_types: Vec<TypeId>,
    /// Expression types, sorted by span start.
    expr_types: Vec<(Span, TypeId)>,
    types: TypeTable,
}

impl TypeResult {
    /// Assembles a result. Errors and expression types are sorted by span
    /// start so iteration, lookup, and diagnostic order are deterministic.
    pub(crate) fn new(
        mut errors: Vec<TypeError>,
        symbol_types: Vec<TypeId>,
        mut expr_types: Vec<(Span, TypeId)>,
        types: TypeTable,
    ) -> Self {
        errors.sort_by_key(|error| error.span().start());
        expr_types.sort_by_key(|(span, _)| span.start());
        Self {
            errors,
            symbol_types,
            expr_types,
            types,
        }
    }

    /// Type diagnostics, in source order. Empty for a well-typed program.
    pub fn errors(&self) -> &[TypeError] {
        &self.errors
    }

    /// Whether the program produced any type errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// The type the symbol with id `symbol` was inferred to have, if the id
    /// is in range.
    pub fn symbol_type(&self, symbol: SymbolId) -> Option<TypeId> {
        self.symbol_types.get(symbol.raw() as usize).copied()
    }

    /// The type of the expression covering exactly `span`, if recorded.
    ///
    /// Expression nodes cover unique spans within a file, so a span
    /// identifies one expression; the lookup is a binary search.
    pub fn expr_type(&self, span: Span) -> Option<TypeId> {
        self.expr_types
            .binary_search_by_key(&span.start(), |(expr_span, _)| expr_span.start())
            .ok()
            .map(|index| self.expr_types[index].1)
    }

    /// Every recorded expression type, in span order: `(expression span,
    /// type)`.
    pub fn expr_types(&self) -> &[(Span, TypeId)] {
        &self.expr_types
    }

    /// The type table: type identity, canonicalization, and display names.
    pub fn types(&self) -> &TypeTable {
        &self.types
    }
}
