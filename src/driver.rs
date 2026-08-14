//! Compiler pipeline orchestration.
//!
//! Owns the sequence Source → Lexer → Parser → AST → Semantic Analysis →
//! Type Analysis → HIR → MIR → Optimization → Backend (see
//! `docs/compiler/COMPILER_ARCHITECTURE.md` §2). The driver runs source
//! loading plus lexical, syntactic, semantic, type, and HIR analysis, and
//! lowers to MIR when the front end is clean: the parser consumes the token
//! stream and produces the AST, and when the source is lexically and
//! syntactically valid, the semantic analyzer validates it and the type
//! checker types it. When semantic and type analysis report no errors, HIR
//! lowering runs, and when HIR lowering succeeds, MIR lowering, MIR
//! validation, and MIR optimization run. [`check`] reports the result;
//! [`build`] additionally compiles the optimized MIR into a native
//! executable image for the requested [`Target`] and writes it to disk
//! (see `docs/implementation/NATIVE_BACKEND_IMPLEMENTATION.md`). Errors
//! are reported together across stages.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::backend::{self, BackendError, Target};
use crate::hir::{self, HirError, HirProgram};
use crate::lexer::LexError;
use crate::mir::{self, MirError, MirProgram};
use crate::parser::{self, ParseError};
use crate::semantics::{self, SemanticError, SemanticResult};
use crate::source::{SourceId, SourceMap, Span};
use crate::typecheck::{self, TypeError, TypeResult};

/// Options controlling a [`build`] run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildOptions {
    /// The target to compile for.
    pub target: Target,
}

impl Default for BuildOptions {
    /// The host's native target (the first implemented target,
    /// `x86_64-windows-pe`).
    fn default() -> Self {
        Self {
            target: Target::native(),
        }
    }
}

/// The outcome of a successful [`build`]: where the executable was written
/// and what it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildOutcome {
    /// The id of the built source file.
    pub source_id: SourceId,
    /// The path of the written executable.
    pub output: PathBuf,
    /// The target the executable was compiled for.
    pub target: Target,
    /// The number of compiled functions.
    pub functions: usize,
    /// The number of compiled module bindings.
    pub statics: usize,
}

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
    /// The source failed front-end analysis (lexing, parsing, semantic
    /// analysis, type checking, HIR lowering, MIR lowering, or MIR
    /// optimization); the report carries every diagnostic.
    FrontEnd(Box<CheckReport>),
    /// The optimized MIR could not be compiled by the backend; every
    /// backend diagnostic is included.
    Backend(Box<[BackendError]>),
    /// The executable image could not be written to disk.
    Output {
        /// The path that could not be written.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::FrontEnd(report) => {
                write!(f, "{} front-end error(s)", report.errors.len())
            }
            Self::Backend(errors) => {
                write!(f, "{} backend error(s)", errors.len())
            }
            Self::Output { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::Output { source, .. } => Some(source),
            Self::FrontEnd(_) | Self::Backend(_) => None,
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
    /// The lowered, structurally validated, and optimized MIR, present when
    /// HIR lowering succeeded and MIR lowering, validation, and
    /// optimization reported no errors. `None` otherwise.
    pub mir: Option<MirProgram>,
}

/// Loads `path` and runs lexical, syntactic, semantic, type, HIR, MIR, and
/// MIR-optimization analysis over it.
///
/// On success returns a [`CheckReport`] describing the token stream, any
/// errors across all stages, and — when the source is lexically and
/// syntactically valid — the [`SemanticResult`], [`TypeResult`], lowered
/// [`HirProgram`], and (for a clean pipeline) lowered, validated, and
/// optimized [`MirProgram`] of analyzing the parsed AST. The caller decides
/// how to surface them. An I/O failure to read the file is reported as
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
/// lowering succeeded; the lowered MIR is structurally validated and then
/// optimized (with validation before the first pass and after every pass)
/// before it is reported; a failure on clean HIR is an internal compiler
/// error and is reported as such (`E-M01`…`E-M11`).
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
                        Ok(mir_program) => match mir::optimize(&mir_program) {
                            // `optimize` validates before the first pass and
                            // after every pass, so malformed or corrupted
                            // MIR surfaces here as structured errors.
                            Ok(optimized) => Some(optimized),
                            Err(optimization_errors) => {
                                errors.extend(optimization_errors.into_iter().map(CheckError::Mir));
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

/// Runs the full compiler pipeline for a single MINK source file and writes
/// a native executable.
///
/// Runs the same front end as [`check`]; when the front end is clean, the
/// optimized MIR is compiled by the backend for `options.target` and the
/// resulting executable image is written next to the source (the source
/// path with its extension replaced by `exe`). On success returns the
/// [`BuildOutcome`] describing the artifact. Front-end, backend, and
/// output failures are reported as [`BuildError`] variants.
pub fn build(
    sources: &mut SourceMap,
    path: &Path,
    options: BuildOptions,
) -> Result<BuildOutcome, BuildError> {
    let report = check(sources, path)?;
    if !report.errors.is_empty() {
        return Err(BuildError::FrontEnd(Box::new(report)));
    }
    let mir = report
        .mir
        .expect("a clean front end always lowers, validates, and optimizes to MIR");
    let image = backend::compile(&mir, sources, options.target)
        .map_err(|errors| BuildError::Backend(errors.into_boxed_slice()))?;
    let output = executable_path(path);
    std::fs::write(&output, &image.bytes).map_err(|source| BuildError::Output {
        path: output.clone(),
        source,
    })?;
    Ok(BuildOutcome {
        source_id: report.source_id,
        output,
        target: options.target,
        functions: image.functions,
        statics: image.statics,
    })
}

/// The executable path for a source file: the source path with its
/// extension replaced by `exe` (`foo.mink` → `foo.exe`, `main` → `main.exe`).
fn executable_path(path: &Path) -> PathBuf {
    path.with_extension("exe")
}
