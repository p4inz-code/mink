//! Type-error model.
//!
//! Type errors are produced by the type checker when a program violates a
//! typing rule: a value of one type is used where another is required, an
//! operator is applied to incompatible operands, a range is built from
//! invalid endpoints, a value that is not a function is called, a call
//! supplies the wrong number of arguments, or a loop iterates over a value
//! that is not a range.
//!
//! The model mirrors the lexer, parser, and semantic error designs: each
//! error carries a stable category ([`TypeErrorKind`]), the precise source
//! [`Span`](crate::source::Span) it applies to, rendered expected/actual
//! types where useful, the offending operator for operator errors, and an
//! optional related span (for example the target of a mismatched
//! assignment). Codes `E-T01` … `E-T06` continue the established ranges
//! (`E-L*` lexical, `E-P*` syntax, `E-S*` semantic); the full catalog is in
//! `docs/implementation/TYPE_SYSTEM_IMPLEMENTATION.md`.

use std::fmt;

use crate::source::Span;

/// The category of a type error.
///
/// Every category has a stable machine-readable code
/// ([`TypeErrorKind::code`]) and a human-readable message
/// ([`fmt::Display`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeErrorKind {
    /// A value of one type is used where a different type is required
    /// (assignment, call argument, return value, or boolean condition).
    TypeMismatch,
    /// A unary or binary operator is applied to incompatible operands.
    InvalidOperator,
    /// A range is constructed from incompatible endpoints.
    InvalidRange,
    /// A value that does not have a function type is called.
    NotCallable,
    /// A call supplies the wrong number of arguments.
    WrongArgumentCount,
    /// A `for` loop iterates over a value that is not a range.
    NotIterable,
}

impl TypeErrorKind {
    /// Stable machine-readable code for this error category.
    ///
    /// Codes are provisional until the full diagnostic engine defines the
    /// final error-code namespace, matching the lexer/parser/semantic
    /// convention.
    pub fn code(self) -> &'static str {
        match self {
            Self::TypeMismatch => "E-T01",
            Self::InvalidOperator => "E-T02",
            Self::InvalidRange => "E-T03",
            Self::NotCallable => "E-T04",
            Self::WrongArgumentCount => "E-T05",
            Self::NotIterable => "E-T06",
        }
    }
}

/// A single type error: a category, the span it applies to, rendered
/// expected/actual types where meaningful, the offending operator (for
/// operator errors), and an optional related span pointing at a second
/// location involved in the error (for example the target of a mismatched
/// assignment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError {
    kind: TypeErrorKind,
    span: Span,
    /// The type or count a construct required (rendered), where applicable.
    expected: Option<String>,
    /// The type(s) or count actually present (rendered), where applicable.
    actual: Option<String>,
    /// The offending operator symbol, for operator errors.
    operator: Option<String>,
    /// A related location involved in the error, where applicable.
    related: Option<Span>,
}

impl TypeError {
    /// Creates an expected/found mismatch error at `span` (`E-T01`).
    ///
    /// `expected` and `actual` are rendered type names (e.g. `Int`); `related`
    /// may point at the second location involved (e.g. the assignment
    /// target).
    pub fn mismatch(
        span: Span,
        expected: impl Into<String>,
        actual: impl Into<String>,
        related: Option<Span>,
    ) -> Self {
        Self {
            kind: TypeErrorKind::TypeMismatch,
            span,
            expected: Some(expected.into()),
            actual: Some(actual.into()),
            operator: None,
            related,
        }
    }

    /// Creates an invalid-operator error at `span` (`E-T02`).
    ///
    /// `actual` is the full operand phrase, e.g. `types `Int` and `Float``
    /// for a binary operator or `type `Bool`` for a unary one.
    pub fn invalid_operator(span: Span, operator: &str, actual: impl Into<String>) -> Self {
        Self {
            kind: TypeErrorKind::InvalidOperator,
            span,
            expected: None,
            actual: Some(actual.into()),
            operator: Some(operator.to_string()),
            related: None,
        }
    }

    /// Creates an invalid-range error at `span` (`E-T03`).
    ///
    /// `actual` renders the endpoint types, e.g. `` `Int` and `Float` ``.
    pub fn invalid_range(span: Span, actual: impl Into<String>) -> Self {
        Self {
            kind: TypeErrorKind::InvalidRange,
            span,
            expected: None,
            actual: Some(actual.into()),
            operator: None,
            related: None,
        }
    }

    /// Creates a not-callable error at `span` (`E-T04`), where `actual` is
    /// the rendered type of the called value.
    pub fn not_callable(span: Span, actual: impl Into<String>) -> Self {
        Self {
            kind: TypeErrorKind::NotCallable,
            span,
            expected: None,
            actual: Some(actual.into()),
            operator: None,
            related: None,
        }
    }

    /// Creates a wrong-argument-count error at `span` (`E-T05`).
    pub fn wrong_arg_count(span: Span, expected: usize, actual: usize) -> Self {
        Self {
            kind: TypeErrorKind::WrongArgumentCount,
            span,
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
            operator: None,
            related: None,
        }
    }

    /// Creates a not-iterable error at `span` (`E-T06`), where `actual` is
    /// the rendered type of the iterated value.
    pub fn not_iterable(span: Span, actual: impl Into<String>) -> Self {
        Self {
            kind: TypeErrorKind::NotIterable,
            span,
            expected: None,
            actual: Some(actual.into()),
            operator: None,
            related: None,
        }
    }

    /// The category of this error.
    pub fn kind(&self) -> TypeErrorKind {
        self.kind
    }

    /// The stable machine-readable code of this error (e.g. `E-T01`).
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// The source span this error applies to.
    pub fn span(&self) -> Span {
        self.span
    }

    /// The type or count a construct required (rendered), when applicable.
    pub fn expected(&self) -> Option<&str> {
        self.expected.as_deref()
    }

    /// The type(s) or count actually present (rendered), when applicable.
    pub fn actual(&self) -> Option<&str> {
        self.actual.as_deref()
    }

    /// The offending operator symbol, for operator errors.
    pub fn operator(&self) -> Option<&str> {
        self.operator.as_deref()
    }

    /// A related location involved in the error, when applicable.
    pub fn related(&self) -> Option<Span> {
        self.related
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let expected = self.expected.as_deref().unwrap_or("");
        let actual = self.actual.as_deref().unwrap_or("");
        let message = match self.kind {
            TypeErrorKind::TypeMismatch => format!("expected `{expected}`, found `{actual}`"),
            TypeErrorKind::InvalidOperator => format!(
                "cannot apply operator `{}` to {actual}",
                self.operator.as_deref().unwrap_or("")
            ),
            TypeErrorKind::InvalidRange => {
                format!("cannot construct a range with operands of types {actual}")
            }
            TypeErrorKind::NotCallable => format!("cannot call a value of type `{actual}`"),
            TypeErrorKind::WrongArgumentCount => {
                format!("expected `{expected}` arguments, found `{actual}`")
            }
            TypeErrorKind::NotIterable => format!("cannot iterate over a value of type `{actual}`"),
        };
        f.write_str(&message)
    }
}
