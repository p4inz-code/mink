//! HIR lowering-error model.
//!
//! Lowering consumes the AST plus the semantic and type results and
//! produces a [`HirProgram`](super::HirProgram). For a valid front end
//! (clean semantic and type analysis) lowering always succeeds; lowering
//! errors represent internal inconsistencies that should never occur in a
//! well-formed pipeline (an identifier with no resolved symbol, a symbol or
//! expression with no recorded type, a function symbol whose type is not a
//! function type). They are reported structurally instead of panicking, so
//! malformed hand-built or tooling-produced ASTs fail cleanly and
//! deterministically.
//!
//! Codes `E-H01` … `E-H03` continue the established stable ranges (`E-L*`
//! lexical, `E-P*` syntax, `E-S*` semantic, `E-T*` type); the full catalog
//! is in `docs/implementation/HIR_IMPLEMENTATION.md`.

use std::fmt;

use crate::source::Span;

/// The category of a HIR lowering error.
///
/// Every category has a stable machine-readable code
/// ([`HirErrorKind::code`]) and a human-readable message
/// ([`fmt::Display`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirErrorKind {
    /// An identifier (a reference or a declaration name) has no resolved
    /// symbol in the semantic result.
    UnresolvedSymbol,
    /// A symbol or expression has no recorded type in the type result.
    MissingType,
    /// A function's symbol type is not a function type.
    InvalidFunctionType,
}

impl HirErrorKind {
    /// Stable machine-readable code for this error category.
    pub fn code(self) -> &'static str {
        match self {
            Self::UnresolvedSymbol => "E-H01",
            Self::MissingType => "E-H02",
            Self::InvalidFunctionType => "E-H03",
        }
    }
}

/// A single HIR lowering error: a category, the span it applies to, and a
/// rendered detail where useful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirError {
    kind: HirErrorKind,
    span: Span,
    /// Rendered detail (for example the offending type name), where
    /// applicable.
    detail: Option<String>,
}

impl HirError {
    /// Creates an unresolved-symbol error at `span` (`E-H01`).
    pub fn unresolved(span: Span) -> Self {
        Self {
            kind: HirErrorKind::UnresolvedSymbol,
            span,
            detail: None,
        }
    }

    /// Creates a missing-type error at `span` (`E-H02`).
    pub fn missing_type(span: Span) -> Self {
        Self {
            kind: HirErrorKind::MissingType,
            span,
            detail: None,
        }
    }

    /// Creates an invalid-function-type error at `span` (`E-H03`), where
    /// `actual` renders the offending type.
    pub fn invalid_function_type(span: Span, actual: impl Into<String>) -> Self {
        Self {
            kind: HirErrorKind::InvalidFunctionType,
            span,
            detail: Some(actual.into()),
        }
    }

    /// The category of this error.
    pub fn kind(&self) -> HirErrorKind {
        self.kind
    }

    /// The stable machine-readable code of this error (e.g. `E-H01`).
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// The source span this error applies to.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Rendered detail, when applicable.
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

impl fmt::Display for HirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            HirErrorKind::UnresolvedSymbol => {
                f.write_str("cannot lower: identifier has no resolved symbol")
            }
            HirErrorKind::MissingType => f.write_str("cannot lower: no recorded type"),
            HirErrorKind::InvalidFunctionType => {
                write!(f, "cannot lower function: not a function type")?;
                if let Some(actual) = &self.detail {
                    write!(f, " (found `{actual}`)")?;
                }
                Ok(())
            }
        }
    }
}
