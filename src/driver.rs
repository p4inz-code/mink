//! Compiler pipeline orchestration.
//!
//! Owns the sequence Source → Lexer → Parser → AST → Semantic Analysis →
//! Backend (see `docs/compiler/COMPILER_ARCHITECTURE.md` §2). The driver runs
//! source loading plus lexical, syntactic, and semantic analysis: the parser
//! consumes the token stream and produces the AST, and when the source is
//! lexically and syntactically valid, the semantic analyzer validates it and
//! reports semantic problems together with any lexical/syntax errors. Type
//! checking and the backend are not yet implemented.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::lexer::LexError;
use crate::parser::{self, ParseError};
use crate::semantics::{self, SemanticError, SemanticResult};
use crate::source::{SourceId, SourceMap, Span};

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

/// A single problem found by `check`: a lexical, a syntax, or a semantic
/// error.
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
}

impl CheckError {
    /// The stable machine-readable code of this error (e.g. `E-L01`,
    /// `E-P03`, `E-S01`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Lex(error) => error.kind().code(),
            Self::Parse(error) => error.kind().code(),
            Self::Semantic(error) => error.kind().code(),
        }
    }

    /// The source span this error applies to.
    pub fn span(&self) -> Span {
        match self {
            Self::Lex(error) => error.span(),
            Self::Parse(error) => error.span(),
            Self::Semantic(error) => error.span(),
        }
    }

    /// A related source span, when this error references another location
    /// (for example the original declaration of a duplicate definition).
    pub fn related_span(&self) -> Option<Span> {
        match self {
            Self::Semantic(error) => error.original(),
            Self::Lex(_) | Self::Parse(_) => None,
        }
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => error.fmt(f),
            Self::Parse(error) => error.fmt(f),
            Self::Semantic(error) => error.fmt(f),
        }
    }
}

/// The result of running lexical, syntactic, and (where applicable) semantic
/// analysis on one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    /// The id of the checked source file.
    pub source_id: SourceId,
    /// Number of tokens produced, excluding the final `Eof` token.
    pub token_count: usize,
    /// Lexical, syntax, and semantic errors, in source order. Empty for
    /// valid input.
    pub errors: Vec<CheckError>,
    /// The semantic-analysis result, present when the source was lexically
    /// and syntactically valid and analysis therefore ran. `None` when
    /// lexical or syntax errors made analysis unsafe or meaningless.
    pub semantic: Option<SemanticResult>,
}

/// Loads `path` and runs lexical, syntactic, and semantic analysis over it.
///
/// On success returns a [`CheckReport`] describing the token stream, any
/// lexical/syntax/semantic errors, and — when the source is lexically and
/// syntactically valid — the [`SemanticResult`] of analyzing the parsed AST.
/// The caller decides how to surface them. An I/O failure to read the file is
/// reported as [`BuildError::Io`].
///
/// Semantic analysis runs only when parsing produced a usable AST (no
/// lexical or syntax errors); otherwise the existing error behavior is
/// preserved and no cascading semantic diagnostics are generated.
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
    // Semantic analysis only when the source is lexically and syntactically
    // valid; a broken token stream or tree makes further analysis unsafe or
    // meaningless, and skipping it avoids cascades.
    let semantic = if parsed.is_valid() {
        let result = semantics::analyze(parsed.ast());
        errors.extend(result.errors().iter().cloned().map(CheckError::Semantic));
        Some(result)
    } else {
        None
    };
    // Report problems in source order regardless of which stage produced them
    // (a stable sort keeps equal-position errors in stage order).
    errors.sort_by_key(|error| error.span().start());
    Ok(CheckReport {
        source_id,
        token_count: parsed.token_count(),
        errors,
        semantic,
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
