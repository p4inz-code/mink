//! Compiler pipeline orchestration.
//!
//! Owns the sequence Source → Lexer → Parser → AST → Semantic Analysis →
//! Backend (see `docs/compiler/COMPILER_ARCHITECTURE.md` §2). At this stage
//! the driver runs source loading plus lexical and syntactic analysis: the
//! parser consumes the token stream and produces the AST, and all lexical
//! and syntax errors are reported together. Semantic analysis, type checking,
//! and the backend are not yet implemented.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::lexer::LexError;
use crate::parser::{self, ParseError};
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

/// A single problem found by `check`: either a lexical or a syntax error.
///
/// Both kinds carry a stable code, a human-readable message, and the exact
/// source span they apply to, so the CLI (and future diagnostic engine) can
/// render them uniformly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckError {
    /// A lexical error produced by the lexer.
    Lex(LexError),
    /// A syntax error produced by the parser.
    Parse(ParseError),
}

impl CheckError {
    /// The stable machine-readable code of this error (e.g. `E-L01`,
    /// `E-P03`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Lex(error) => error.kind().code(),
            Self::Parse(error) => error.kind().code(),
        }
    }

    /// The source span this error applies to.
    pub fn span(&self) -> Span {
        match self {
            Self::Lex(error) => error.span(),
            Self::Parse(error) => error.span(),
        }
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => error.fmt(f),
            Self::Parse(error) => error.fmt(f),
        }
    }
}

/// The result of running lexical and syntactic analysis on one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    /// The id of the checked source file.
    pub source_id: SourceId,
    /// Number of tokens produced, excluding the final `Eof` token.
    pub token_count: usize,
    /// Lexical and syntax errors, in source order. Empty for valid input.
    pub errors: Vec<CheckError>,
}

/// Loads `path` and runs lexical and syntactic analysis over it.
///
/// On success returns a [`CheckReport`] describing the token stream, the
/// parsed AST, and any lexical or syntax errors; the caller decides how to
/// surface them. An I/O failure to read the file is reported as
/// [`BuildError::Io`].
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
    // Report problems in source order regardless of which stage produced them
    // (a stable sort keeps equal-position errors in stage order).
    errors.sort_by_key(|error| error.span().start());
    Ok(CheckReport {
        source_id,
        token_count: parsed.token_count(),
        errors,
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
