//! Backend error model.
//!
//! The backend lowers the optimized [`MirProgram`](super::MirProgram) into a
//! target-independent instruction representation ([`lower`](super::lower)),
//! verifies it ([`verify`](super::verify)), and emits machine code for a
//! selected [`Target`](super::Target) ([`emit`](super::emit)). For the
//! current native subset, lowering intentionally supports only the scalar
//! core of the language — integers, booleans, integer ranges, functions,
//! calls, and structured control flow — and reports everything outside that
//! subset as structured errors instead of emitting incorrect output.
//!
//! Every error carries a stable machine-readable code (`E-B01`…), a
//! human-readable message, and the exact source span of the construct it
//! rejects. Codes `E-B01` … continue the established stable ranges
//! (`E-L*` lexical, `E-P*` syntax, `E-S*` semantic, `E-T*` type, `E-H*`
//! HIR, `E-M*` MIR); the full catalog is in
//! `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`.

use std::fmt;

use crate::source::Span;

/// The category of a backend error.
///
/// Every category has a stable machine-readable code
/// ([`BackendErrorKind::code`]) and a human-readable message
/// ([`fmt::Display`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendErrorKind {
    /// A MIR rvalue the backend cannot lower yet: member loads, index
    /// loads, and function-valued operands. The constructs are valid MINK
    /// that the front end accepts; the current native subset rejects them
    /// cleanly instead of guessing their semantics.
    UnsupportedRvalue,
    /// A literal constant the backend cannot decode yet: floating-point,
    /// string, character, and `null` literals. Only integer and boolean
    /// literals are supported by the first native subset.
    UnsupportedConstant,
    /// A type the backend cannot represent: `Float`, `Str`, `Char`, `Null`,
    /// an unresolved inference type, or `Range` in a position that requires
    /// a single machine word (a function result, a module binding, or an
    /// operand). `Range` values themselves are supported as locals and call
    /// arguments.
    UnsupportedType,
    /// A MIR assignment target the backend cannot lower yet: member and
    /// index places.
    UnsupportedPlace,
    /// A module-level `let`/`const` binding whose initializer requires
    /// runtime evaluation (non-constant statements, or a final value that
    /// references another module item). The first native subset supports
    /// statics initialized by a constant only.
    UnsupportedStatic,
    /// A call whose callee is not a module-level function. The first native
    /// subset supports direct calls only.
    UnsupportedCallee,
    /// The backend instruction representation is internally inconsistent
    /// (dangling local or block references, unordered blocks, an invalid
    /// entry). Lowering always produces valid instructions; this defends
    /// the pipeline against malformed hand-built or mutated programs.
    InvalidBackendIr,
    /// The program has no `main` function to serve as the executable's
    /// entry point. `mink build` requires a module-level `fn main()`.
    NoEntryPoint,
    /// The `main` function cannot serve as the entry point: it has
    /// parameters, or its result type is unsupported for an exit code.
    InvalidEntryPoint,
    /// A literal's source text could not be decoded (for example the source
    /// map has no text for the literal's span). Only reachable on malformed
    /// hand-built MIR or a missing source map; a clean pipeline never hits
    /// it.
    DecodeError,
    /// The selected [`Target`](super::Target) is recognized but has no
    /// implementation yet. The first native target is
    /// `x86_64-windows-pe`.
    UnsupportedTarget,
    /// The target name is not a recognized target.
    InvalidTarget,
}

impl BackendErrorKind {
    /// Stable machine-readable code for this error category.
    pub fn code(self) -> &'static str {
        match self {
            Self::UnsupportedRvalue => "E-B01",
            Self::UnsupportedConstant => "E-B02",
            Self::UnsupportedType => "E-B03",
            Self::UnsupportedPlace => "E-B04",
            Self::UnsupportedStatic => "E-B05",
            Self::UnsupportedCallee => "E-B06",
            Self::InvalidBackendIr => "E-B07",
            Self::NoEntryPoint => "E-B08",
            Self::InvalidEntryPoint => "E-B09",
            Self::DecodeError => "E-B10",
            Self::UnsupportedTarget => "E-B11",
            Self::InvalidTarget => "E-B12",
        }
    }
}

/// A single backend error: a category, the span it applies to, and a
/// rendered detail where useful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendError {
    kind: BackendErrorKind,
    span: Span,
    /// Rendered detail (for example the offending local or block id), where
    /// applicable.
    detail: Option<String>,
}

impl BackendError {
    /// Creates a new backend error of `kind` at `span` with an optional
    /// rendered `detail`.
    pub fn new(kind: BackendErrorKind, span: Span, detail: Option<String>) -> Self {
        Self { kind, span, detail }
    }

    /// Creates an unsupported-rvalue error at `span` (`E-B01`).
    pub fn unsupported_rvalue(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::UnsupportedRvalue,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an unsupported-constant error at `span` (`E-B02`).
    pub fn unsupported_constant(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::UnsupportedConstant,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an unsupported-type error at `span` (`E-B03`).
    pub fn unsupported_type(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::UnsupportedType,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an unsupported-assignment-target error at `span` (`E-B04`).
    pub fn unsupported_assign_target(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::UnsupportedPlace,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an unsupported-static error at `span` (`E-B05`).
    pub fn unsupported_static(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::UnsupportedStatic,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an unsupported-callee error at `span` (`E-B06`).
    pub fn unsupported_callee(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::UnsupportedCallee,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an invalid-backend-IR error at `span` (`E-B07`).
    pub fn invalid_backend_ir(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::InvalidBackendIr,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates a no-entry-point error (`E-B08`) at `span`.
    pub fn no_entry_point(span: Span) -> Self {
        Self {
            kind: BackendErrorKind::NoEntryPoint,
            span,
            detail: None,
        }
    }

    /// Creates an invalid-entry-point error at `span` (`E-B09`).
    pub fn invalid_entry_point(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::InvalidEntryPoint,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates a literal-decode error at `span` (`E-B10`).
    pub fn decode_error(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::DecodeError,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an unsupported-target error at `span` (`E-B11`).
    pub fn unsupported_target(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::UnsupportedTarget,
            span,
            detail: Some(detail.into()),
        }
    }

    /// Creates an invalid-target error at `span` (`E-B12`).
    pub fn invalid_target(span: Span, detail: impl Into<String>) -> Self {
        Self {
            kind: BackendErrorKind::InvalidTarget,
            span,
            detail: Some(detail.into()),
        }
    }

    /// The category of this error.
    pub fn kind(&self) -> BackendErrorKind {
        self.kind
    }

    /// The stable machine-readable code of this error (e.g. `E-B01`).
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

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            BackendErrorKind::UnsupportedRvalue => {
                f.write_str("backend: cannot lower this expression")?;
            }
            BackendErrorKind::UnsupportedConstant => {
                f.write_str("backend: this literal is not supported by the native subset")?;
            }
            BackendErrorKind::UnsupportedType => {
                f.write_str("backend: this type is not supported by the native subset")?;
            }
            BackendErrorKind::UnsupportedPlace => {
                f.write_str("backend: cannot lower this assignment target")?;
            }
            BackendErrorKind::UnsupportedStatic => {
                f.write_str("backend: this module binding needs runtime initialization")?;
            }
            BackendErrorKind::UnsupportedCallee => {
                f.write_str("backend: can only call module-level functions")?;
            }
            BackendErrorKind::InvalidBackendIr => {
                f.write_str("invalid backend IR")?;
            }
            BackendErrorKind::NoEntryPoint => {
                f.write_str(
                    "backend: the program has no `main` function to use as the entry point",
                )?;
            }
            BackendErrorKind::InvalidEntryPoint => {
                f.write_str("backend: `main` cannot be used as the entry point")?;
            }
            BackendErrorKind::DecodeError => {
                f.write_str("backend: could not decode a literal")?;
            }
            BackendErrorKind::UnsupportedTarget => {
                f.write_str("backend: this target is not implemented yet")?;
            }
            BackendErrorKind::InvalidTarget => {
                f.write_str("backend: unknown target")?;
            }
        }
        if let Some(detail) = &self.detail {
            write!(f, " ({detail})")?;
        }
        Ok(())
    }
}
