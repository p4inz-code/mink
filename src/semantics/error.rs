//! Semantic error model.
//!
//! Semantic errors are produced by the semantic analyzer when a syntactically
//! valid program violates a semantic rule currently supported by MINK:
//! unresolved names, duplicate declarations, invalid shadowing, assignment
//! to immutable bindings, and control-flow context violations (`break`,
//! `continue`, `return` outside their legal context).
//!
//! The model mirrors the lexer's and parser's lightweight error designs
//! ([`LexError`](crate::lexer::LexError), [`ParseError`](crate::parser::ParseError)):
//! each error carries a stable category ([`SemanticErrorKind`]), the precise
//! source [`Span`](crate::source::Span) it applies to, and — for name-related
//! diagnostics — the offending name and, for duplicate declarations, the span
//! of the original declaration. All three error kinds feed into the driver's
//! `check` report; the full structured diagnostic engine (severity,
//! explanations, machine-readable output) remains a later milestone per
//! `docs/language/ERROR_SYSTEM.md`.
//!
//! Codes `E-S01` … `E-S07` are reserved for semantic analysis, following the
//! lexical (`E-L*`) and syntax (`E-P*`) ranges. See
//! `docs/implementation/SEMANTIC_ANALYSIS_IMPLEMENTATION.md` for the full
//! catalog.

use std::fmt;

use crate::source::Span;

/// The category of a semantic error.
///
/// Every category has a stable machine-readable code
/// ([`SemanticErrorKind::code`]) and a human-readable message
/// ([`fmt::Display`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticErrorKind {
    /// A name reference that resolves to no declaration in any visible scope.
    UnresolvedName,
    /// Two declarations of the same name in the same scope.
    DuplicateDefinition,
    /// An assignment to a binding that is not mutable (`let`, parameter,
    /// `for` variable, or function name).
    AssignmentToImmutable,
    /// An assignment to a `const` binding.
    AssignmentToConstant,
    /// `break;` appearing outside any loop body.
    BreakOutsideLoop,
    /// `continue;` appearing outside any loop body.
    ContinueOutsideLoop,
    /// `return;` appearing outside a function body.
    ReturnOutsideFunction,
    /// Two `struct` declarations of the same name at module scope.
    DuplicateStruct,
    /// A struct declares the same field name twice.
    DuplicateField,
    /// A use of a value after its ownership was moved away (ownership
    /// analysis, session 15).
    UseOfMovedValue,
    /// An attempt to mutate an immutable (literal) string via
    /// `rt_str_set_byte` (ownership analysis, session 15).
    MutatingImmutableString,
    /// A borrow conflict (session 16): borrowing a value that is already
    /// borrowed incompatibly, or using/mutating/consuming a value while it
    /// is borrowed.
    BorrowConflict,
    /// An invalid borrow (session 16): `&mut` of a binding that is not
    /// mutable, or borrowing a constant.
    InvalidBorrow,
    /// A dangling reference (session 16): returning a reference (or an
    /// aggregate containing one) to a function-local value, so it would
    /// outlive its source.
    DanglingReference,
}

impl SemanticErrorKind {
    /// Stable machine-readable code for this error category.
    ///
    /// Codes are provisional until the full diagnostic engine defines the
    /// final error-code namespace, matching the lexer/parser convention.
    pub fn code(self) -> &'static str {
        match self {
            Self::UnresolvedName => "E-S01",
            Self::DuplicateDefinition => "E-S02",
            Self::AssignmentToImmutable => "E-S03",
            Self::AssignmentToConstant => "E-S04",
            Self::BreakOutsideLoop => "E-S05",
            Self::ContinueOutsideLoop => "E-S06",
            Self::ReturnOutsideFunction => "E-S07",
            Self::DuplicateStruct => "E-S08",
            Self::DuplicateField => "E-S09",
            Self::UseOfMovedValue => "E-S10",
            Self::MutatingImmutableString => "E-S11",
            Self::BorrowConflict => "E-S12",
            Self::InvalidBorrow => "E-S13",
            Self::DanglingReference => "E-S14",
        }
    }
}

/// A single semantic error: a category, the span it applies to, the
/// offending name (for name-related categories), and the span of the
/// original declaration (for duplicate definitions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticError {
    kind: SemanticErrorKind,
    span: Span,
    /// The offending identifier spelling, or empty for categories that do
    /// not name a specific identifier (`break`/`continue`/`return` context).
    name: String,
    /// The span of the original declaration a duplicate collides with.
    original: Option<Span>,
    /// A category-specific detail line rendered instead of the default
    /// message when present (ownership diagnostics use it to carry the
    /// full explanation, e.g. which field of a struct was moved).
    detail: Option<String>,
}

impl SemanticError {
    /// Creates an unresolved-name error for `name` at `span`.
    pub fn unresolved(name: impl Into<String>, span: Span) -> Self {
        Self {
            kind: SemanticErrorKind::UnresolvedName,
            span,
            name: name.into(),
            original: None,
            detail: None,
        }
    }

    /// Creates a duplicate-definition error for `name` at `span`, whose
    /// original declaration is at `original`.
    pub fn duplicate(name: impl Into<String>, span: Span, original: Span) -> Self {
        Self {
            kind: SemanticErrorKind::DuplicateDefinition,
            span,
            name: name.into(),
            original: Some(original),
            detail: None,
        }
    }

    /// Creates an assignment-to-immutable error for `name` at `span`.
    pub fn immutable_assignment(name: impl Into<String>, span: Span) -> Self {
        Self {
            kind: SemanticErrorKind::AssignmentToImmutable,
            span,
            name: name.into(),
            original: None,
            detail: None,
        }
    }

    /// Creates an assignment-to-constant error for `name` at `span`.
    pub fn const_assignment(name: impl Into<String>, span: Span) -> Self {
        Self {
            kind: SemanticErrorKind::AssignmentToConstant,
            span,
            name: name.into(),
            original: None,
            detail: None,
        }
    }

    /// Creates a `break`-outside-a-loop error at `span`.
    pub fn break_outside_loop(span: Span) -> Self {
        Self {
            kind: SemanticErrorKind::BreakOutsideLoop,
            span,
            name: String::new(),
            original: None,
            detail: None,
        }
    }

    /// Creates a `continue`-outside-a-loop error at `span`.
    pub fn continue_outside_loop(span: Span) -> Self {
        Self {
            kind: SemanticErrorKind::ContinueOutsideLoop,
            span,
            name: String::new(),
            original: None,
            detail: None,
        }
    }

    /// Creates a `return`-outside-a-function error at `span`.
    pub fn return_outside_function(span: Span) -> Self {
        Self {
            kind: SemanticErrorKind::ReturnOutsideFunction,
            span,
            name: String::new(),
            original: None,
            detail: None,
        }
    }

    /// Creates a duplicate-struct error for `name` at `span`, whose original
    /// declaration is at `original`.
    pub fn duplicate_struct(name: impl Into<String>, span: Span, original: Span) -> Self {
        Self {
            kind: SemanticErrorKind::DuplicateStruct,
            span,
            name: name.into(),
            original: Some(original),
            detail: None,
        }
    }

    /// Creates a duplicate-field error for `name` at `span`, whose original
    /// declaration is at `original`.
    pub fn duplicate_field(name: impl Into<String>, span: Span, original: Span) -> Self {
        Self {
            kind: SemanticErrorKind::DuplicateField,
            span,
            name: name.into(),
            original: Some(original),
            detail: None,
        }
    }

    /// Creates a use-of-moved-value error for `name` at `span` (E-S10).
    pub fn use_of_moved(name: impl Into<String>, span: Span) -> Self {
        Self {
            kind: SemanticErrorKind::UseOfMovedValue,
            span,
            name: name.into(),
            original: None,
            detail: None,
        }
    }

    /// Creates a use-of-moved-value error for `name` at `span` (E-S10)
    /// whose message is `detail` instead of the default.
    pub fn use_of_moved_detail(name: impl Into<String>, span: Span, detail: String) -> Self {
        Self {
            kind: SemanticErrorKind::UseOfMovedValue,
            span,
            name: name.into(),
            original: None,
            detail: Some(detail),
        }
    }

    /// Creates a mutating-immutable-string error for `name` at `span`
    /// (E-S11).
    pub fn mutating_immutable_string(name: impl Into<String>, span: Span) -> Self {
        Self {
            kind: SemanticErrorKind::MutatingImmutableString,
            span,
            name: name.into(),
            original: None,
            detail: None,
        }
    }

    /// Creates a mutating-immutable-string error at `span` (E-S11) whose
    /// message is `detail` instead of the default (used when the target is
    /// not a plain identifier).
    pub fn mutating_immutable_string_detail(span: Span, detail: String) -> Self {
        Self {
            kind: SemanticErrorKind::MutatingImmutableString,
            span,
            name: String::new(),
            original: None,
            detail: Some(detail),
        }
    }

    /// Creates a borrow-conflict error for `name` at `span` (E-S12) whose
    /// message is `detail` instead of the default.
    pub fn borrow_conflict(name: impl Into<String>, span: Span, detail: String) -> Self {
        Self {
            kind: SemanticErrorKind::BorrowConflict,
            span,
            name: name.into(),
            original: None,
            detail: Some(detail),
        }
    }

    /// Creates a borrow-conflict error at `span` (E-S12) whose message is
    /// `detail` (used when there is no single offending name).
    pub fn borrow_conflict_detail(span: Span, detail: String) -> Self {
        Self {
            kind: SemanticErrorKind::BorrowConflict,
            span,
            name: String::new(),
            original: None,
            detail: Some(detail),
        }
    }

    /// Creates an invalid-borrow error at `span` (E-S13) whose message is
    /// `detail`.
    pub fn invalid_borrow(span: Span, detail: String) -> Self {
        Self {
            kind: SemanticErrorKind::InvalidBorrow,
            span,
            name: String::new(),
            original: None,
            detail: Some(detail),
        }
    }

    /// Creates a dangling-reference error at `span` (E-S14) whose message
    /// is `detail`.
    pub fn dangling_reference(span: Span, detail: String) -> Self {
        Self {
            kind: SemanticErrorKind::DanglingReference,
            span,
            name: String::new(),
            original: None,
            detail: Some(detail),
        }
    }

    /// The category of this error.
    pub fn kind(&self) -> SemanticErrorKind {
        self.kind
    }

    /// The stable machine-readable code of this error (e.g. `E-S01`).
    ///
    /// Mirrors [`CheckError::code`](crate::driver::CheckError::code) for
    /// direct use by tooling and tests.
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// The source span this error applies to.
    pub fn span(&self) -> Span {
        self.span
    }

    /// The offending name, for name-related categories (empty otherwise).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The span of the original declaration, for duplicate definitions.
    pub fn original(&self) -> Option<Span> {
        self.original
    }
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.kind {
            SemanticErrorKind::UnresolvedName => {
                format!("cannot find name `{}` in this scope", self.name)
            }
            SemanticErrorKind::DuplicateDefinition => {
                format!("duplicate definition of `{}`", self.name)
            }
            SemanticErrorKind::AssignmentToImmutable => {
                format!("cannot assign to `{}`: it is not mutable", self.name)
            }
            SemanticErrorKind::AssignmentToConstant => {
                format!("cannot assign to `{}`: it is a constant", self.name)
            }
            SemanticErrorKind::BreakOutsideLoop => "`break` outside of a loop".to_string(),
            SemanticErrorKind::ContinueOutsideLoop => "`continue` outside of a loop".to_string(),
            SemanticErrorKind::ReturnOutsideFunction => {
                "`return` outside of a function".to_string()
            }
            SemanticErrorKind::DuplicateStruct => {
                format!("duplicate definition of struct `{}`", self.name)
            }
            SemanticErrorKind::DuplicateField => {
                format!("duplicate field `{}` in struct declaration", self.name)
            }
            SemanticErrorKind::UseOfMovedValue => self
                .detail
                .clone()
                .unwrap_or_else(|| format!("use of moved value `{}`", self.name)),
            SemanticErrorKind::MutatingImmutableString => {
                self.detail.clone().unwrap_or_else(|| {
                    format!("cannot mutate `{}`: it is an immutable string", self.name)
                })
            }
            SemanticErrorKind::BorrowConflict => self
                .detail
                .clone()
                .unwrap_or_else(|| format!("borrow conflict involving `{}`", self.name)),
            SemanticErrorKind::InvalidBorrow => self
                .detail
                .clone()
                .unwrap_or_else(|| "invalid borrow".to_string()),
            SemanticErrorKind::DanglingReference => self
                .detail
                .clone()
                .unwrap_or_else(|| "dangling reference".to_string()),
        };
        f.write_str(&message)
    }
}
