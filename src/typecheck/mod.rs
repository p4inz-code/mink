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
use crate::source::{SourceMap, Span};

pub use error::{TypeError, TypeErrorKind};
pub use ty::{
    EnumId, EnumInfo, EnumVariantInfo, StructFieldInfo, StructId, StructInfo, TypeId, TypeKind,
    TypeTable,
};

/// A serializable function signature for cross-module type sharing.
///
/// Unlike [`TypeKind::Fn`], this stores scalar type names instead of
/// [`TypeId`]s, so it can be constructed in one module's [`TypeTable`]
/// and consumed in another module's table without stale id references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalFnSig {
    /// The concrete type name for each parameter.
    pub params: Vec<ScalarType>,
    /// The concrete type name for the return value.
    pub result: ScalarType,
}

/// A scalar type that can be serialized across module boundaries.
///
/// This covers the V1 language's non-parametric types. Parameterized
/// types (e.g. `Ptr<T>`, `Range<T>`) are not yet importable across
/// modules and will be added in a later milestone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalarType {
    /// 64-bit integer type.
    Int,
    /// 64-bit floating-point type.
    Float,
    /// Boolean type.
    Bool,
    /// Unicode scalar value type.
    Char,
    /// String type.
    Str,
    /// Null literal type.
    Null,
    /// Unit (zero-width) type.
    Unit,
    /// Unresolved inference variable.
    Infer,
}

impl ScalarType {
    /// Converts a scalar type to the corresponding [`TypeKind`].
    pub fn to_type_kind(self) -> TypeKind {
        match self {
            Self::Int => TypeKind::Int,
            Self::Float => TypeKind::Float,
            Self::Bool => TypeKind::Bool,
            Self::Char => TypeKind::Char,
            Self::Str => TypeKind::Str,
            Self::Null => TypeKind::Null,
            Self::Unit => TypeKind::Unit,
            Self::Infer => TypeKind::Infer(None),
        }
    }

    /// Converts a [`TypeKind`] to a scalar type, if possible.
    /// Non-scalar types (references, pointers, tuples, etc.) return `None`.
    pub fn from_type_kind(kind: &TypeKind) -> Option<Self> {
        match kind {
            TypeKind::Int => Some(Self::Int),
            TypeKind::Float => Some(Self::Float),
            TypeKind::Bool => Some(Self::Bool),
            TypeKind::Char => Some(Self::Char),
            TypeKind::Str => Some(Self::Str),
            TypeKind::Null => Some(Self::Null),
            TypeKind::Unit => Some(Self::Unit),
            TypeKind::Infer(_) => Some(Self::Infer),
            _ => None,
        }
    }
}

/// The human-readable reason a layout could not be computed (shared with
/// the backend lowering, which validates aggregate layouts against the
/// same deterministic engine).
pub(crate) use checker::layout_error_message;

/// Runs type analysis over `ast`, consuming the session-05 semantic result
/// and reading literal source text through `sources` (used for the
/// null-pointer-constant rule: the integer literal `0` is the null pointer
/// in a pointer-typed argument position; see
/// `docs/implementation/STRING_MEMORY_IMPLEMENTATION.md`).
///
/// Returns a [`TypeResult`] regardless of whether the program is
/// well-typed; validity is determined by [`TypeResult::has_errors`].
/// Analysis is deterministic and continues past independent errors: failed
/// sub-expressions receive the unknown/error type so one root error does
/// not cascade into misleading secondary diagnostics.
pub fn check(ast: &Ast, semantic: &SemanticResult, sources: &SourceMap) -> TypeResult {
    checker::check_ast(ast, semantic, sources, &std::collections::HashMap::new())
}

/// Runs type analysis with pre-resolved types for imported symbols.
///
/// `external_types` maps a symbol's name to its type kind (e.g. the
/// function signature from another module). Symbols present in this map
/// receive the given type instead of `Infer(None)`.
pub fn check_with_external_types(
    ast: &Ast,
    semantic: &SemanticResult,
    sources: &SourceMap,
    external_types: &std::collections::HashMap<String, crate::typecheck::ExternalFnSig>,
) -> TypeResult {
    checker::check_ast(ast, semantic, sources, external_types)
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

    /// The type of the expression covering **exactly** `span`, if recorded.
    ///
    /// [`TypeResult::expr_type`] matches by span start, which is ambiguous
    /// when one expression is a prefix of another (`1` and `1 + 2` share a
    /// start). This lookup requires the full span to match, which uniquely
    /// identifies one expression node; it is what HIR lowering uses to give
    /// every lowered expression its precise type.
    ///
    /// Parser-produced expression nodes cover unique exact spans; only
    /// hand-built ASTs could contain two nodes with the same exact span,
    /// and for those the first in stable (traversal) order wins.
    pub fn expr_type_exact(&self, span: Span) -> Option<TypeId> {
        let start = span.start();
        let lower = self
            .expr_types
            .partition_point(|(expr_span, _)| expr_span.start() < start);
        self.expr_types[lower..]
            .iter()
            .take_while(|(expr_span, _)| expr_span.start() == start)
            .find(|(expr_span, _)| *expr_span == span)
            .map(|(_, ty)| *ty)
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
