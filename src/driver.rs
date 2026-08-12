//! Compiler pipeline orchestration.
//!
//! Owns the sequence Source → Lexer → Parser → AST → Semantic Analysis →
//! Type Analysis → HIR → MIR → Backend (see
//! `docs/compiler/COMPILER_ARCHITECTURE.md` §2). The driver runs source
//! loading plus lexical, syntactic, semantic, type, and HIR analysis, and
//! lowers to MIR when the front end is clean: the parser consumes the token
//! stream and produces the AST, and when the source is lexically and
//! syntactically valid, the semantic analyzer validates it and the type
//! checker types it. When semantic and type analysis report no errors, HIR
//! lowering runs, and when HIR lowering succeeds, MIR lowering and
//! validation run. Errors are reported together across stages. The backend
//! is not yet implemented.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::hir::{self, HirError, HirProgram};
use crate::lexer::LexError;
use crate::mir::{self, MirError, MirProgram};
use crate::parser::{self, ParseError};
use crate::semantics::{self, SemanticError, SemanticResult};
use crate::source::{SourceId, SourceMap, Span};
use crate::typecheck::{self, TypeError, TypeResult};

/// Errors produced while running the build pipeline.
#[derive(Debug)]
pub enum BuildError {
    /// The source file could not be read from disk.
    Io {
        /// The path that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// The pipeline has not been implemented past the syntax-analysis stage.
    NotImplemented,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::NotImplemented => {
                write!(f, "the build pipeline is not yet implemented")
            }
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::NotImplemented => None,
        }
    }
}

/// A single problem found by `check`: a lexical, a syntax, a semantic, a
/// type, a HIR, or a MIR error.
///
/// All kinds carry a stable code, a human-readable message, and the exact
/// source span they apply to, so the CLI (and future diagnostic engine) can
/// render them uniformly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckError {
    /// A lexical error produced by the lexer.
    Lex(LexError),
    /// A syntax error produced by the parser.
    Parse(ParseError),
    /// A semantic error produced by the semantic analyzer.
    Semantic(SemanticError),
    /// A type error produced by the type checker.
    Type(TypeError),
    /// A HIR lowering error produced by the HIR layer.
    Hir(HirError),
    /// A MIR lowering or validation error produced by the MIR layer.
    Mir(MirError),
}

impl CheckError {
    /// The stable machine-readable code of this error (e.g. `E-L01`,
    /// `E-P03`, `E-S01`, `E-T01`, `E-H01`, `E-M01`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Lex(error) => error.kind().code(),
            Self::Parse(error) => error.kind().code(),
            Self::Semantic(error) => error.kind().code(),
            Self::Type(error) => error.kind().code(),
            Self::Hir(error) => error.kind().code(),
            Self::Mir(error) => error.kind().code(),
        }
    }

    /// The source span this error applies to.
    pub fn span(&self) -> Span {
        match self {
            Self::Lex(error) => error.span(),
            Self::Parse(error) => error.span(),
            Self::Semantic(error) => error.span(),
            Self::Type(error) => error.span(),
            Self::Hir(error) => error.span(),
            Self::Mir(error) => error.span(),
        }
    }

    /// A related source span, when this error references another location
    /// (for example the original declaration of a duplicate definition, or
    /// the target of a mismatched assignment).
    pub fn related_span(&self) -> Option<Span> {
        match self {
            Self::Semantic(error) => error.original(),
            Self::Type(error) => error.related(),
            Self::Lex(_) | Self::Parse(_) | Self::Hir(_) | Self::Mir(_) => None,
        }
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => error.fmt(f),
            Self::Parse(error) => error.fmt(f),
            Self::Semantic(error) => error.fmt(f),
            Self::Type(error) => error.fmt(f),
            Self::Hir(error) => error.fmt(f),
            Self::Mir(error) => error.fmt(f),
        }
    }
}

/// The result of running lexical, syntactic, semantic, type, HIR, and
/// (where applicable) MIR analysis on one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    /// The id of the checked source file.
    pub source_id: SourceId,
    /// Number of tokens produced, excluding the final `Eof` token.
    pub token_count: usize,
    /// Lexical, syntax, semantic, type, HIR, and MIR errors, in source
    /// order. Empty for valid input.
    pub errors: Vec<CheckError>,
    /// The semantic-analysis result, present when the source was lexically
    /// and syntactically valid and analysis therefore ran. `None` when
    /// lexical or syntax errors made analysis unsafe or meaningless.
    pub semantic: Option<SemanticResult>,
    /// The type-analysis result, present whenever semantic analysis ran.
    /// `None` when lexical or syntax errors suppressed analysis.
    pub types: Option<TypeResult>,
    /// The lowered HIR, present when the front end (semantic and type
    /// analysis) reported no errors and lowering succeeded. `None`
    /// otherwise.
    pub hir: Option<HirProgram>,
    /// The lowered and structurally validated MIR, present when HIR
    /// lowering succeeded and MIR lowering plus validation reported no
    /// errors. `None` otherwise.
    pub mir: Option<MirProgram>,
}

/// Loads `path` and runs lexical, syntactic, semantic, type, HIR, and MIR
/// analysis over it.
///
/// On success returns a [`CheckReport`] describing the token stream, any
/// errors across all stages, and — when the source is lexically and
/// syntactically valid — the [`SemanticResult`], [`TypeResult`], lowered
/// [`HirProgram`], and (for a clean pipeline) lowered and validated
/// [`MirProgram`] of analyzing the parsed AST. The caller decides how to
/// surface them. An I/O failure to read the file is reported as
/// [`BuildError::Io`].
///
/// Semantic analysis runs only when parsing produced a usable AST (no
/// lexical or syntax errors); otherwise the existing error behavior is
/// preserved and no cascading semantic diagnostics are generated. Type
/// analysis runs whenever semantic analysis ran: the type checker consumes
/// the semantic result directly and its unknown/error type keeps semantic
/// errors from cascading into misleading type diagnostics. HIR lowering
/// runs only when semantic and type analysis reported no errors — lowering
/// an inconsistent front end would only add misleading diagnostics; a
/// lowering failure on a clean front end is an internal compiler error and
/// is reported as such (`E-H01`…`E-H03`). MIR lowering runs only when HIR
/// lowering succeeded, and the lowered MIR is structurally validated before
/// it is reported; a failure on clean HIR is an internal compiler error and
/// is reported as such (`E-M01`…`E-M11`).
pub fn check(sources: &mut SourceMap, path: &Path) -> Result<CheckReport, BuildError> {
    let source_id = sources.load(path).map_err(|source| BuildError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let file = sources
        .get(source_id)
        .expect("the file id returned by load is always registered");
    let parsed = parser::parse(file);
    let mut errors: Vec<CheckError> = parsed
        .lex_errors()
        .iter()
        .copied()
        .map(CheckError::Lex)
        .collect();
    errors.extend(parsed.parse_errors().iter().copied().map(CheckError::Parse));
    // Semantic and type analysis only when the source is lexically and
    // syntactically valid; a broken token stream or tree makes further
    // analysis unsafe or meaningless, and skipping it avoids cascades.
    let (semantic, types, hir, mir) = if parsed.is_valid() {
        let semantic = semantics::analyze(parsed.ast());
        let types = typecheck::check(parsed.ast(), &semantic);
        errors.extend(semantic.errors().iter().cloned().map(CheckError::Semantic));
        errors.extend(types.errors().iter().cloned().map(CheckError::Type));
        let (hir, mir) = if errors.is_empty() {
            match hir::lower(parsed.ast(), &semantic, &types) {
                Ok(program) => {
                    let mir = match mir::lower(&program) {
                        Ok(mir_program) => match mir::validate(&mir_program) {
                            Ok(()) => Some(mir_program),
                            Err(validation_errors) => {
                                errors.extend(validation_errors.into_iter().map(CheckError::Mir));
                                None
                            }
                        },
                        Err(lowering_errors) => {
                            errors.extend(lowering_errors.into_iter().map(CheckError::Mir));
                            None
                        }
                    };
                    (Some(program), mir)
                }
                Err(lowering_errors) => {
                    errors.extend(lowering_errors.into_iter().map(CheckError::Hir));
                    (None, None)
                }
            }
        } else {
            (None, None)
        };
        (Some(semantic), Some(types), hir, mir)
    } else {
        (None, None, None, None)
    };
    // Report problems in source order regardless of which stage produced
    // them (a stable sort keeps equal-position errors in stage order).
    errors.sort_by_key(|error| error.span().start());
    Ok(CheckReport {
        source_id,
        token_count: parsed.token_count(),
        errors,
        semantic,
        types,
        hir,
        mir,
    })
}

/// Runs the compiler pipeline for a single MINK source file.
///
/// Returns the id of the source file registered in `sources`. Current status:
/// the driver registers the file and then reports
/// [`BuildError::NotImplemented`] because semantic analysis, type checking,
/// and code generation are not implemented yet. Use [`check`] to run the
/// front end (lexing + parsing) that is implemented.
pub fn build(sources: &mut SourceMap, path: &Path) -> Result<SourceId, BuildError> {
    let _id = sources.load(path).map_err(|source| BuildError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Err(BuildError::NotImplemented)
}
