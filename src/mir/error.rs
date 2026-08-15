//! MIR lowering- and validation-error model.
//!
//! MIR lowering consumes the [`HirProgram`](super::MirProgram) and produces
//! a control-flow representation (see [`lower`](super::lower)); structural
//! validation checks the result (see [`validate`](super::validate)). For a
//! valid front end (a clean pipeline through HIR) lowering always succeeds;
//! lowering errors represent internal inconsistencies that should never
//! occur in a well-formed pipeline (`break` outside a loop, a `for` over a
//! non-range, an identifier with no corresponding local, an invalid
//! assignment target, a block left without a terminator). Validation errors
//! are likewise defensive: they report malformed hand-built or tooling-built
//! MIR instead of panicking. Both classes are reported structurally, in
//! deterministic order, with stable machine-readable codes.
//!
//! Codes `E-M01` … `E-M12` continue the established stable ranges (`E-L*`
//! lexical, `E-P*` syntax, `E-S*` semantic, `E-T*` type, `E-H*` HIR); the
//! full catalog is in `docs/implementation/MIR_IMPLEMENTATION.md`.

use std::fmt;

use crate::source::Span;

/// The category of a MIR error.
///
/// Every category has a stable machine-readable code
/// ([`MirErrorKind::code`]) and a human-readable message
/// ([`fmt::Display`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MirErrorKind {
    /// A `break` statement has no enclosing loop to target. Semantic
    /// analysis rejects this, so the error path is defensive.
    BreakOutsideLoop,
    /// A `continue` statement has no enclosing loop to target. Semantic
    /// analysis rejects this, so the error path is defensive.
    ContinueOutsideLoop,
    /// A `for` loop iterates over a value whose type is not a range. The
    /// type checker rejects this, so the error path is defensive.
    NonRangeForIterable,
    /// An identifier reference has no corresponding local and does not
    /// reference a module-level item.
    UnresolvedLocal,
    /// An assignment target is not a place expression.
    InvalidAssignmentTarget,
    /// A borrow target is not a local-rooted place (session 16): the
    /// checker rejects non-place borrows, so the error path is defensive
    /// (module-storage roots have no stack address).
    InvalidBorrowTarget,
    /// A block was left without a terminator. Blocks are built with exactly
    /// one terminator by construction, so this only occurs through an
    /// internal builder error.
    MissingTerminator,
    /// A terminator references a block that does not exist.
    InvalidBlockReference,
    /// A statement or operand references a local that does not exist.
    InvalidLocalReference,
    /// A node references a type that is not present in the type table.
    InvalidTypeReference,
    /// Blocks are not ordered by id: the block at index `i` has a different
    /// id. Deterministic block ordering requires `block.id == index`.
    BlockIdMismatch,
    /// Parameter locals are not the first locals, in declaration order.
    ParamLocalMismatch,
}

impl MirErrorKind {
    /// Stable machine-readable code for this error category.
    pub fn code(self) -> &'static str {
        match self {
            Self::BreakOutsideLoop => "E-M01",
            Self::ContinueOutsideLoop => "E-M02",
            Self::NonRangeForIterable => "E-M03",
            Self::UnresolvedLocal => "E-M04",
            Self::InvalidAssignmentTarget => "E-M05",
            Self::MissingTerminator => "E-M06",
            Self::InvalidBlockReference => "E-M07",
            Self::InvalidLocalReference => "E-M08",
            Self::InvalidTypeReference => "E-M09",
            Self::BlockIdMismatch => "E-M10",
            Self::ParamLocalMismatch => "E-M11",
            Self::InvalidBorrowTarget => "E-M12",
        }
    }
}

/// A single MIR error: a category, the span it applies to, and a rendered
/// detail where useful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MirError {
    kind: MirErrorKind,
    span: Span,
    /// Rendered detail (for example the offending block or local id), where
    /// applicable.
    detail: Option<String>,
}

impl MirError {
    /// Creates a `break`-outside-loop error at `span` (`E-M01`).
    pub fn break_outside_loop(span: Span) -> Self {
        Self {
            kind: MirErrorKind::BreakOutsideLoop,
            span,
            detail: None,
        }
    }

    /// Creates a `continue`-outside-loop error at `span` (`E-M02`).
    pub fn continue_outside_loop(span: Span) -> Self {
        Self {
            kind: MirErrorKind::ContinueOutsideLoop,
            span,
            detail: None,
        }
    }

    /// Creates a non-range-iterable error at `span` (`E-M03`).
    pub fn non_range_for_iterable(span: Span) -> Self {
        Self {
            kind: MirErrorKind::NonRangeForIterable,
            span,
            detail: None,
        }
    }

    /// Creates an unresolved-local error at `span` (`E-M04`).
    pub fn unresolved_local(span: Span) -> Self {
        Self {
            kind: MirErrorKind::UnresolvedLocal,
            span,
            detail: None,
        }
    }

    /// Creates an invalid-assignment-target error at `span` (`E-M05`).
    pub fn invalid_assignment_target(span: Span) -> Self {
        Self {
            kind: MirErrorKind::InvalidAssignmentTarget,
            span,
            detail: None,
        }
    }

    /// Creates an invalid-borrow-target error at `span` (`E-M12`): the
    /// borrowed place is not rooted at a local (module storage has no
    /// stack address to borrow).
    pub fn invalid_borrow_target(span: Span) -> Self {
        Self {
            kind: MirErrorKind::InvalidBorrowTarget,
            span,
            detail: None,
        }
    }

    /// Creates a missing-terminator error at `span` (`E-M06`).
    pub fn missing_terminator(span: Span) -> Self {
        Self {
            kind: MirErrorKind::MissingTerminator,
            span,
            detail: None,
        }
    }

    /// Creates an invalid-block-reference error at `span` (`E-M07`).
    pub fn invalid_block_reference(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: MirErrorKind::InvalidBlockReference,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an invalid-local-reference error at `span` (`E-M08`).
    pub fn invalid_local_reference(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: MirErrorKind::InvalidLocalReference,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an invalid-type-reference error at `span` (`E-M09`).
    pub fn invalid_type_reference(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: MirErrorKind::InvalidTypeReference,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates a block-id-mismatch error at `span` (`E-M10`).
    pub fn block_id_mismatch(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: MirErrorKind::BlockIdMismatch,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates a parameter-local-mismatch error at `span` (`E-M11`).
    pub fn param_local_mismatch(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: MirErrorKind::ParamLocalMismatch,
            span,
            detail: Some(detail.into()),
        }
    }

    /// The category of this error.
    pub fn kind(&self) -> MirErrorKind {
        self.kind
    }

    /// The stable machine-readable code of this error (e.g. `E-M01`).
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

impl fmt::Display for MirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = self.detail.as_deref().unwrap_or("");
        match self.kind {
            MirErrorKind::BreakOutsideLoop => f.write_str("cannot lower `break` outside of a loop"),
            MirErrorKind::ContinueOutsideLoop => {
                f.write_str("cannot lower `continue` outside of a loop")
            }
            MirErrorKind::NonRangeForIterable => {
                f.write_str("cannot lower `for` loop: iterable is not a range")
            }
            MirErrorKind::UnresolvedLocal => {
                f.write_str("cannot lower: identifier has no corresponding local")
            }
            MirErrorKind::InvalidAssignmentTarget => {
                f.write_str("cannot lower: invalid assignment target")
            }
            MirErrorKind::InvalidBorrowTarget => {
                f.write_str("cannot lower: borrow target is not a local-rooted place")
            }
            MirErrorKind::MissingTerminator => {
                f.write_str("cannot lower: block is missing a terminator")
            }
            MirErrorKind::InvalidBlockReference => {
                write!(f, "invalid MIR: terminator references an unknown block")?;
                self.write_detail(f, detail)
            }
            MirErrorKind::InvalidLocalReference => {
                write!(f, "invalid MIR: references an unknown local")?;
                self.write_detail(f, detail)
            }
            MirErrorKind::InvalidTypeReference => {
                write!(f, "invalid MIR: references an unknown type")?;
                self.write_detail(f, detail)
            }
            MirErrorKind::BlockIdMismatch => {
                write!(f, "invalid MIR: block ids are not ordered")?;
                self.write_detail(f, detail)
            }
            MirErrorKind::ParamLocalMismatch => {
                write!(f, "invalid MIR: parameter locals are not the first locals")?;
                self.write_detail(f, detail)
            }
        }
    }
}

impl MirError {
    /// Appends ` (detail)` when a detail string is present.
    fn write_detail(&self, f: &mut fmt::Formatter<'_>, detail: &str) -> fmt::Result {
        if !detail.is_empty() {
            write!(f, " ({detail})")?;
        }
        Ok(())
    }
}
